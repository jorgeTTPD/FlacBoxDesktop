use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::decoder::{DecCmd, DecEvt};
use crate::shared::*;

#[cfg(target_os = "android")]
use crate::output as backend;
#[cfg(not(target_os = "android"))]
use crate::output_cpal as backend;

type StreamHandle = backend::Stream;

pub enum Cmd {
    SetQueue {
        paths: Vec<String>,
        start: usize,
        start_ms: u64,
    },
    Play,
    PlayIndex(usize),
    Pause,
    Stop,
    Seek(u64),
    Next,
    Prev,
    SetRepeat(u8),
    SetShuffle(bool),
    SetVolume(f32),
    Release,
}

pub struct EngineHandle {
    pub shared: Arc<Shared>,
    cmd_tx: Sender<Cmd>,
    threads: Vec<JoinHandle<()>>,
}

impl EngineHandle {
    pub fn new(shared: Arc<Shared>, cmd_tx: Sender<Cmd>, threads: Vec<JoinHandle<()>>) -> Self {
        Self {
            shared,
            cmd_tx,
            threads,
        }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn shutdown(mut self) {
        let _ = self.cmd_tx.send(Cmd::Release);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------- cola

pub struct Queue {
    pub items: Vec<String>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    repeat: u8,
    rng: u64,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            order: Vec::new(),
            pos: 0,
            shuffle: false,
            repeat: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn set_items(&mut self, paths: Vec<String>, start: usize) {
        self.items = paths;
        self.order = (0..self.items.len()).collect();
        self.pos = start.min(self.items.len().saturating_sub(1));
        self.regenerate();
    }

    pub fn current_real_index(&self) -> Option<usize> {
        self.order.get(self.pos).copied()
    }

    pub fn current_path(&self) -> Option<&str> {
        self.current_real_index()
            .and_then(|i| self.items.get(i))
            .map(|s| s.as_str())
    }

    /// Salta a un índice real de la cola (respetando el orden de shuffle).
    pub fn jump_to(&mut self, real_index: usize) -> bool {
        if real_index >= self.items.len() {
            return false;
        }
        match self.order.iter().position(|&x| x == real_index) {
            Some(pos) => {
                self.pos = pos;
                true
            }
            None => false,
        }
    }

    pub fn set_repeat(&mut self, mode: u8) {
        self.repeat = mode.min(2);
    }

    pub fn repeat(&self) -> u8 {
        self.repeat
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        self.regenerate();
    }

    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn regenerate(&mut self) {
        let current = self.current_real_index();
        if self.shuffle {
            let mut order: Vec<usize> = (0..self.items.len()).collect();
            for i in (1..order.len()).rev() {
                let j = (self.next_rand() % (i as u64 + 1)) as usize;
                order.swap(i, j);
            }
            if let Some(cur) = current {
                if let Some(p) = order.iter().position(|&x| x == cur) {
                    order.swap(0, p);
                }
                self.pos = 0;
            } else {
                self.pos = 0;
            }
            self.order = order;
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = current.unwrap_or(0).min(self.order.len().saturating_sub(1));
        }
    }

    /// Avance manual (botón siguiente). Ignora el modo "repetir una".
    pub fn manual_next(&mut self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        match self.peek_advance() {
            Some(v) => Some(v),
            None => {
                if self.repeat == 1 {
                    self.pos = 0;
                    self.current_real_index()
                } else {
                    None
                }
            }
        }
    }

    fn peek_advance(&mut self) -> Option<usize> {
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
            self.current_real_index()
        } else {
            None
        }
    }

    pub fn manual_prev(&mut self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == 1 {
            self.pos = self.order.len() - 1;
        }
        self.current_real_index()
    }

    /// Avance automático al terminar una pista (fin de archivo).
    pub fn on_track_end(&mut self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        if self.repeat == 2 {
            return self.current_real_index();
        }
        if let Some(v) = self.peek_advance() {
            return Some(v);
        }
        if self.repeat == 1 {
            self.pos = 0;
            return self.current_real_index();
        }
        None
    }
}

// ---------------------------------------------------------------- motor

pub fn engine_main(
    shared: Arc<Shared>,
    cmd_rx: Receiver<Cmd>,
    dec_rx: Receiver<DecEvt>,
    dec_tx: Sender<DecCmd>,
    mut emit: impl FnMut(i32, i32, i64, i64, Option<&str>),
) {
    let mut queue = Queue::new();
    let mut stream: Option<StreamHandle> = None;
    let mut loaded = false;
    let mut failures = 0usize;

    macro_rules! open_track {
        ($idx:expr, $start:expr) => {{
            let idx: usize = $idx;
            if let Some(path) = queue.items.get(idx).cloned() {
                shared.set_status(STATUS_LOADING);
                emit(EVENT_STATUS, idx as i32, i64::from(STATUS_LOADING), 0, None);
                let _ = dec_tx.send(DecCmd::Open {
                    index: idx,
                    path,
                    start_ms: $start,
                });
                loaded = true;
            }
        }};
    }

    loop {
        crossbeam_channel::select! {
            recv(cmd_rx) -> msg => match msg {
                Ok(cmd) => {
                    match cmd {
                        Cmd::SetQueue { paths, start, start_ms } => {
                            queue.set_items(paths, start);
                            failures = 0;
                            // Vacía el anillo sin destruir el stream: si el sample
                            // rate cambia, el decodificador pedirá uno nuevo.
                            shared.reset_position(0);
                            if let Some(idx) = queue.current_real_index() {
                                open_track!(idx, start_ms);
                            }
                        }
                        Cmd::PlayIndex(idx) => {
                            if queue.jump_to(idx) {
                                open_track!(idx, 0);
                            }
                        }
                        Cmd::Play => {
                            if !loaded || shared.status() == STATUS_IDLE || shared.status() == STATUS_ENDED {
                                if let Some(idx) = queue.current_real_index() {
                                    open_track!(idx, 0);
                                }
                            }
                            shared.playing.store(true, Ordering::Release);
                            if shared.status() != STATUS_LOADING {
                                shared.set_status(STATUS_PLAYING);
                                emit(EVENT_STATUS, shared.index.load(Ordering::Acquire), i64::from(STATUS_PLAYING), shared.position_ms(), None);
                            }
                        }
                        Cmd::Pause => {
                            shared.playing.store(false, Ordering::Release);
                            if shared.status() == STATUS_PLAYING {
                                shared.set_status(STATUS_PAUSED);
                                emit(EVENT_STATUS, shared.index.load(Ordering::Acquire), i64::from(STATUS_PAUSED), shared.position_ms(), None);
                            }
                        }
                        Cmd::Stop => {
                            shared.playing.store(false, Ordering::Release);
                            let _ = dec_tx.send(DecCmd::Stop);
                            shared.reset_position(0);
                            shared.set_status(STATUS_IDLE);
                            shared.index.store(-1, Ordering::Release);
                            loaded = false;
                            emit(EVENT_STATUS, -1, i64::from(STATUS_IDLE), 0, None);
                        }
                        Cmd::Seek(ms) => {
                            let _ = dec_tx.send(DecCmd::Seek(ms));
                        }
                        Cmd::Next => {
                            if let Some(idx) = queue.manual_next() {
                                open_track!(idx, 0);
                            }
                        }
                        Cmd::Prev => {
                            if let Some(idx) = queue.manual_prev() {
                                open_track!(idx, 0);
                            }
                        }
                        Cmd::SetRepeat(mode) => queue.set_repeat(mode),
                        Cmd::SetShuffle(on) => queue.set_shuffle(on),
                        Cmd::SetVolume(v) => shared.set_volume(v),
                        Cmd::Release => break,
                    }
                }
                Err(_) => break,
            },
            recv(dec_rx) -> msg => match msg {
                Ok(evt) => match evt {
                    DecEvt::NeedStream { index, rate } => {
                        stream = None;
                        match backend::build_stream(rate, shared.clone()) {
                            Ok((s, prod)) => {
                                let actual = shared.stream_rate.load(Ordering::Acquire);
                                stream = Some(s);
                                let _ = dec_tx.send(DecCmd::NewProducer(prod, actual));
                                emit(EVENT_STREAM_INFO, index as i32, i64::from(actual), i64::from(rate), None);
                            }
                            Err(msg) => {
                                shared.set_status(STATUS_ERROR);
                                emit(EVENT_ERROR, index as i32, 0, 0, Some(&msg));
                                let _ = dec_tx.send(DecCmd::Stop);
                            }
                        }
                    }
                    DecEvt::TrackStarted { index, duration_ms, source_rate, stream_rate, bits, channels, codec } => {
                        failures = 0;
                        shared.index.store(index as i32, Ordering::Release);
                        shared.duration_ms.store(duration_ms, Ordering::Release);
                        shared.source_rate.store(source_rate, Ordering::Release);
                        shared.bits.store(bits, Ordering::Release);
                        shared.channels.store(channels as u32, Ordering::Release);
                        let playing = shared.playing.load(Ordering::Acquire);
                        let status = if playing { STATUS_PLAYING } else { STATUS_PAUSED };
                        shared.set_status(status);
                        emit(
                            EVENT_TRACK_STARTED,
                            index as i32,
                            duration_ms as i64,
                            ((stream_rate as i64) << 32) | (bits as i64),
                            Some(&codec),
                        );
                        emit(EVENT_STATUS, index as i32, i64::from(status), 0, None);
                    }
                    DecEvt::TrackEnded { index } => {
                        emit(EVENT_TRACK_ENDED, index as i32, shared.position_ms(), 0, None);
                        match queue.on_track_end() {
                            Some(next) => {
                                if failures > queue.len() {
                                    shared.playing.store(false, Ordering::Release);
                                    shared.set_status(STATUS_ENDED);
                                    emit(EVENT_QUEUE_ENDED, -1, 0, 0, None);
                                } else {
                                    open_track!(next, 0);
                                }
                            }
                            None => {
                                shared.playing.store(false, Ordering::Release);
                                shared.set_status(STATUS_ENDED);
                                loaded = false;
                                emit(EVENT_QUEUE_ENDED, index as i32, 0, 0, None);
                            }
                        }
                    }
                    DecEvt::Error { index, msg } => {
                        failures += 1;
                        emit(EVENT_ERROR, index as i32, 0, 0, Some(&msg));
                    }
                },
                Err(_) => break,
            },
            default(Duration::from_millis(40)) => {}
        }
    }

    let _ = dec_tx.send(DecCmd::Stop);
    shared.playing.store(false, Ordering::Release);
    #[cfg(target_os = "android")]
    drop(stream);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cola() -> Queue {
        let mut q = Queue::new();
        q.set_items(vec!["a".into(), "b".into(), "c".into()], 0);
        q
    }

    #[test]
    fn avanza_en_orden_sin_shuffle() {
        let mut q = cola();
        assert_eq!(q.current_path(), Some("a"));
        assert_eq!(q.on_track_end(), Some(1));
        assert_eq!(q.current_path(), Some("b"));
        assert_eq!(q.on_track_end(), Some(2));
        assert_eq!(q.current_path(), Some("c"));
    }

    #[test]
    fn para_al_final_sin_repeat() {
        let mut q = cola();
        q.on_track_end();
        q.on_track_end();
        assert_eq!(q.on_track_end(), None);
    }

    #[test]
    fn repeat_all_vuelve_al_principio() {
        let mut q = cola();
        q.set_repeat(1);
        q.on_track_end();
        q.on_track_end();
        assert_eq!(q.on_track_end(), Some(0));
    }

    #[test]
    fn repeat_one_repite_la_misma() {
        let mut q = cola();
        q.set_repeat(2);
        q.on_track_end();
        assert_eq!(q.current_path(), Some("a"));
        assert_eq!(q.on_track_end(), Some(0));
    }

    #[test]
    fn prev_al_inicio_se_queda() {
        let mut q = cola();
        assert_eq!(q.manual_prev(), Some(0));
    }

    #[test]
    fn shuffle_conserva_todas_las_pistas() {
        let mut q = cola();
        q.set_shuffle(true);
        assert_eq!(q.len(), 3);
        let mut seen: Vec<usize> = Vec::new();
        for _ in 0..3 {
            seen.push(q.current_real_index().unwrap());
            q.on_track_end();
        }
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2]);
    }

    #[test]
    fn shuffle_empieza_por_la_pista_actual() {
        let mut q = cola();
        q.on_track_end();
        assert_eq!(q.current_path(), Some("b"));
        q.set_shuffle(true);
        assert_eq!(q.current_path(), Some("b"));
    }
}
