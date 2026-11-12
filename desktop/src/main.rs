mod library;
mod lyrics;
mod theme;
mod views;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::prelude::*;
use gpui::*;

use flacplayer::engine::Cmd;
use flacplayer::lyrics::Lyrics;
use flacplayer::player::{Player, Snapshot};
use flacplayer::shared::{STATUS_ENDED, STATUS_ERROR, STATUS_PAUSED, STATUS_PLAYING};

use library::{Library, ScanMsg, Track};
use lyrics::{LyricsRequest, LyricsResult, LyricsStatus};
use theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Songs,
    Albums,
    Artists,
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
    pub show_queue: bool,
    pub show_player: bool,
    pub add_folder_open: bool,
    pub folder_input: String,
    pub folder_focus: FocusHandle,
    pub art_cache: HashMap<PathBuf, Option<Arc<Image>>>,
    pub status_line: Option<String>,
    pub lyrics: Option<Lyrics>,
    pub lyrics_status: LyricsStatus,
    pub lyrics_scroll: ScrollHandle,
    lyrics_tx: Sender<LyricsResult>,
    lyrics_rx: Receiver<LyricsResult>,
    lyrics_key: Option<String>,
    lyrics_track: Option<String>,
    lyrics_active: Option<usize>,
    last_pos: i64,
    last_status: u8,
    last_index: i32,
}

impl FlacBox {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let start = std::env::var("FLACBOX_START_VIEW").unwrap_or_default();
        let player = Arc::new(Player::spawn().expect("no se pudo arrancar el motor de audio"));
        let (tx, rx) = unbounded();
        let (lyrics_tx, lyrics_rx) = unbounded();

        let mut folders = library::default_folders();
        folders.extend(read_extra_folders());
        folders.dedup();

        library::spawn_scan(folders.clone(), tx);

        let this = Self {
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
                _ => Tab::Songs,
            },
            drill: None,
            search: String::new(),
            search_focus: cx.focus_handle(),
            snap: Snapshot::default(),
            queue: Vec::new(),
            shuffle: false,
            repeat: 0,
            show_queue: start == "queue",
            show_player: start == "player",
            add_folder_open: false,
            folder_input: String::new(),
            folder_focus: cx.focus_handle(),
            art_cache: HashMap::new(),
            status_line: None,
            lyrics: None,
            lyrics_status: LyricsStatus::Idle,
            lyrics_scroll: ScrollHandle::new(),
            lyrics_tx,
            lyrics_rx,
            lyrics_key: None,
            lyrics_track: None,
            lyrics_active: None,
            last_pos: -1,
            last_status: 255,
            last_index: -1,
        };

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
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
                    dirty = true;
                }
            }
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

        self.update_lyrics(cx);
        self.update_lyrics_position(cx);

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
        let identity = track.as_ref().map(|t| t.path.to_string_lossy().to_string());
        if identity == self.lyrics_track {
            return;
        }
        self.lyrics_track = identity;
        self.lyrics = None;
        self.lyrics_active = None;
        self.lyrics_key = None;
        self.lyrics_scroll.set_offset(point(px(0.), px(0.)));

        let Some(track) = track else {
            self.lyrics_status = LyricsStatus::Idle;
            cx.notify();
            return;
        };

        self.lyrics_status = LyricsStatus::Loading;
        let key = flacplayer::lyrics::cache_key(&track.title, &track.artist, &track.album);
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
        let active = lyrics.active_index(self.snap.position_ms);
        if active == self.lyrics_active {
            return;
        }
        self.lyrics_active = active;
        if let Some(index) = active {
            self.lyrics_scroll.scroll_to_item(index);
        }
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
        self.lib.tracks.iter().find(|t| &t.path == path)
    }

    pub fn visible_tracks(&self) -> Vec<usize> {
        let q = self.search.trim().to_lowercase();

        if let Some(drill) = &self.drill {
            if q.is_empty() {
                return drill.tracks.clone();
            }
            return drill
                .tracks
                .iter()
                .copied()
                .filter(|&i| self.matches(i, &q))
                .collect();
        }

        let base: Vec<usize> = match self.tab {
            Tab::Songs | Tab::Albums | Tab::Artists => (0..self.lib.tracks.len()).collect(),
        };

        if q.is_empty() {
            base
        } else {
            base.into_iter().filter(|&i| self.matches(i, &q)).collect()
        }
    }

    fn matches(&self, i: usize, q: &str) -> bool {
        let t = &self.lib.tracks[i];
        t.title.to_lowercase().contains(q)
            || t.artist.to_lowercase().contains(q)
            || t.album.to_lowercase().contains(q)
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
        self.queue = tracks.iter().map(|&i| self.lib.tracks[i].path.clone()).collect();
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

    pub fn art(&mut self, path: &PathBuf) -> Option<Arc<Image>> {
        if let Some(cached) = self.art_cache.get(path) {
            return cached.clone();
        }
        let image = flacplayer::tags::artwork(path).and_then(|bytes| {
            let format = if bytes.starts_with(&[0xFF, 0xD8]) {
                ImageFormat::Jpeg
            } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                ImageFormat::Png
            } else {
                return None;
            };
            Some(Arc::new(Image::from_bytes(format, bytes)))
        });
        self.art_cache.insert(path.clone(), image.clone());
        image
    }

    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        self.lib.scanning = true;
        let (tx, rx) = unbounded();
        self.scan_rx = rx;
        library::spawn_scan(self.folders.clone(), tx);
        cx.notify();
    }

    pub fn add_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !path.is_dir() || self.folders.contains(&path) {
            return;
        }
        self.folders.push(path);
        write_extra_folders(&self.folders);
        self.add_folder_open = false;
        self.folder_input.clear();
        self.rescan(cx);
    }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = if self.show_player {
            views::now_playing(self, cx).into_any_element()
        } else {
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(views::sidebar(self, cx))
                .child(views::main_area(self, cx))
                .into_any_element()
        };

        let player_bar = views::player_bar(self, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                if this.search_focus.is_focused(_window) || this.folder_focus.is_focused(_window) {
                    return;
                }
                match ev.keystroke.key.as_str() {
                    "space" => this.toggle_play(),
                    "right" => this.seek_to(this.snap.position_ms + 5000),
                    "left" => this.seek_to(this.snap.position_ms - 5000),
                    "n" => this.player.send(Cmd::Next),
                    "p" => this.player.send(Cmd::Prev),
                    "1" => {
                        this.tab = Tab::Songs;
                        this.drill = None;
                        this.show_player = false;
                    }
                    "2" => {
                        this.tab = Tab::Albums;
                        this.drill = None;
                        this.show_player = false;
                    }
                    "3" => {
                        this.tab = Tab::Artists;
                        this.drill = None;
                        this.show_player = false;
                    }
                    "f" => {
                        this.show_player = !this.show_player;
                    }
                    "escape" => {
                        if this.add_folder_open {
                            this.add_folder_open = false;
                            this.folder_input.clear();
                        } else if this.show_player {
                            this.show_player = false;
                        } else if this.show_queue {
                            this.show_queue = false;
                        } else {
                            this.drill = None;
                            this.search.clear();
                        }
                    }
                    _ => return,
                }
                cx.notify();
            }))
            .child(body)
            .child(player_bar)
            .when(self.add_folder_open, |el| el.child(views::add_folder_modal(self, cx)))
    }
}
