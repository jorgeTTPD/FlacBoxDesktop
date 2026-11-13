use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use flacplayer::tags;

const AUDIO_EXT: &[&str] = &[
    "flac", "mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "alac", "aiff", "aif", "m4b",
];

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<String>,
    pub duration_ms: u64,
    pub track_no: u32,
    pub disc_no: u32,
    pub sample_rate: u32,
    pub bits: u8,
    pub channels: u8,
    pub has_art: bool,
    pub search_key: String,
}

impl Track {
    pub fn quality(&self) -> String {
        let ext = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_uppercase();
        let ext = if ext == "M4A" { "AAC".to_string() } else { ext };
        if self.sample_rate == 0 {
            return format!("{ext} · YOUTUBE");
        }
        if self.bits > 0 {
            format!("{} {}/{}", ext, self.bits, self.sample_rate / 1000)
        } else {
            format!("{} {}", ext, self.sample_rate / 1000)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Album {
    pub name: String,
    pub artist: String,
    pub year: Option<String>,
    pub tracks: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Artist {
    pub name: String,
    pub albums: usize,
    pub tracks: Vec<usize>,
}

#[derive(Default)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub scanning: bool,
}

pub enum ScanMsg {
    Done(Vec<Track>),
}

impl Library {
    pub fn rebuild(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
        self.tracks.sort_by(|a, b| {
            a.album_artist
                .to_lowercase()
                .cmp(&b.album_artist.to_lowercase())
                .then(a.album.to_lowercase().cmp(&b.album.to_lowercase()))
                .then(a.disc_no.cmp(&b.disc_no))
                .then(a.track_no.cmp(&b.track_no))
                .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        let mut album_map: HashMap<(String, String), Album> = HashMap::new();
        let mut artist_map: HashMap<String, Artist> = HashMap::new();

        for (i, t) in self.tracks.iter().enumerate() {
            let key = (t.album.to_lowercase(), t.album_artist.to_lowercase());
            let entry = album_map.entry(key).or_insert_with(|| Album {
                name: t.album.clone(),
                artist: t.album_artist.clone(),
                year: t.year.clone(),
                tracks: Vec::new(),
            });
            entry.tracks.push(i);

            let artist = artist_map.entry(t.artist.to_lowercase()).or_default();
            artist.name = t.artist.clone();
            artist.tracks.push(i);
        }

        let mut albums: Vec<Album> = album_map.into_values().collect();
        for album in albums.iter_mut() {
            album.tracks.sort_by_key(|&i| {
                let t = &self.tracks[i];
                (t.disc_no, t.track_no, t.title.to_lowercase())
            });
        }
        albums.sort_by(|a, b| {
            a.artist
                .to_lowercase()
                .cmp(&b.artist.to_lowercase())
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let mut artists: Vec<Artist> = artist_map.into_values().collect();
        for artist in artists.iter_mut() {
            let mut albums: Vec<String> = artist
                .tracks
                .iter()
                .map(|&i| self.tracks[i].album.to_lowercase())
                .collect();
            albums.sort();
            albums.dedup();
            artist.albums = albums.len();
        }
        artists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.albums = albums;
        self.artists = artists;
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Escanea las carpetas en un hilo y manda el resultado por el canal.
pub fn spawn_scan(folders: Vec<PathBuf>, tx: Sender<ScanMsg>) {
    std::thread::Builder::new()
        .name("flacbox-scan".into())
        .spawn(move || {
            let mut seen = std::collections::HashSet::new();
            let mut tracks = Vec::new();

            for root in folders {
                let walker = walkdir::WalkDir::new(&root)
                    .max_depth(8)
                    .into_iter()
                    .filter_entry(|e| {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        !(e.file_type().is_dir()
                            && (name.starts_with('.') || name == "node_modules"))
                    });

                for entry in walker.flatten() {
                    if !entry.file_type().is_file() || !is_audio(entry.path()) {
                        continue;
                    }
                    let path = entry.path().to_path_buf();
                    if !seen.insert(path.clone()) {
                        continue;
                    }
                    if let Some(track) = read_track(&path) {
                        tracks.push(track);
                    }
                }
            }

            tracks.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()));
            let _ = tx.send(ScanMsg::Done(tracks));
        })
        .ok();
}

fn read_track(path: &Path) -> Option<Track> {
    let tags = tags::read(path);
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Desconocido".into());

    let t = tags.unwrap_or_default();
    let duration_ms = t.duration_ms;
    if duration_ms > 0 && duration_ms < 2000 {
        return None;
    }

    let album = t
        .album
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Sin álbum".into());
    let artist = t
        .artist
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Artista desconocido".into());
    let album_artist = t
        .album_artist
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| artist.clone());

    let title = t.title.filter(|s| !s.trim().is_empty()).unwrap_or(fallback);

    Some(Track {
        title: title.clone(),
        artist: artist.clone(),
        album: album.clone(),
        album_artist,
        year: t.year,
        duration_ms,
        track_no: t.track.and_then(|v| v.parse().ok()).unwrap_or(0),
        disc_no: t.disc.and_then(|v| v.parse().ok()).unwrap_or(0),
        sample_rate: t.sample_rate,
        bits: t.bit_depth.unwrap_or(0),
        channels: t.channels,
        has_art: t.has_art,
        path: path.to_path_buf(),
        search_key: format!("{title} {artist} {album}").to_lowercase(),
    })
}

pub fn default_folders() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mut folders = Vec::new();
    for name in ["Música", "Music"] {
        let p = PathBuf::from(&home).join(name);
        if p.is_dir() {
            folders.push(p);
        }
    }
    folders
}
