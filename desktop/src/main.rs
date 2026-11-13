mod internet;
mod library;
mod lyrics;
mod theme;
mod video;
mod views;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::prelude::*;
use gpui::*;
use serde::{Deserialize, Serialize};

use flacplayer::engine::Cmd;
use flacplayer::lyrics::Lyrics;
use flacplayer::player::{Player, Snapshot};
use flacplayer::shared::{STATUS_ENDED, STATUS_ERROR, STATUS_PAUSED, STATUS_PLAYING};

use internet::{InternetMsg, InternetResult, existing_cached, spawn_download, spawn_search, thumbs_dir};
use library::{Library, ScanMsg, Track};
use video::VideoState;
use lyrics::{LyricsMode, LyricsRequest, LyricsResult, LyricsStatus};
use theme::*;

const ART_CACHE_LIMIT: usize = 12;
const ART_MAX_SIDE: u32 = 384;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AppState {
    last_track: Option<String>,
    last_position_ms: u64,
    playlists: Vec<PlaylistState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistState {
    pub name: String,
    pub tracks: Vec<PathBuf>,
}

fn app_state_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("flacboxdesktop")
        .join("state.json")
}

fn load_app_state() -> AppState {
    fs::read_to_string(app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_app_state(state: &AppState) {
    let path = app_state_path();
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    if let Ok(raw) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, raw);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum NpMode {
    #[default]
    Cover,
    Lyrics,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Songs,
    Albums,
    Artists,
    Internet,
}

pub struct Drill {
    pub title: String,
    pub subtitle: String,
    pub tracks: Vec<usize>,
}

pub struct FlacBox {
    pub player: Arc<Player>,
    pub lib: Library,
    pub scan_rx: Receiver<ScanMsg>,
    pub folders: Vec<PathBuf>,
    pub tab: Tab,
    pub drill: Option<Drill>,
    pub search: String,
    pub search_focus: FocusHandle,
    pub snap: Snapshot,
    pub queue: Vec<PathBuf>,
    pub shuffle: bool,
    pub repeat: u8,
    pub volume: f32,
    pub muted: bool,
    volume_before_mute: f32,
    pub show_queue: bool,
    pub show_player: bool,
    pub np_mode: NpMode,
    pub playlists: Vec<PlaylistState>,
    pub playlist_menu: Option<PathBuf>,
    pub playlist_modal_open: bool,
    pub playlist_input: String,
    pub playlist_focus: FocusHandle,
    pub add_folder_open: bool,
    pub folder_input: String,
    pub folder_focus: FocusHandle,
    pub art_cache: HashMap<PathBuf, Option<Arc<Image>>>,
    art_colors: HashMap<PathBuf, Option<u32>>,
    pub internet: InternetState,
    pub internet_track: Option<Track>,
    pub internet_titles: HashMap<PathBuf, String>,
    pub video: VideoState,
    video_tx: Sender<video::VideoMsg>,
    video_rx: Receiver<video::VideoMsg>,
    pending_video: Option<String>,
    last_video_pos: Option<i64>,
    internet_url: Option<String>,
    art_loading: HashSet<PathBuf>,
    art_tx: Sender<ArtResult>,
    art_rx: Receiver<ArtResult>,
    pub status_line: Option<String>,
    pub lyrics: Option<Lyrics>,
    pub lyrics_status: LyricsStatus,
    pub lyrics_mode: LyricsMode,
    lyrics_tx: Sender<LyricsResult>,
    lyrics_rx: Receiver<LyricsResult>,
    lyrics_key: Option<String>,
    lyrics_track: Option<String>,
    lyrics_active: Option<usize>,
    last_pos: i64,
    last_status: u8,
    last_index: i32,
    visible_cache: Option<Arc<Vec<usize>>>,
    visible_cache_revision: u64,
    navigation_revision: u64,
    restored_session: bool,
    last_saved_track: Option<PathBuf>,
}

struct ArtResult {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
    color: Option<u32>,
}

pub struct InternetState {
    pub focus: FocusHandle,
    pub query: String,
    pub results: Vec<InternetResult>,
    pub searching: bool,
    pub error: Option<String>,
    pub downloading: HashSet<String>,
    pub downloaded: HashSet<String>,
    pub downloading_video: HashSet<String>,
    pending_play: Option<String>,
    internet_tx: Sender<InternetMsg>,
    internet_rx: Receiver<InternetMsg>,
}

impl FlacBox {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let start = std::env::var("FLACBOX_START_VIEW").unwrap_or_default();
        let saved = load_app_state();
        let player = Arc::new(Player::spawn().expect("no se pudo arrancar el motor de audio"));
        let (tx, rx) = unbounded();
        let (lyrics_tx, lyrics_rx) = unbounded();
        let (art_tx, art_rx) = unbounded();
        let (internet_tx, internet_rx) = unbounded();
        let (video_tx, video_rx) = unbounded();

        let mut folders = library::default_folders();
        folders.extend(read_extra_folders());
        folders.dedup();

        library::spawn_scan(folders.clone(), tx);

        let mut this = Self {
            player,
            lib: Library {
                scanning: true,
                ..Default::default()
            },
            scan_rx: rx,
            folders,
            tab: match start.as_str() {
                "albums" => Tab::Albums,
                "artists" => Tab::Artists,
                "internet" => Tab::Internet,
                _ => Tab::Songs,
            },
            drill: None,
            search: String::new(),
            search_focus: cx.focus_handle(),
            snap: Snapshot::default(),
            queue: Vec::new(),
            shuffle: false,
            repeat: 0,
            volume: 1.0,
            muted: false,
            volume_before_mute: 1.0,
            show_queue: start == "queue",
            show_player: std::env::var_os("FLACBOX_START_PLAYER").is_some(),
            np_mode: match std::env::var("FLACBOX_START_MODE").ok().as_deref() {
                Some("lyrics") => NpMode::Lyrics,
                _ => NpMode::Cover,
            },
            playlists: saved.playlists,
            playlist_menu: None,
            playlist_modal_open: false,
            playlist_input: String::new(),
            playlist_focus: cx.focus_handle(),
            add_folder_open: false,
            folder_input: String::new(),
            folder_focus: cx.focus_handle(),
            art_cache: HashMap::new(),
            art_colors: HashMap::new(),
            internet: InternetState {
                focus: cx.focus_handle(),
                query: String::new(),
                results: Vec::new(),
                searching: false,
                error: None,
                downloading: HashSet::new(),
                downloaded: HashSet::new(),
                downloading_video: HashSet::new(),
                pending_play: None,
                internet_tx,
                internet_rx,
            },
            internet_track: None,
            internet_titles: HashMap::new(),
            video: VideoState::new(),
            video_tx,
            video_rx,
            pending_video: None,
            last_video_pos: None,
            internet_url: None,
            art_loading: HashSet::new(),
            art_tx,
            art_rx,
            status_line: None,
            lyrics: None,
            lyrics_status: LyricsStatus::Idle,
            lyrics_mode: LyricsMode::Auto,
            lyrics_tx,
            lyrics_rx,
            lyrics_key: None,
            lyrics_track: None,
            lyrics_active: None,
            last_pos: -1,
            last_status: 255,
            last_index: -1,
            visible_cache: None,
            visible_cache_revision: 0,
            navigation_revision: 1,
            restored_session: false,
            last_saved_track: None,
        };

        if let Ok(query) = std::env::var("FLACBOX_START_QUERY") {
            if !query.trim().is_empty() {
                this.internet.query = query;
                this.search_internet();
            }
        }

        if let Ok(id) = std::env::var("FLACBOX_VIDEO_ID") {
            if let Some(path) = video::existing_video(&id) {
                let result = this.internet.results.iter().find(|result| result.id == id).cloned();
                let (title, uploader, duration_ms) = result
                    .map(|r| (r.title, r.uploader, r.duration_ms))
                    .unwrap_or_else(|| ("Video de prueba".to_string(), "YouTube".to_string(), 0));
                match video::begin_video(&id, path, title, uploader, duration_ms, &mut this.video) {
                    Ok(()) => {
                        this.show_player = true;
                        this.np_mode = NpMode::Cover;
                    }
                    Err(message) => eprintln!("FLACBOX_VIDEO_ID: {message}"),
                }
            }
        }

        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                break;
            }
        })
        .detach();

        this
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let mut dirty = false;

        while let Ok(msg) = self.scan_rx.try_recv() {
            match msg {
                ScanMsg::Done(tracks) => {
                    self.lib.rebuild(tracks);
                    self.lib.scanning = false;
                    self.restore_session();
                    self.invalidate_navigation();
                    dirty = true;
                }
            }
        }

        while let Ok(msg) = self.internet.internet_rx.try_recv() {
            match msg {
                InternetMsg::SearchDone(results) => {
                    self.internet.searching = false;
                    self.internet.results = results;
                    dirty = true;
                }
                InternetMsg::SearchError(message) => {
                    self.internet.searching = false;
                    self.internet.error = Some(message.clone());
                    self.status_line = Some(message);
                    dirty = true;
                }
                InternetMsg::DownloadDone { id, path } => {
                    self.internet.downloading.remove(&id);
                    self.internet.downloaded.insert(id.clone());
                    if self.internet.pending_play.as_deref() == Some(id.as_str()) {
                        self.internet.pending_play = None;
                        self.play_cached_internet(id, path);
                    }
                    dirty = true;
                }
                InternetMsg::DownloadError { id, message } => {
                    self.internet.downloading.remove(&id);
                    self.internet.pending_play = None;
                    self.internet.error = Some(message.clone());
                    self.status_line = Some(format!("No se pudo descargar: {message}"));
                    dirty = true;
                }
            }
        }

        while let Ok(msg) = self.video_rx.try_recv() {
            match msg {
                video::VideoMsg::Ready { id, path } => {
                    self.internet.downloading_video.remove(&id);
                    if self.pending_video.as_deref() == Some(id.as_str()) {
                        self.pending_video = None;
                        self.start_cached_video(id, path);
                    }
                    dirty = true;
                }
                video::VideoMsg::Failed { id, message } => {
                    self.internet.downloading_video.remove(&id);
                    self.pending_video = None;
                    self.status_line = Some(format!("No se pudo descargar el video: {message}"));
                    dirty = true;
                }
            }
        }

        while let Ok(result) = self.art_rx.try_recv() {
            self.art_loading.remove(&result.path);
            let image = result.bytes.and_then(|bytes| {
                let format = if bytes.starts_with(&[0xFF, 0xD8]) {
                    ImageFormat::Jpeg
                } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    ImageFormat::Png
                } else {
                    return None;
                };
                Some(Arc::new(Image::from_bytes(format, bytes)))
            });
            if self.art_cache.len() >= ART_CACHE_LIMIT && !self.art_cache.contains_key(&result.path)
            {
                if let Some(oldest) = self.art_cache.keys().next().cloned() {
                    self.art_cache.remove(&oldest);
                    self.art_colors.remove(&oldest);
                }
            }
            self.art_colors.insert(result.path.clone(), result.color);
            self.art_cache.insert(result.path, image);
            dirty = true;
        }

        while let Ok(evt) = self.player.events.try_recv() {
            if evt.kind == flacplayer::shared::EVENT_ERROR {
                self.status_line = evt.text.clone();
            }
            if evt.kind == flacplayer::shared::EVENT_QUEUE_ENDED {
                self.status_line = None;
            }
            dirty = true;
        }

        let snap = self.player.snapshot();
        if snap.position_ms / 250 != self.last_pos / 250
            || snap.status != self.last_status
            || snap.index != self.last_index
        {
            dirty = true;
        }
        self.last_pos = snap.position_ms;
        self.last_status = snap.status;
        self.last_index = snap.index;
        self.snap = snap;
        self.persist_last_track();

        self.update_lyrics(cx);
        self.update_lyrics_position(cx);

        if !self.show_player && self.video.active {
            self.stop_video();
        }

        let video_pos = self
            .video
            .video
            .as_ref()
            .map(|video| video.position().as_millis() as i64);
        if video_pos != self.last_video_pos {
            self.last_video_pos = video_pos;
            dirty = true;
        }

        if dirty {
            cx.notify();
        }
    }

    /// Pide la letra cuando cambia la pista y recoge el resultado del hilo.
    fn update_lyrics(&mut self, cx: &mut Context<Self>) {
        while let Ok(result) = self.lyrics_rx.try_recv() {
            if self.lyrics_key.as_deref() != Some(result.key.as_str()) {
                continue;
            }
            self.lyrics = result.lyrics;
            self.lyrics_status = match &self.lyrics {
                Some(l) if !l.is_empty() => LyricsStatus::Ready,
                _ => LyricsStatus::Empty,
            };
            self.lyrics_active = None;
            cx.notify();
        }

        let track = self.current_track().cloned();
        let identity = track
            .as_ref()
            .map(|t| format!("{}::{:?}", t.path.to_string_lossy(), self.lyrics_mode));
        if identity == self.lyrics_track {
            return;
        }
        self.lyrics_track = identity;
        self.lyrics = None;
        self.lyrics_active = None;
        self.lyrics_key = None;

        let Some(track) = track else {
            self.lyrics_status = LyricsStatus::Idle;
            cx.notify();
            return;
        };

        self.lyrics_status = LyricsStatus::Loading;
        let key = format!(
            "{}::{:?}",
            flacplayer::lyrics::cache_key(&track.title, &track.artist, &track.album),
            self.lyrics_mode
        );
        self.lyrics_key = Some(key.clone());
        lyrics::spawn_resolve(
            LyricsRequest {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_ms: track.duration_ms as i64,
                audio_path: Some(track.path.clone()),
            },
            key,
            self.lyrics_mode,
            self.lyrics_tx.clone(),
        );
        cx.notify();
    }

    /// Línea de letra activa y auto-scroll del panel para mantenerla visible.
    fn update_lyrics_position(&mut self, cx: &mut Context<Self>) {
        let Some(lyrics) = &self.lyrics else { return };
        if !lyrics.synced {
            return;
        }
        let position_ms = match &self.video.video {
            Some(video) => video.position().as_millis() as i64,
            None => self.snap.position_ms,
        };
        let active = lyrics.active_index(position_ms);
        if active == self.lyrics_active {
            return;
        }
        self.lyrics_active = active;
        cx.notify();
    }

    pub fn is_playing(&self) -> bool {
        self.snap.status == STATUS_PLAYING
    }

    pub fn current_track(&self) -> Option<&Track> {
        let idx = self.snap.index;
        if idx < 0 {
            return None;
        }
        let path = self.queue.get(idx as usize)?;
        if let Some(track) = self.lib.tracks.iter().find(|track| &track.path == path) {
            return Some(track);
        }
        self.internet_track.as_ref().filter(|track| &track.path == path)
    }

    fn restore_session(&mut self) {
        if self.restored_session {
            return;
        }
        self.restored_session = true;
        let state = load_app_state();
        let Some(last) = state.last_track else {
            return;
        };
        let last = PathBuf::from(last);
        let Some(start) = self.lib.tracks.iter().position(|track| track.path == last) else {
            return;
        };
        self.last_saved_track = Some(last.clone());
        let paths = vec![self.lib.tracks[start].path.to_string_lossy().to_string()];
        self.queue = vec![self.lib.tracks[start].path.clone()];
        self.player.send(Cmd::SetQueue {
            paths,
            start: 0,
            start_ms: state.last_position_ms,
        });
    }

    fn persist_last_track(&mut self) {
        let Some(path) = self.current_track().map(|track| track.path.clone()) else {
            return;
        };
        if !self.lib.tracks.iter().any(|track| track.path == path) {
            return;
        }
        if self.last_saved_track.as_ref() == Some(&path) {
            return;
        }
        self.last_saved_track = Some(path.clone());
        let mut state = load_app_state();
        state.last_track = Some(path.to_string_lossy().to_string());
        state.last_position_ms = self.snap.position_ms.max(0) as u64;
        state.playlists = self.playlists.clone();
        save_app_state(&state);
    }

    pub fn create_playlist(&mut self) {
        let name = self.playlist_input.trim();
        if name.is_empty() {
            return;
        }
        if self.playlists.iter().any(|item| item.name == name) {
            self.status_line = Some(format!("Ya existe una playlist llamada \"{name}\""));
            return;
        }
        self.playlists.push(PlaylistState {
            name: name.to_string(),
            tracks: Vec::new(),
        });
        self.playlist_input.clear();
        self.playlist_modal_open = false;
        let mut state = load_app_state();
        state.playlists = self.playlists.clone();
        save_app_state(&state);
    }

    pub fn add_to_playlist(&mut self, playlist_name: &str, path: PathBuf) {
        if let Some(playlist) = self
            .playlists
            .iter_mut()
            .find(|item| item.name == playlist_name)
        {
            if !playlist.tracks.contains(&path) {
                playlist.tracks.push(path);
            }
        }
        self.playlist_menu = None;
        let mut state = load_app_state();
        state.playlists = self.playlists.clone();
        save_app_state(&state);
    }

    pub fn visible_tracks(&mut self) -> Arc<Vec<usize>> {
        if self.visible_cache_revision == self.navigation_revision {
            if let Some(cache) = &self.visible_cache {
                return Arc::clone(cache);
            }
        }

        let q = self.search.trim().to_lowercase();

        if let Some(drill) = &self.drill {
            if q.is_empty() {
                return self.cache_visible(drill.tracks.clone());
            }
            return self.cache_visible(
                drill
                    .tracks
                    .iter()
                    .copied()
                    .filter(|&i| self.matches(i, &q))
                    .collect(),
            );
        }

        let base: Vec<usize> = match self.tab {
            Tab::Songs | Tab::Albums | Tab::Artists | Tab::Internet => {
                (0..self.lib.tracks.len()).collect()
            }
        };

        let visible = if q.is_empty() {
            base
        } else {
            base.into_iter().filter(|&i| self.matches(i, &q)).collect()
        };
        self.cache_visible(visible)
    }

    fn cache_visible(&mut self, visible: Vec<usize>) -> Arc<Vec<usize>> {
        let visible = Arc::new(visible);
        self.visible_cache = Some(Arc::clone(&visible));
        self.visible_cache_revision = self.navigation_revision;
        visible
    }

    fn invalidate_navigation(&mut self) {
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.visible_cache = None;
    }

    pub fn set_search(&mut self, search: String) {
        self.search = search;
        self.invalidate_navigation();
    }

    pub fn invalidate_search(&mut self) {
        self.invalidate_navigation();
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.drill = None;
        if tab != Tab::Albums {
            self.art_cache.clear();
            self.art_colors.clear();
            self.art_loading.clear();
        }
        self.invalidate_navigation();
    }

    pub fn open_drill(&mut self, drill: Drill) {
        self.drill = Some(drill);
        self.invalidate_navigation();
    }

    pub fn open_playlist(&mut self, index: usize) {
        let Some(playlist) = self.playlists.get(index) else {
            return;
        };
        let name = playlist.name.clone();
        let tracks: Vec<usize> = playlist
            .tracks
            .iter()
            .filter_map(|path| self.lib.tracks.iter().position(|track| &track.path == path))
            .collect();
        if tracks.is_empty() {
            self.status_line = Some(format!("La playlist \"{name}\" está vacía"));
            return;
        }
        self.open_drill(Drill {
            title: name,
            subtitle: "Playlist".to_string(),
            tracks,
        });
    }

    pub fn clear_drill(&mut self) {
        self.drill = None;
        self.invalidate_navigation();
    }

    pub fn clear_search(&mut self) {
        if !self.search.is_empty() {
            self.search.clear();
            self.invalidate_navigation();
        }
    }

    fn matches(&self, i: usize, q: &str) -> bool {
        self.lib.tracks[i].search_key.contains(q)
    }

    pub fn search_internet(&mut self) {
        let query = self.internet.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.internet.searching = true;
        self.internet.error = None;
        self.internet.results.clear();
        spawn_search(query, self.internet.internet_tx.clone());
    }

    pub fn play_internet(&mut self, index: usize) {
        let Some(result) = self.internet.results.get(index) else {
            return;
        };
        let id = result.id.clone();
        let url = result.url.clone();
        self.internet_url = Some(url.clone());
        if let Some(path) = existing_cached(&id) {
            self.play_cached_internet(id, path);
            return;
        }
        self.internet.downloading.insert(id.clone());
        self.internet.pending_play = Some(id.clone());
        spawn_download(id, url, self.internet.internet_tx.clone());
    }

    fn play_cached_internet(&mut self, id: String, path: PathBuf) {
        let Some(result) = self
            .internet
            .results
            .iter()
            .find(|result| result.id == id)
            .cloned()
        else {
            return;
        };
        let title = result.title.clone();
        if let Ok(bytes) = std::fs::read(thumbs_dir().join(format!("{id}.jpg"))) {
            let color = dominant_color(&bytes);
            let bytes = shrink_art(&bytes);
            let format = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            };
            self.art_cache
                .insert(path.clone(), Some(Arc::new(Image::from_bytes(format, bytes))));
            self.art_colors.insert(path.clone(), color);
        }
        let track = Track {
            path: path.clone(),
            title: title.clone(),
            artist: result.uploader,
            album: "YouTube".to_string(),
            album_artist: String::new(),
            year: None,
            duration_ms: result.duration_ms,
            track_no: 0,
            disc_no: 0,
            sample_rate: 0,
            bits: 0,
            channels: 2,
            has_art: true,
            search_key: title.to_lowercase(),
        };
        self.internet_titles.insert(path.clone(), title);
        self.internet_track = Some(track);
        self.internet.downloaded.insert(id);
        self.queue = vec![path.clone()];
        let paths = vec![path.to_string_lossy().to_string()];
        self.player.send(Cmd::SetQueue {
            paths,
            start: 0,
            start_ms: 0,
        });
        self.player.send(Cmd::Play);
        self.status_line = None;
    }

    pub fn queue_label(&self, index: usize) -> String {
        let Some(path) = self.queue.get(index) else {
            return String::new();
        };
        if let Some(title) = self.internet_titles.get(path) {
            return title.clone();
        }
        if let Some(track) = &self.internet_track {
            if &track.path == path {
                return track.title.clone();
            }
        }
        path.file_stem()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    pub fn internet_thumb(&self, id: &str) -> PathBuf {
        thumbs_dir().join(format!("{id}.jpg"))
    }

    pub fn np_track(&self) -> Option<&Track> {
        if let Some(track) = &self.video.track {
            return Some(track);
        }
        self.current_track()
    }

    pub fn is_internet_current(&self) -> bool {
        self.np_track().is_some_and(|track| track.album == "YouTube")
    }

    /// Vuelve a reproducir el audio (M4A) de la pista actual de Internet.
    pub fn start_audio_internet(&mut self) {
        let Some(track) = self.np_track().cloned() else {
            return;
        };
        let Some(id) = internet_id_of(&track) else {
            return;
        };
        self.stop_video();
        if let Some(path) = existing_cached(&id) {
            let paths = vec![path.to_string_lossy().to_string()];
            self.queue = vec![path];
            self.player.send(Cmd::SetQueue {
                paths,
                start: 0,
                start_ms: 0,
            });
            self.player.send(Cmd::Play);
            return;
        }
        if let Some(url) = &self.internet_url {
            let url = url.clone();
            self.internet.downloading.insert(id.clone());
            self.internet.pending_play = Some(id.clone());
            spawn_download(id, url, self.internet.internet_tx.clone());
        }
    }

    /// Reproduce el video de la pista actual de Internet.
    pub fn start_video_internet(&mut self) {
        let Some(track) = self.np_track().cloned() else {
            return;
        };
        let Some(id) = internet_id_of(&track) else {
            return;
        };
        self.stop_video();
        if let Some(path) = video::existing_video(&id) {
            self.start_cached_video(id, path);
            return;
        }
        if let Some(url) = &self.internet_url {
            let url = url.clone();
            self.player.send(Cmd::Stop);
            self.internet.downloading_video.insert(id.clone());
            self.pending_video = Some(id.clone());
            video::spawn_download_video(id, url, self.video_tx.clone());
        }
    }

    pub fn play_video(&mut self, index: usize) {
        let Some(result) = self.internet.results.get(index).cloned() else {
            return;
        };
        let id = result.id.clone();
        let url = result.url.clone();
        self.internet_url = Some(url.clone());
        if let Some(path) = video::existing_video(&id) {
            self.start_cached_video(id, path);
            return;
        }
        self.internet.downloading_video.insert(id.clone());
        self.pending_video = Some(id.clone());
        video::spawn_download_video(id, url, self.video_tx.clone());
    }

    fn start_cached_video(&mut self, id: String, path: PathBuf) {
        let Some(result) = self
            .internet
            .results
            .iter()
            .find(|result| result.id == id)
            .cloned()
        else {
            return;
        };
        self.player.send(Cmd::Stop);
        let title = result.title.clone();
        match video::begin_video(
            &id,
            path,
            title,
            result.uploader,
            result.duration_ms,
            &mut self.video,
        ) {
            Ok(()) => {
                self.show_player = true;
                self.np_mode = NpMode::Cover;
                self.status_line = None;
            }
            Err(message) => {
                self.status_line = Some(message);
            }
        }
    }

    pub fn stop_video(&mut self) {
        self.video.video = None;
        self.video.track = None;
        self.video.active = false;
    }

    pub fn play_tracks(&mut self, tracks: &[usize], start: usize) {
        if tracks.is_empty() {
            return;
        }
        let start = start.min(tracks.len() - 1);
        let paths: Vec<String> = tracks
            .iter()
            .map(|&i| self.lib.tracks[i].path.to_string_lossy().to_string())
            .collect();
        self.queue = tracks
            .iter()
            .map(|&i| self.lib.tracks[i].path.clone())
            .collect();
        self.player.send(Cmd::SetQueue {
            paths,
            start,
            start_ms: 0,
        });
        self.player.send(Cmd::Play);
    }

    pub fn toggle_play(&mut self) {
        match self.snap.status {
            STATUS_PLAYING => self.player.send(Cmd::Pause),
            STATUS_PAUSED | STATUS_ENDED => self.player.send(Cmd::Play),
            STATUS_ERROR => {
                self.status_line = None;
                self.player.send(Cmd::Play);
            }
            _ => self.player.send(Cmd::Play),
        }
    }

    pub fn seek_to(&mut self, ms: i64) {
        if self.snap.duration_ms > 0 {
            let ms = ms.clamp(0, self.snap.duration_ms);
            self.player.send(Cmd::Seek(ms as u64));
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = self.volume <= 0.0;
        if !self.muted {
            self.volume_before_mute = self.volume;
        }
        self.player.send(Cmd::SetVolume(self.volume));
    }

    pub fn toggle_mute(&mut self) {
        if self.muted || self.volume <= 0.0 {
            self.muted = false;
            self.volume = self.volume_before_mute.max(0.1);
        } else {
            self.volume_before_mute = self.volume;
            self.muted = true;
            self.volume = 0.0;
        }
        self.player.send(Cmd::SetVolume(self.volume));
    }

    pub fn current_art_color(&self) -> Option<u32> {
        let path = self.current_track().map(|track| track.path.clone())?;
        self.art_colors.get(&path).copied().flatten()
    }

    pub fn lyrics_available(&self) -> bool {
        self.lyrics
            .as_ref()
            .is_some_and(|lyrics| lyrics.synced && !lyrics.is_empty())
    }

    pub fn toggle_np_mode(&mut self) {
        if !self.lyrics_available() {
            return;
        }
        self.np_mode = match self.np_mode {
            NpMode::Cover => NpMode::Lyrics,
            NpMode::Lyrics => NpMode::Cover,
        };
    }

    pub fn art(&mut self, path: &PathBuf) -> Option<Arc<Image>> {
        if let Some(cached) = self.art_cache.get(path) {
            return cached.clone();
        }
        if self.art_loading.insert(path.clone()) {
            let path = path.clone();
            let tx = self.art_tx.clone();
            std::thread::Builder::new()
                .name("flacbox-art".into())
                .spawn(move || {
                    let is_image = matches!(
                        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_lowercase()).as_deref(),
                        Some("jpg") | Some("jpeg") | Some("png")
                    );
                    let bytes = if is_image {
                        std::fs::read(&path).ok()
                    } else {
                        flacplayer::tags::artwork(&path)
                    };
                    let color = bytes.as_deref().and_then(dominant_color);
                    let bytes = bytes.map(|bytes| shrink_art(&bytes));
                    let _ = tx.send(ArtResult { path, bytes, color });
                })
                .ok();
        }
        None
    }

    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        self.lib.scanning = true;
        self.art_cache.clear();
        self.art_colors.clear();
        self.art_loading.clear();
        self.invalidate_navigation();
        let (tx, rx) = unbounded();
        self.scan_rx = rx;
        library::spawn_scan(self.folders.clone(), tx);
        cx.notify();
    }

    pub fn add_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !path.is_dir() {
            self.status_line = Some(format!(
                "La carpeta no existe o no es un directorio: {}",
                path.display()
            ));
            return;
        }
        if self.folders.contains(&path) {
            self.status_line = Some("Esa carpeta ya está añadida".to_string());
            return;
        }
        self.folders.push(path);
        write_extra_folders(&self.folders);
        self.add_folder_open = false;
        self.folder_input.clear();
        self.status_line = None;
        self.rescan(cx);
    }
}

