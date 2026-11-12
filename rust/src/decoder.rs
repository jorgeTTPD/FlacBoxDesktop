use std::fs::File;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use rtrb::Producer;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use crate::shared::Shared;

const SLEEP_WHEN_FULL: Duration = Duration::from_millis(2);

pub enum DecCmd {
    Open {
        index: usize,
        path: String,
        start_ms: u64,
    },
    Seek(u64),
    NewProducer(Producer<f32>, u32),
    Stop,
}

pub enum DecEvt {
    NeedStream {
        index: usize,
        rate: u32,
    },
    TrackStarted {
        index: usize,
        duration_ms: u64,
        source_rate: u32,
        stream_rate: u32,
        bits: u32,
        channels: usize,
        codec: String,
    },
    TrackEnded {
        index: usize,
    },
    Error {
        index: usize,
        msg: String,
    },
}

pub struct TrackReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    pub rate: u32,
    pub channels: usize,
    pub bits: u32,
    pub duration_ms: u64,
    pub codec: String,
}

impl TrackReader {
    pub fn open(path: &str) -> Result<TrackReader, String> {
        let file = File::open(Path::new(path)).map_err(|e| format!("abrir {path}: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("probe: {e}"))?;

        let (track_id, rate, channels, bits, duration_ms, codec, time_base, params) = {
            let track = format
                .default_track(TrackType::Audio)
                .ok_or_else(|| "sin pista de audio".to_string())?;
            let params = track
                .codec_params
                .as_ref()
                .and_then(|p| p.audio())
                .ok_or_else(|| "parámetros de audio ausentes".to_string())?
                .clone();
            let rate = params.sample_rate.unwrap_or(44100);
            let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2);
            let bits = params.bits_per_sample.unwrap_or(16);
            let duration_ms = match (track.num_frames, rate) {
                (Some(frames), r) if r > 0 => frames * 1000 / u64::from(r),
                _ => 0,
            };
            let codec = Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_uppercase())
                .unwrap_or_else(|| params.codec.to_string());
            (
                track.id,
                rate,
                channels,
                bits,
                duration_ms,
                codec,
                track.time_base,
                params,
            )
        };

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| format!("decoder: {e}"))?;

        Ok(TrackReader {
            format,
            decoder,
            track_id,
            time_base,
            rate,
            channels,
            bits,
            duration_ms,
            codec,
        })
    }

    fn actual_ms(&self, ts: Timestamp) -> Option<u64> {
        self.time_base
            .and_then(|tb| tb.calc_time(ts))
            .map(|t| (t.as_secs_f64().max(0.0) * 1000.0) as u64)
    }

    /// Devuelve (ms reales tras el seek, frames a descartar para llegar al objetivo).
    pub fn seek_ms(&mut self, ms: u64) -> (u64, u64) {
        let time = Time::from_millis_u64(ms);
        match self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        ) {
            Ok(seeked) => {
                self.decoder.reset();
                let actual = self.actual_ms(seeked.actual_ts).unwrap_or(ms);
                let skip = if ms > actual {
                    (ms - actual) * u64::from(self.rate) / 1000
                } else {
                    0
                };
                (actual, skip)
            }
            Err(e) => {
                self.decoder.reset();
                log_warn(&format!("seek falló: {e}"));
                (ms, 0)
            }
        }
    }

    /// Decodifica el siguiente paquete intercalado. `Ok(true)` = fin de archivo.
    pub fn next_frames(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(true),
                Err(SymError::IoError(io)) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(true)
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let frames = buf.frames();
                    let channels = buf.spec().channels().count().max(1);
                    out.resize(frames * channels, 0.0);
                    buf.copy_to_slice_interleaved(&mut out[..]);
                    return Ok(false);
                }
                Err(SymError::DecodeError(msg)) => {
                    log_warn(&format!("paquete con error: {msg}"));
                    continue;
                }
                Err(SymError::IoError(io)) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(true)
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

fn log_warn(msg: &str) {
    log::warn!("{msg}");
}

pub struct DecoderThread {
    pub shared: Arc<Shared>,
}

impl DecoderThread {
    pub fn spawn(
        shared: Arc<Shared>,
        cmd_rx: Receiver<DecCmd>,
        evt_tx: Sender<DecEvt>,
        prod: Producer<f32>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("flacbox-decoder".into())
            .spawn(move || decoder_main(shared, cmd_rx, evt_tx, prod))
            .expect("no se pudo crear el hilo decodificador")
    }
}

