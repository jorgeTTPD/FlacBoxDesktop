use std::sync::atomic::Ordering;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver};
use rtrb::RingBuffer;

use crate::decoder::{DecCmd, DecEvt, DecoderThread};
use crate::engine::{engine_main, Cmd, EngineHandle};
use crate::shared::*;

/// Un evento del motor, equivalente a `NativeAudio.onEvent` en Android.
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: i32,
    pub index: i32,
    pub a: i64,
    pub b: i64,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub position_ms: i64,
    pub duration_ms: i64,
    pub index: i32,
    pub status: u8,
    pub source_rate: u32,
    pub stream_rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub underruns: i64,
    pub level: f32,
    pub playing: bool,
}

/// Motor listo para usarse desde Rust (sin JNI).
pub struct Player {
    pub shared: Arc<Shared>,
    handle: EngineHandle,
    pub events: Receiver<Event>,
}

impl Player {
    pub fn spawn() -> Result<Self, String> {
        let shared = Arc::new(Shared::new());
        let (cmd_tx, cmd_rx) = bounded::<Cmd>(64);
        let (dec_tx, dec_rx) = bounded::<DecCmd>(64);
        let (evt_tx, evt_rx) = bounded::<DecEvt>(256);
        let (prod, cons) = RingBuffer::<f32>::new(1024);
        std::mem::forget(cons);

        let (out_tx, out_rx) = crossbeam_channel::unbounded::<Event>();

        let dec_shared = shared.clone();
        let mut threads = Vec::with_capacity(2);
        threads.push(DecoderThread::spawn(dec_shared, dec_rx, evt_tx, prod));

        let eng_shared = shared.clone();
        let handle = std::thread::Builder::new()
            .name("flacbox-engine".into())
            .spawn(move || {
                let emit = move |kind: i32, index: i32, a: i64, b: i64, text: Option<&str>| {
                    let _ = out_tx.send(Event {
                        kind,
                        index,
                        a,
                        b,
                        text: text.map(|t| t.to_string()),
                    });
                };
                engine_main(eng_shared, cmd_rx, evt_rx, dec_tx, emit);
            })
            .map_err(|e| format!("no se pudo crear el hilo de control: {e}"))?;
        threads.push(handle);

        Ok(Self {
            shared: shared.clone(),
            handle: EngineHandle::new(shared, cmd_tx, threads),
            events: out_rx,
        })
    }

    pub fn send(&self, cmd: Cmd) {
        self.handle.send(cmd);
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = &self.shared;
        let bits = s.level.load(Ordering::Relaxed);
        Snapshot {
            position_ms: s.position_ms(),
            duration_ms: s.duration_ms.load(Ordering::Acquire) as i64,
            index: s.index.load(Ordering::Acquire),
            status: s.status(),
            source_rate: s.source_rate.load(Ordering::Acquire),
            stream_rate: s.stream_rate.load(Ordering::Acquire),
            bits: s.bits.load(Ordering::Acquire),
            channels: s.channels.load(Ordering::Acquire),
            underruns: s.underruns.load(Ordering::Acquire) as i64,
            level: f32::from_bits(bits),
            playing: s.playing.load(Ordering::Acquire),
        }
    }

    pub fn shutdown(self) {
        self.handle.shutdown();
    }
}