fn shrink_art(bytes: &[u8]) -> Vec<u8> {
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img,
        Err(_) => return bytes.to_vec(),
    };
    let (w, h) = (img.width(), img.height());
    let max = w.max(h);
    if max <= ART_MAX_SIDE {
        return bytes.to_vec();
    }
    let ratio = ART_MAX_SIDE as f32 / max as f32;
    let nw = ((w as f32 * ratio).round() as u32).max(1);
    let nh = ((h as f32 * ratio).round() as u32).max(1);
    let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    match resized.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => bytes.to_vec(),
    }
}

fn dominant_color(bytes: &[u8]) -> Option<u32> {
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    let total = width as usize * height as usize;
    let step = (total / 1500).max(1);
    let mut r = 0.0f64;
    let mut g = 0.0f64;
    let mut b = 0.0f64;
    let mut wsum = 0.0f64;
    for (i, pixel) in img.pixels().enumerate() {
        if i % step != 0 {
            continue;
        }
        let [pr, pg, pb, pa] = pixel.0;
        if pa < 128 {
            continue;
        }
        let max = pr.max(pg).max(pb) as f64;
        let min = pr.min(pg).min(pb) as f64;
        let sat = if max <= 0.0 { 0.0 } else { (max - min) / max };
        let weight = if sat > 0.12 && max > 30.0 {
            0.25 + sat
        } else {
            0.08
        };
        r += pr as f64 * weight;
        g += pg as f64 * weight;
        b += pb as f64 * weight;
        wsum += weight;
    }
    if wsum == 0.0 {
        return None;
    }
    Some((((r / wsum).round() as u32) << 16) | (((g / wsum).round() as u32) << 8) | (b / wsum).round() as u32)
}

