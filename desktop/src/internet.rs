use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_json::Value;

const SEARCH_LIMIT: usize = 15;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(240);

#[derive(Clone)]
pub struct InternetResult {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration_ms: u64,
    pub view_count: u64,
    pub url: String,
}

pub enum InternetMsg {
    SearchDone(Vec<InternetResult>),
    SearchError(String),
    DownloadDone { id: String, path: PathBuf },
    DownloadError { id: String, message: String },
}

pub fn streams_dir() -> PathBuf {
    std::env::var_os("FLACBOX_STREAMS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .unwrap_or_else(|| PathBuf::from("."))
                .join("flacbox/streams")
        })
}

pub fn thumbs_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("flacbox/thumbs")
}

pub fn existing_cached(id: &str) -> Option<PathBuf> {
    let dir = streams_dir();
    for ext in ["m4a", "webm", "opus", "mp3", "ogg"] {
        let candidate = dir.join(format!("{id}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn spawn_search(query: String, tx: Sender<InternetMsg>) {
    thread::Builder::new()
        .name("flacbox-yt-search".into())
        .spawn(move || {
            let arg = format!("ytsearch{SEARCH_LIMIT}:{query}");
            let child = Command::new("yt-dlp")
                .args([
                    "--flat-playlist",
                    "--dump-json",
                    "--no-warnings",
                    "--",
                    &arg,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let child = match child {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(InternetMsg::SearchError(format!(
                        "no se pudo ejecutar yt-dlp: {err}. Instala yt-dlp (pacman -S yt-dlp)"
                    )));
                    return;
                }
            };
            match wait_with_timeout(child, SEARCH_TIMEOUT) {
                Ok((stdout, stderr)) => {
                    let results = parse_search_json(&stdout);
                    spawn_thumbs(&results);
                    if results.is_empty() && !stderr.trim().is_empty() {
                        let _ = tx.send(InternetMsg::SearchError(stderr.trim().to_string()));
                    } else {
                        let _ = tx.send(InternetMsg::SearchDone(results));
                    }
                }
                Err(message) => {
                    let _ = tx.send(InternetMsg::SearchError(message));
                }
            }
        })
        .ok();
}

pub fn spawn_download(id: String, url: String, tx: Sender<InternetMsg>) {
    if let Some(path) = existing_cached(&id) {
        let _ = tx.send(InternetMsg::DownloadDone { id, path });
        return;
    }
    thread::Builder::new()
        .name("flacbox-yt-download".into())
        .spawn(move || {
            let dir = streams_dir();
            if std::fs::create_dir_all(&dir).is_err() {
                let _ = tx.send(InternetMsg::DownloadError {
                    id,
                    message: "no se pudo crear el cache de streams".to_string(),
                });
                return;
            }
            let out_template = dir.join(format!("{id}.%(ext)s"));
            let child = Command::new("yt-dlp")
                .args([
                    "-f",
                    "bestaudio[ext=m4a]/bestaudio",
                    "--no-part",
                    "--no-playlist",
                    "--no-warnings",
                    "--newline",
                    "-o",
                    &out_template.to_string_lossy(),
                    "--",
                    &url,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let child = match child {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(InternetMsg::DownloadError {
                        id,
                        message: format!("no se pudo ejecutar yt-dlp: {err}"),
                    });
                    return;
                }
            };
            match wait_with_timeout(child, DOWNLOAD_TIMEOUT) {
                Ok((_stdout, stderr)) => {
                    if let Some(path) = existing_cached(&id) {
                        let _ = tx.send(InternetMsg::DownloadDone { id, path });
                    } else {
                        let message = stderr
                            .lines()
                            .rev()
                            .find(|line| !line.trim().is_empty())
                            .unwrap_or("yt-dlp no produjo salida")
                            .trim()
                            .to_string();
                        let _ = tx.send(InternetMsg::DownloadError { id, message });
                    }
                }
                Err(message) => {
                    let _ = tx.send(InternetMsg::DownloadError { id, message });
                }
            }
        })
        .ok();
}

pub(crate) fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<(String, String), String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let _ = status;
                return Ok((stdout, stderr));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("yt-dlp tardó demasiado y fue cancelado".to_string());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(format!("error esperando a yt-dlp: {err}")),
        }
    }
}

pub fn spawn_thumbs(results: &[InternetResult]) {
    let dir = thumbs_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for result in results.iter() {
        let path = dir.join(format!("{}.jpg", result.id));
        if path.is_file() {
            continue;
        }
        let id = result.id.clone();
        thread::Builder::new()
            .name("flacbox-yt-thumb".into())
            .spawn(move || {
                let url = format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg");
                let mut response = match ureq::get(&url).call() {
                    Ok(response) => response,
                    Err(_) => return,
                };
                if let Ok(bytes) = response.body_mut().read_to_vec() {
                    let _ = std::fs::write(path, bytes);
                }
            })
            .ok();
    }
}

fn parse_search_json(raw: &str) -> Vec<InternetResult> {
    raw.lines()
        .filter_map(|line| parse_result_line(line))
        .collect()
}

fn parse_result_line(line: &str) -> Option<InternetResult> {
    let value: Value = serde_json::from_str(line).ok()?;
    let id = value.get("id").and_then(Value::as_str)?.to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)?
        .to_string();
    let uploader = value
        .get("uploader")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| value.get("channel").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let duration_ms = value
        .get("duration")
        .and_then(Value::as_f64)
        .map(|seconds| (seconds * 1000.0) as u64)
        .unwrap_or(0);
    let view_count = value
        .get("view_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
    Some(InternetResult {
        id,
        title,
        uploader,
        duration_ms,
        view_count,
        url,
    })
}
