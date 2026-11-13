use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;
use gpui_video_player::Video;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::internet::{streams_dir, wait_with_timeout};
use crate::library::Track;

const VIDEO_TIMEOUT: Duration = Duration::from_secs(600);

pub enum VideoMsg {
    Ready { id: String, path: PathBuf },
    Failed { id: String, message: String },
}

pub struct VideoState {
    pub video: Option<Video>,
    pub track: Option<Track>,
    pub active: bool,
}

impl VideoState {
    pub fn new() -> Self {
        Self {
            video: None,
            track: None,
            active: false,
        }
    }
}

pub fn existing_video(id: &str) -> Option<PathBuf> {
    let dir = streams_dir();
    for ext in ["mkv", "webm", "mp4"] {
        let candidate = dir.join(format!("{id}.video.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn spawn_download_video(id: String, url: String, tx: Sender<VideoMsg>) {
    if let Some(path) = existing_video(&id) {
        let _ = tx.send(VideoMsg::Ready { id, path });
        return;
    }
    thread::Builder::new()
        .name("flacbox-yt-video".into())
        .spawn(move || {
            let dir = streams_dir();
            if std::fs::create_dir_all(&dir).is_err() {
                let _ = tx.send(VideoMsg::Failed {
                    id,
                    message: "no se pudo crear el cache de streams".to_string(),
                });
                return;
            }
            let out = dir.join(format!("{id}.video.mkv"));
            let child = Command::new("yt-dlp")
                .args([
                    "-f",
                    "bestvideo[ext=webm][height<=480]+bestaudio[ext=webm]/best[height<=480]",
                    "--merge-output-format",
                    "mkv",
                    "--no-part",
                    "--no-playlist",
                    "--no-warnings",
                    "-o",
                    &out.to_string_lossy(),
                    "--",
                    &url,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let child = match child {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(VideoMsg::Failed {
                        id,
                        message: format!("no se pudo ejecutar yt-dlp: {err}"),
                    });
                    return;
                }
            };
            match wait_with_timeout(child, VIDEO_TIMEOUT) {
                Ok((_stdout, stderr)) => {
                    if let Some(path) = existing_video(&id) {
                        let _ = tx.send(VideoMsg::Ready { id, path });
                    } else {
                        let message = stderr
                            .lines()
                            .rev()
                            .find(|line| !line.trim().is_empty())
                            .unwrap_or("yt-dlp no produjo salida")
                            .trim()
                            .to_string();
                        let _ = tx.send(VideoMsg::Failed { id, message });
                    }
                }
                Err(message) => {
                    let _ = tx.send(VideoMsg::Failed { id, message });
                }
            }
        })
        .ok();
}

pub fn begin_video(
    id: &str,
    path: PathBuf,
    title: String,
    uploader: String,
    duration_ms: u64,
    state: &mut VideoState,
) -> Result<(), String> {
    gst::init().map_err(|err| format!("gstreamer no disponible: {err}"))?;
    let url = url::Url::from_file_path(&path).map_err(|()| "ruta de video inválida".to_string())?;
    let launch = format!(
        "playbin uri=\"{}\" video-sink=\"videoscale ! video/x-raw,format=NV12,width=848,height=480,pixel-aspect-ratio=1/1 ! videoconvert ! appsink name=gpui_video drop=true max-buffers=200 enable-last-sample=false caps=video/x-raw,format=NV12,pixel-aspect-ratio=1/1\"",
        url.as_str()
    );
    let pipeline = gst::parse::launch(&launch)
        .map_err(|err| format!("no se pudo lanzar el pipeline: {err}"))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "el pipeline no es un Pipeline".to_string())?;
    let video_sink: gst::Element = pipeline.property("video-sink");
    let pad = video_sink
        .pads()
        .first()
        .cloned()
        .ok_or("el appsink no tiene pads")?;
    let pad = pad
        .dynamic_cast::<gst::GhostPad>()
        .map_err(|_| "el appsink no es un GhostPad".to_string())?;
    let bin = pad
        .parent_element()
        .ok_or("el pipeline no tiene padre")?
        .downcast::<gst::Bin>()
        .map_err(|_| "el video-sink no es un bin".to_string())?;
    let video_sink = bin
        .by_name("gpui_video")
        .ok_or("no se encontró el appsink gpui_video")?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| "gpui_video no es un appsink".to_string())?;
    let video = gpui_video_player::Video::from_gst_pipeline(pipeline, video_sink, None)
        .map_err(|err| format!("no se pudo abrir el video: {err}"))?;
    state.video = Some(video);
    state.active = true;
    state.track = Some(Track {
        path,
        title,
        artist: uploader,
        album: "YouTube".to_string(),
        album_artist: String::new(),
        year: None,
        duration_ms,
        track_no: 0,
        disc_no: 0,
        sample_rate: 0,
        bits: 0,
        channels: 2,
        has_art: false,
        search_key: String::new(),
    });
    let _ = id;
    Ok(())
}