fn internet_id_of(track: &Track) -> Option<String> {
    track
        .path
        .file_stem()
        .and_then(|stem| stem.to_str().map(String::from))
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/flacbox/folders")
}

fn read_extra_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Ok(content) = std::fs::read_to_string(config_path()) {
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() {
                let p = PathBuf::from(line);
                if p.is_dir() {
                    folders.push(p);
                }
            }
        }
    }
    folders
}

fn write_extra_folders(folders: &[PathBuf]) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let body: String = folders
        .iter()
        .map(|f| format!("{}\n", f.to_string_lossy()))
        .collect();
    let _ = std::fs::write(path, body);
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(120.), px(80.)),
                size: size(px(1180.), px(760.)),
            })),
            titlebar: Some(TitlebarOptions::default()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            window.set_window_title("FlacBox");
            cx.new(FlacBox::new)
        })
        .expect("no se pudo abrir la ventana");
        cx.activate(true);
    });
}

impl Render for FlacBox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = if self.show_player {
            views::now_playing(self, window, cx)
        } else {
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(views::sidebar(self, cx))
                .child(views::main_area(self, cx))
                .into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                if this.search_focus.is_focused(_window)
                    || this.folder_focus.is_focused(_window)
                    || this.playlist_focus.is_focused(_window)
                    || this.internet.focus.is_focused(_window)
                {
                    return;
                }
                match ev.keystroke.key.as_str() {
                    "space" => this.toggle_play(),
                    "right" => this.seek_to(this.snap.position_ms + 5000),
                    "left" => this.seek_to(this.snap.position_ms - 5000),
                    "n" => this.player.send(Cmd::Next),
                    "p" => this.player.send(Cmd::Prev),
                    "1" => {
                        this.set_tab(Tab::Songs);
                        this.show_player = false;
                    }
                    "2" => {
                        this.set_tab(Tab::Albums);
                        this.show_player = false;
                    }
                    "3" => {
                        this.set_tab(Tab::Artists);
                        this.show_player = false;
                    }
                    "4" => {
                        this.set_tab(Tab::Internet);
                        this.show_player = false;
                    }
                    "f" => {
                        this.show_player = !this.show_player;
                    }
                    "l" => {
                        if this.show_player && !this.video.active {
                            this.toggle_np_mode();
                        }
                    }
                    "escape" => {
                        if this.playlist_menu.is_some() {
                            this.playlist_menu = None;
                        } else if this.video.active {
                            this.stop_video();
                        } else if this.add_folder_open {
                            this.add_folder_open = false;
                            this.folder_input.clear();
                        } else if this.playlist_modal_open {
                            this.playlist_modal_open = false;
                            this.playlist_input.clear();
                        } else if this.show_player {
                            if this.np_mode == NpMode::Lyrics {
                                this.np_mode = NpMode::Cover;
                            } else {
                                this.show_player = false;
                            }
                        } else if this.show_queue {
                            this.show_queue = false;
                        } else {
                            this.clear_drill();
                            this.clear_search();
                        }
                    }
                    _ => return,
                }
                cx.notify();
            }))
            .child(body)
            .when(!self.show_player, |el| {
                el.child(views::player_bar(self, cx))
            })
            .when(self.add_folder_open, |el| {
                el.child(views::add_folder_modal(self, cx))
            })
            .when(self.playlist_modal_open, |el| {
                el.child(views::create_playlist_modal(self, cx))
            })
    }
}
