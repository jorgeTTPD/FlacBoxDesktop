use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::shared::Shared;

/// 1 << 17 f32 = 131072 muestras ≈ 1.36 s a 48 kHz estéreo.
pub const RING_CAPACITY: usize = 1 << 17;

const FADE_SECONDS: f32 = 0.012;

pub struct Stream {
    _inner: cpal::Stream,
    rate: u32,
}

impl Stream {
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
}

struct Renderer {
    cons: Consumer<f32>,
    shared: Arc<Shared>,
    epoch: u64,
    gain: f32,
    popped: u64,
    peak: f32,
}

impl Renderer {
    fn new(cons: Consumer<f32>, shared: Arc<Shared>) -> Self {
        Self {
            cons,
            shared,
            epoch: 0,
            gain: 0.0,
            popped: 0,
            peak: 0.0,
        }
    }

    fn render(&mut self, data: &mut [f32], channels: usize, rate: u32) {
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        if epoch != self.epoch {
            self.epoch = epoch;
            while self.cons.pop().is_ok() {}
            self.popped = 0;
            self.shared.popped_frames.store(0, Ordering::Release);
        }

        let step = 1.0 / (rate.max(8000) as f32 * FADE_SECONDS);
        let playing = self.shared.playing.load(Ordering::Acquire);
        let target = if playing { 1.0 } else { 0.0 };
        let volume = self.shared.volume();

        let mut underruns = 0u64;
        let mut peak = 0.0f32;

        for frame in data.chunks_mut(channels) {
            if self.gain < target {
                self.gain = (self.gain + step).min(target);
            } else if self.gain > target {
                self.gain = (self.gain - step).max(target);
            }

            if !playing && self.gain <= 0.0 {
                for s in frame.iter_mut() {
                    *s = 0.0;
                }
                continue;
            }

            let left = match self.cons.pop() {
                Ok(v) => v,
                Err(_) => {
                    underruns += 1;
                    0.0
                }
            };
            let right = self.cons.pop().unwrap_or(0.0);

            let l = left * volume * self.gain;
            let r = right * volume * self.gain;
            let abs = l.abs().max(r.abs());
            if abs > peak {
                peak = abs;
            }

            if channels == 1 {
                frame[0] = (l + r) * 0.5;
            } else {
                frame[0] = l;
                frame[1] = r;
                for s in frame[2..].iter_mut() {
                    *s = 0.0;
                }
            }

            self.popped += 1;
        }

        if self.popped > 0 {
            self.shared
                .popped_frames
                .fetch_add(self.popped, Ordering::AcqRel);
            self.popped = 0;
        }
        if underruns > 0 {
            self.shared
                .underruns
                .fetch_add(underruns, Ordering::Relaxed);
        }
        self.shared.level.store(peak.to_bits(), Ordering::Relaxed);
    }
}

pub fn build_stream(rate: u32, shared: Arc<Shared>) -> Result<(Stream, Producer<f32>), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no hay dispositivo de salida de audio".to_string())?;

    let mut chosen = None;
    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            if range.sample_format() != cpal::SampleFormat::F32 {
                continue;
            }
            if range.min_sample_rate() > rate || range.max_sample_rate() < rate {
                continue;
            }
            if range.channels() < 2 {
                continue;
            }
            chosen = Some(range.with_sample_rate(rate));
            break;
        }
    }

    let supported =
        chosen.ok_or_else(|| format!("el dispositivo no soporta {rate} Hz en f32 estéreo"))?;
    let channels = supported.channels() as usize;
    let sample_rate = supported.sample_rate();
    let config: cpal::StreamConfig = supported.into();

    let (prod, cons) = RingBuffer::new(RING_CAPACITY);
    let mut renderer = Renderer::new(cons, shared.clone());

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [f32], _| renderer.render(data, channels, sample_rate),
            move |err| log::error!("error en el stream de audio: {err}"),
            Some(Duration::from_millis(500)),
        )
        .map_err(|e| format!("no se pudo abrir el dispositivo: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("no se pudo iniciar el stream: {e}"))?;

    shared.stream_rate.store(sample_rate, Ordering::Release);

    Ok((
        Stream {
            _inner: stream,
            rate: sample_rate,
        },
        prod,
    ))
}