fn wait_producer(
    shared: &Shared,
    cmd_rx: &Receiver<DecCmd>,
    evt_tx: &Sender<DecEvt>,
    index: usize,
    rate: u32,
) -> Option<Producer<f32>> {
    loop {
        evt_tx.send(DecEvt::NeedStream { index, rate }).ok()?;
        match cmd_rx.recv_timeout(Duration::from_millis(300)) {
            Ok(DecCmd::NewProducer(prod, stream_rate)) => {
                shared.stream_rate.store(stream_rate, Ordering::Release);
                return Some(prod);
            }
            Ok(DecCmd::Stop) => return None,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn decoder_main(
    shared: Arc<Shared>,
    cmd_rx: Receiver<DecCmd>,
    evt_tx: Sender<DecEvt>,
    mut prod: Producer<f32>,
) {
    let mut pending: Option<(usize, String, u64)> = None;

    'outer: loop {
        let (index, path, start_ms) = match pending.take() {
            Some(v) => v,
            None => loop {
                match cmd_rx.recv() {
                    Ok(DecCmd::Open {
                        index,
                        path,
                        start_ms,
                    }) => break (index, path, start_ms),
                    Ok(DecCmd::NewProducer(p, rate)) => {
                        prod = p;
                        shared.stream_rate.store(rate, Ordering::Release);
                    }
                    Ok(DecCmd::Seek(_)) => {}
                    Ok(DecCmd::Stop) | Err(_) => return,
                }
            },
        };

        let mut reader = match TrackReader::open(&path) {
            Ok(r) => r,
            Err(msg) => {
                let _ = evt_tx.send(DecEvt::Error { index, msg });
                let _ = evt_tx.send(DecEvt::TrackEnded { index });
                continue;
            }
        };

        if reader.channels == 0 || reader.rate == 0 {
            let _ = evt_tx.send(DecEvt::Error {
                index,
                msg: "formato de audio inválido".into(),
            });
            let _ = evt_tx.send(DecEvt::TrackEnded { index });
            continue;
        }

        let current_stream_rate = shared.stream_rate.load(Ordering::Acquire);
        if current_stream_rate == 0 || current_stream_rate != reader.rate {
            match wait_producer(&shared, &cmd_rx, &evt_tx, index, reader.rate) {
                Some(p) => prod = p,
                None => return,
            }
        }
        let stream_rate = shared.stream_rate.load(Ordering::Acquire).max(1);

        shared.source_rate.store(reader.rate, Ordering::Release);
        shared
            .duration_ms
            .store(reader.duration_ms, Ordering::Release);
        shared.bits.store(reader.bits, Ordering::Release);
        shared
            .channels
            .store(reader.channels as u32, Ordering::Release);
        shared.index.store(index as i32, Ordering::Release);

        let _ = evt_tx.send(DecEvt::TrackStarted {
            index,
            duration_ms: reader.duration_ms,
            source_rate: reader.rate,
            stream_rate,
            bits: reader.bits,
            channels: reader.channels,
            codec: reader.codec.clone(),
        });

        let mut skip_frames = 0u64;
        if start_ms > 0 {
            let (actual, skip) = reader.seek_ms(start_ms);
            skip_frames = skip;
            shared.reset_position(actual * u64::from(reader.rate) / 1000);
        } else {
            shared.reset_position(0);
        }

        let mut buf: Vec<f32> = Vec::with_capacity(16384);
        let mut stereo: Vec<f32> = Vec::with_capacity(16384);
        let mut eof = false;

        while !eof {
            // comandos pendientes
            loop {
                match cmd_rx.try_recv() {
                    Ok(DecCmd::Seek(ms)) => {
                        let (actual, skip) = reader.seek_ms(ms);
                        skip_frames = skip;
                        shared.reset_position(actual * u64::from(reader.rate) / 1000);
                        stereo.clear();
                    }
                    Ok(DecCmd::Stop) => return,
                    Ok(DecCmd::Open {
                        index,
                        path,
                        start_ms,
                    }) => {
                        pending = Some((index, path, start_ms));
                        continue 'outer;
                    }
                    Ok(DecCmd::NewProducer(p, rate)) => {
                        prod = p;
                        shared.stream_rate.store(rate, Ordering::Release);
                    }
                    Err(_) => break,
                }
            }

            match reader.next_frames(&mut buf) {
                Ok(true) => {
                    eof = true;
                    continue;
                }
                Ok(false) => {}
                Err(msg) => {
                    let _ = evt_tx.send(DecEvt::Error { index, msg });
                    eof = true;
                    continue;
                }
            }

            let channels = reader.channels;
            let frames = buf.len() / channels.max(1);

            if skip_frames > 0 {
                let drop_frames = skip_frames.min(frames as u64) as usize;
                skip_frames -= drop_frames as u64;
                if drop_frames == frames {
                    continue;
                }
                buf.drain(..drop_frames * channels);
            }

            let frames_left = buf.len() / channels.max(1);
            stereo.clear();
            stereo.reserve(frames_left * 2);
            match channels {
                1 => {
                    for s in &buf {
                        stereo.push(*s);
                        stereo.push(*s);
                    }
                }
                2 => stereo.extend_from_slice(&buf),
                n => {
                    // multicanal: se usan los dos primeros canales
                    for frame in buf.chunks_exact(n) {
                        stereo.push(frame[0]);
                        stereo.push(frame[1]);
                    }
                }
            }

            let mut written = 0usize;
            while written < stereo.len() {
                match prod.push(stereo[written]) {
                    Ok(()) => written += 1,
                    Err(_) => match cmd_rx.recv_timeout(SLEEP_WHEN_FULL) {
                        Ok(DecCmd::Seek(ms)) => {
                            let (actual, skip) = reader.seek_ms(ms);
                            skip_frames = skip;
                            shared.reset_position(actual * u64::from(reader.rate) / 1000);
                            break;
                        }
                        Ok(DecCmd::Stop) => return,
                        Ok(DecCmd::Open {
                            index,
                            path,
                            start_ms,
                        }) => {
                            pending = Some((index, path, start_ms));
                            continue 'outer;
                        }
                        Ok(DecCmd::NewProducer(p, rate)) => {
                            prod = p;
                            shared.stream_rate.store(rate, Ordering::Release);
                        }
                        Err(_) => {}
                    },
                }
            }
        }

        let _ = evt_tx.send(DecEvt::TrackEnded { index });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_archivo_inexistente_devuelve_error() {
        assert!(TrackReader::open("/no/existe.flac").is_err());
    }
}
