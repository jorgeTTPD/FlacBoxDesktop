use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};

pub const STATUS_IDLE: u8 = 0;
pub const STATUS_LOADING: u8 = 1;
pub const STATUS_PLAYING: u8 = 2;
pub const STATUS_PAUSED: u8 = 3;
pub const STATUS_ENDED: u8 = 4;
#[allow(dead_code)]
pub const STATUS_ERROR: u8 = 5;

pub const EVENT_STATUS: i32 = 1;
pub const EVENT_TRACK_STARTED: i32 = 2;
pub const EVENT_TRACK_ENDED: i32 = 3;
pub const EVENT_ERROR: i32 = 4;
pub const EVENT_QUEUE_ENDED: i32 = 5;
pub const EVENT_STREAM_INFO: i32 = 6;

/// Estado compartido entre el hilo de control, el hilo decodificador y el
/// callback de audio. Todo son atómicos: el callback nunca bloquea.
pub struct Shared {
    /// Puerta de reproducción. Si es false el callback emite silencio.
    pub playing: AtomicBool,
    /// Se incrementa en cada seek/flush. El callback drena la cola al verlo cambiar.
    pub epoch: AtomicU64,
    /// Frames de stream consumidos desde el último flush.
    pub popped_frames: AtomicU64,
    /// Frames de origen (sample rate del archivo) al inicio del tramo actual.
    pub seek_base_frames: AtomicU64,
    pub stream_rate: AtomicU32,
    pub source_rate: AtomicU32,
    pub index: AtomicI32,
    pub duration_ms: AtomicU64,
    pub status: AtomicU8,
    pub volume: AtomicU32,
    /// Pico del último bloque, en bits de f32 (para el vúmetro).
    pub level: AtomicU32,
    pub underruns: AtomicU64,
    pub bits: AtomicU32,
    pub channels: AtomicU32,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            popped_frames: AtomicU64::new(0),
            seek_base_frames: AtomicU64::new(0),
            stream_rate: AtomicU32::new(0),
            source_rate: AtomicU32::new(0),
            index: AtomicI32::new(-1),
            duration_ms: AtomicU64::new(0),
            status: AtomicU8::new(STATUS_IDLE),
            volume: AtomicU32::new(1.0f32.to_bits()),
            level: AtomicU32::new(0),
            underruns: AtomicU64::new(0),
            bits: AtomicU32::new(0),
            channels: AtomicU32::new(0),
        }
    }

    pub fn position_ms(&self) -> i64 {
        let popped = self.popped_frames.load(Ordering::Acquire);
        let stream_rate = self.stream_rate.load(Ordering::Acquire) as u64;
        let source_rate = self.source_rate.load(Ordering::Acquire) as u64;
        if stream_rate == 0 || source_rate == 0 {
            return 0;
        }
        let source_frames = popped.saturating_mul(source_rate) / stream_rate;
        let base = self.seek_base_frames.load(Ordering::Acquire);
        ((base + source_frames) * 1000 / source_rate) as i64
    }

    pub fn reset_position(&self, source_frames: u64) {
        self.seek_base_frames
            .store(source_frames, Ordering::Release);
        self.popped_frames.store(0, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_status(&self, status: u8) {
        self.status.store(status, Ordering::Release);
    }

    pub fn status(&self) -> u8 {
        self.status.load(Ordering::Acquire)
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Acquire))
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posicion_avanza_con_los_frames_consumidos() {
        let s = Shared::new();
        s.stream_rate.store(48000, Ordering::Release);
        s.source_rate.store(48000, Ordering::Release);
        s.reset_position(0);
        s.popped_frames.store(48000, Ordering::Release);
        assert_eq!(s.position_ms(), 1000);
    }

    #[test]
    fn posicion_corrige_cuando_el_stream_remuestrea() {
        let s = Shared::new();
        s.stream_rate.store(48000, Ordering::Release);
        s.source_rate.store(44100, Ordering::Release);
        s.reset_position(0);
        s.popped_frames.store(48000, Ordering::Release);
        assert_eq!(s.position_ms(), 1000);
    }

    #[test]
    fn posicion_parte_de_la_base_del_seek() {
        let s = Shared::new();
        s.stream_rate.store(44100, Ordering::Release);
        s.source_rate.store(44100, Ordering::Release);
        s.reset_position(44100 * 30);
        s.popped_frames.store(44100 / 2, Ordering::Release);
        assert_eq!(s.position_ms(), 30500);
    }
}
