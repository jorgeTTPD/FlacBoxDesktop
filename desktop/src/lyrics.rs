use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::Sender;
use ureq::typestate::WithoutBody;
use ureq::RequestBuilder;

use flacplayer::lyrics::{
    cache_dir, cache_key, embedded_lyrics, parse_lrc, read_sidecar, LyricLine, LyricSource, Lyrics,
};

const USER_AGENT: &str = "flacbox-desktop/0.1 (+https://github.com/jorgeTTPD/FlacBoxDesktop)";
const TIMEOUT_SECS: u64 = 8;

/// Estado de la carga de la letra en la interfaz.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LyricsStatus {
    /// No hay nada sonando.
    #[default]
    Idle,
    /// Buscando la letra de la pista actual.
    Loading,
    /// Letra lista para mostrar.
    Ready,
    /// No se encontró letra.
    Empty,
}

/// Datos necesarios para resolver la letra de una pista.
#[derive(Clone, Debug)]
pub struct LyricsRequest {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub audio_path: Option<PathBuf>,
}

/// Resultado que manda el hilo de resolución.
#[derive(Clone, Debug)]
pub struct LyricsResult {
    /// Clave de la pista; si no coincide con la que se pidió, el resultado está obsoleto.
    pub key: String,
    pub lyrics: Option<Lyrics>,
}

/// Lanza la resolución en un hilo aparte (mismo patrón que el escaneo de biblioteca).
pub fn spawn_resolve(request: LyricsRequest, key: String, tx: Sender<LyricsResult>) {
    std::thread::Builder::new()
        .name("flacbox-lyrics".into())
        .spawn(move || {
            let lyrics = resolve(&request);
            let _ = tx.send(LyricsResult { key, lyrics });
        })
        .ok();
}

/// Prioridad de resolución (igual que tus scripts de Python, pero en Rust):
/// 1. letra embebida sincronizada · 2. `.lrc` hermano · 3. cache en disco ·
/// 4. LRCLIB · 5. letra embebida plana como último recurso.
pub fn resolve(request: &LyricsRequest) -> Option<Lyrics> {
    let mut plain_fallback: Option<Lyrics> = None;

    if let Some(path) = &request.audio_path {
        if let Some(embedded) = embedded_lyrics(path) {
            if embedded.synced {
                return Some(embedded);
            }
            plain_fallback = Some(embedded);
        }
        if let Some(sidecar) = read_sidecar(path) {
            return Some(sidecar);
        }
    }

    let key = cache_key(&request.title, &request.artist, &request.album);

    if let Some(cached) = cache_read(&key) {
        return Some(cached);
    }

    if !request.title.trim().is_empty() {
        if let Some(fetched) = fetch_lrclib(
            &request.title,
            &request.artist,
            &request.album,
            request.duration_ms,
        ) {
            cache_write(&key, &fetched);
            return Some(fetched);
        }
    }

    plain_fallback
}

/// Busca letras sincronizadas en LRCLIB: coincidencia exacta (con duración),
/// búsqueda normal y búsqueda difusa por título.
pub fn fetch_lrclib(title: &str, artist: &str, album: &str, duration_ms: i64) -> Option<Lyrics> {
    let agent = agent();
    let duration_secs = (duration_ms > 0).then(|| (duration_ms / 1000) as u32);

    let exact = duration_secs.and_then(|secs| {
        lrclib_object(
            agent
                .get("https://lrclib.net/api/get")
                .query("track_name", title)
                .query("artist_name", artist)
                .query("album_name", album)
                .query("duration", &secs.to_string()),
        )
    });

    let searched = exact.or_else(|| {
        lrclib_array(
            agent
                .get("https://lrclib.net/api/search")
                .query("track_name", title)
                .query("artist_name", artist)
                .query("album_name", album),
        )
    });

    searched.or_else(|| lrclib_array(agent.get("https://lrclib.net/api/search").query("q", title)))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn get_text(request: RequestBuilder<WithoutBody>) -> Option<String> {
    let mut response = request.call().ok()?;
    response.body_mut().read_to_string().ok()
}

/// Texto LRC -> letra sincronizada de LRCLIB.
fn to_lyrics(text: &str) -> Option<Lyrics> {
    let (lines, timed) = parse_lrc(text);
    if lines.is_empty() {
        return None;
    }
    Some(Lyrics {
        lines,
        synced: timed,
        source: LyricSource::Lrclib,
    })
}

/// `/api/get`: un objeto con `syncedLyrics`.
fn lrclib_object(request: RequestBuilder<WithoutBody>) -> Option<Lyrics> {
    let text = get_text(request)?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    to_lyrics(json.get("syncedLyrics")?.as_str()?)
}

/// `/api/search`: array de objetos, el primero con `syncedLyrics`.
fn lrclib_array(request: RequestBuilder<WithoutBody>) -> Option<Lyrics> {
    let text = get_text(request)?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let raw = json
        .as_array()?
        .iter()
        .find_map(|item| item.get("syncedLyrics")?.as_str())?;
    to_lyrics(raw)
}

// ------------------------------------------------------------- cache en disco

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedLyrics {
    synced: bool,
    lines: Vec<CachedLine>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedLine {
    time_ms: u64,
    lyric: String,
}

fn cache_file(key: &str) -> Option<PathBuf> {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_matches('_').to_string();
    if safe.is_empty() {
        return None;
    }
    Some(cache_dir()?.join(format!("{safe}.json")))
}

fn cache_read(key: &str) -> Option<Lyrics> {
    let text = std::fs::read_to_string(cache_file(key)?).ok()?;
    let cached: CachedLyrics = serde_json::from_str(&text).ok()?;
    let lines = cached
        .lines
        .into_iter()
        .map(|l| LyricLine {
            time_ms: l.time_ms,
            text: l.lyric,
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(Lyrics {
        lines,
        synced: cached.synced,
        source: LyricSource::Lrclib,
    })
}

fn cache_write(key: &str, lyrics: &Lyrics) {
    if lyrics.is_empty() {
        return;
    }
    let Some(path) = cache_file(key) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = CachedLyrics {
        synced: lyrics.synced,
        lines: lyrics
            .lines
            .iter()
            .map(|l| CachedLine {
                time_ms: l.time_ms,
                lyric: l.text.clone(),
            })
            .collect(),
    };
    if let Ok(text) = serde_json::to_string(&payload) {
        let _ = std::fs::write(path, text);
    }
}
