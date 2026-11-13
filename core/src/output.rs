use std::sync::atomic::Ordering;
use std::sync::Arc;

use oboe::{
    AudioOutputCallback, AudioOutputStreamSafe, AudioStream, AudioStreamAsync, AudioStreamBase,
    AudioStreamBuilder, AudioStreamSafe, DataCallbackResult, Output, PerformanceMode, SharingMode,
    Stereo,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::shared::Shared;

/// 1 << 17 f32 = 131072 muestras ≈ 1.36 s a 48 kHz estéreo.
pub const RING_CAPACITY: usize = 1 << 17;

const FADE_SECONDS: f32 = 0.012;

pub struct Renderer {
    cons: Consumer<f32>,
    shared: Arc<Shared>,
    epoch: u64,
    gain: f32,
    popped: u64,
    peak: f32,
}

impl Renderer {
    pub fn new(cons: Consumer<f32>, shared: Arc<Shared>) -> Self {
        Self {
            cons,
            shared,
            epoch: 0,
            gain: 0.0,
            popped: 0,
            peak: 0.0,
        }
    }
}

impl AudioOutputCallback for Renderer {
    type FrameType = (f32, Stereo);

    fn on_audio_ready(
        &mut self,
        stream: &mut dyn AudioOutputStreamSafe,
        data: &mut [(f32, f32)],
    ) -> DataCallbackResult {
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        if epoch != self.epoch {
            self.epoch = epoch;
            while self.cons.pop().is_ok() {}
            self.popped = 0;
            self.shared.popped_frames.store(0, Ordering::Release);
        }

        let rate = stream.get_sample_rate().max(8000) as f32;
        let step = 1.0 / (rate * FADE_SECONDS);
        let playing = self.shared.playing.load(Ordering::Acquire);
        let target = if playing { 1.0 } else { 0.0 };
        let volume = self.shared.volume();

        let mut underruns = 0u64;
        let mut peak = 0.0f32;

        for frame in data.iter_mut() {
            if self.gain < target {
                self.gain = (self.gain + step).min(target);
            } else if self.gain > target {
                self.gain = (self.gain - step).max(target);
            }

            if !playing && self.gain <= 0.0 {
                *frame = (0.0, 0.0);
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
            *frame = (l, r);
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

        DataCallbackResult::Continue
    }
}

pub type Stream = AudioStreamAsync<Output, Renderer>;

pub fn build_stream(rate: u32, shared: Arc<Shared>) -> Result<(Stream, Producer<f32>), String> {
    let (prod, cons) = RingBuffer::new(RING_CAPACITY);

    let builder = AudioStreamBuilder::default()
        .set_format::<f32>()
        .set_channel_count::<Stereo>()
        .set_sample_rate(rate as i32)
        .set_performance_mode(PerformanceMode::None)
        .set_sharing_mode(SharingMode::Shared)
        .set_callback(Renderer::new(cons, shared.clone()));

    let mut stream = builder
        .open_stream()
        .map_err(|e| format!("no se pudo abrir AAudio: {e:?}"))?;

    let burst = stream.get_frames_per_burst().max(64);
    let _ = stream.set_buffer_size_in_frames(burst * 2);

    let actual = stream.get_sample_rate().max(8000) as u32;
    shared.stream_rate.store(actual, Ordering::Release);

    stream
        .start()
        .map_err(|e| format!("no se pudo iniciar el stream: {e:?}"))?;

    Ok((stream, prod))
}
