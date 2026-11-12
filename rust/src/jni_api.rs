use std::os::raw::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crossbeam_channel::bounded;
use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jfloat, jint, jlong, jlongArray, jstring, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use rtrb::RingBuffer;

use crate::decoder::{DecCmd, DecEvt, DecoderThread};
use crate::engine::{engine_main, Cmd, EngineHandle};
use crate::shared::Shared;

fn emitter(vm: JavaVM, cb: GlobalRef) -> impl FnMut(i32, i32, i64, i64, Option<&str>) {
    move |kind: i32, index: i32, a: i64, b: i64, text: Option<&str>| {
        let Ok(mut env) = vm.attach_current_thread_permanently() else {
            return;
        };
        let null = JObject::null();
        let text_obj = match text {
            Some(t) => match env.new_string(t) {
                Ok(js) => JObject::from(js),
                Err(_) => JObject::null(),
            },
            None => null,
        };
        let res = env.call_method(
            cb.as_obj(),
            "onEvent",
            "(IIJJLjava/lang/String;)V",
            &[
                JValue::Int(kind),
                JValue::Int(index),
                JValue::Long(a),
                JValue::Long(b),
                JValue::Object(&text_obj),
            ],
        );
        if res.is_err() {
            let _ = env.exception_clear();
        }
    }
}

fn handle<'a>(ptr: jlong) -> Option<&'a EngineHandle> {
    if ptr == 0 {
        return None;
    }
    Some(unsafe { &*(ptr as *const EngineHandle) })
}

macro_rules! guard {
    ($default:expr, $body:block) => {
        match catch_unwind(AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "panic nativo".to_string()
                };
                log::error!("panic en JNI: {msg}");
                $default
            }
        }
    };
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_create(
    env: JNIEnv,
    _this: JObject,
    callback: JObject,
) -> jlong {
    guard!(0, {
        let vm = match env.get_java_vm() {
            Ok(vm) => vm,
            Err(_) => return 0,
        };
        let cb = match env.new_global_ref(callback) {
            Ok(cb) => cb,
            Err(_) => return 0,
        };

        let shared = Arc::new(Shared::new());
        let (cmd_tx, cmd_rx) = bounded::<Cmd>(64);
        let (dec_tx, dec_rx) = bounded::<DecCmd>(64);
        let (evt_tx, evt_rx) = bounded::<DecEvt>(256);
        let (prod, cons) = RingBuffer::<f32>::new(1024);
        std::mem::forget(cons);

        let mut threads = Vec::with_capacity(2);

        let dec_shared = shared.clone();
        match DecoderThread::spawn(dec_shared, dec_rx, evt_tx, prod) {
            h => threads.push(h),
        }

        let vm_for_engine = vm;
        let eng_shared = shared.clone();
        match std::thread::Builder::new()
            .name("flacbox-engine".into())
            .spawn(move || {
                let emit = emitter(vm_for_engine, cb);
                engine_main(eng_shared, cmd_rx, evt_rx, dec_tx, emit);
            }) {
            Ok(h) => threads.push(h),
            Err(e) => {
                log::error!("no se pudo crear el hilo de control: {e}");
                return 0;
            }
        }

        let handle = Box::new(EngineHandle::new(shared, cmd_tx, threads));
        Box::into_raw(handle) as jlong
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_release(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if ptr == 0 {
            return;
        }
        let handle = unsafe { Box::from_raw(ptr as *mut EngineHandle) };
        handle.shutdown();
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_setQueue(
    mut env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    paths: JObjectArray,
    start: jint,
    start_ms: jlong,
) {
    guard!((), {
        let Some(h) = handle(ptr) else { return };
        let len = match env.get_array_length(&paths) {
            Ok(l) => l,
            Err(_) => return,
        };
        let mut list = Vec::with_capacity(len.max(0) as usize);
        for i in 0..len {
            let Ok(elem) = env.get_object_array_element(&paths, i) else {
                continue;
            };
            let js = JString::from(elem);
            let value = env.get_string(&js).map(String::from);
            if let Ok(s) = value {
                list.push(s);
            }
        }
        h.send(Cmd::SetQueue {
            paths: list,
            start: start.max(0) as usize,
            start_ms: start_ms.max(0) as u64,
        });
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_play(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Play);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_pause(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Pause);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_stop(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Stop);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_next(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Next);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_playIndex(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    index: jint,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::PlayIndex(index.max(0) as usize));
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_previous(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Prev);
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_seekTo(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    ms: jlong,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::Seek(ms.max(0) as u64));
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_setRepeat(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    mode: jint,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::SetRepeat(mode.clamp(0, 2) as u8));
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_setShuffle(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    on: jboolean,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::SetShuffle(on == JNI_TRUE));
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_setVolume(
    _env: JNIEnv,
    _this: JObject,
    ptr: jlong,
    volume: jfloat,
) {
    guard!((), {
        if let Some(h) = handle(ptr) {
            h.send(Cmd::SetVolume(volume));
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_snapshot(
    env: JNIEnv,
    _this: JObject,
    ptr: jlong,
) -> jlongArray {
    guard!(std::ptr::null_mut(), {
        let Some(h) = handle(ptr) else {
            return std::ptr::null_mut();
        };
        let s = &h.shared;
        let values: [jlong; 12] = [
            s.position_ms(),
            s.duration_ms.load(Ordering::Acquire) as jlong,
            s.index.load(Ordering::Acquire) as jlong,
            s.status() as jlong,
            s.source_rate.load(Ordering::Acquire) as jlong,
            s.stream_rate.load(Ordering::Acquire) as jlong,
            s.bits.load(Ordering::Acquire) as jlong,
            s.channels.load(Ordering::Acquire) as jlong,
            s.underruns.load(Ordering::Acquire) as jlong,
            s.level.load(Ordering::Acquire) as jlong,
            i64::from(s.playing.load(Ordering::Acquire)) as jlong,
            s.seek_base_frames.load(Ordering::Acquire) as jlong,
        ];
        match env.new_long_array(12) {
            Ok(arr) => {
                if env.set_long_array_region(&arr, 0, &values).is_err() {
                    return std::ptr::null_mut();
                }
                arr.into_raw()
            }
            Err(_) => std::ptr::null_mut(),
        }
    })
}

fn jstring_or_null(env: &mut JNIEnv, value: String) -> jstring {
    match env.new_string(value) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_readMeta(
    mut env: JNIEnv,
    _this: JObject,
    path: JString,
) -> jstring {
    guard!(std::ptr::null_mut(), {
        let path: String = match env.get_string(&path) {
            Ok(p) => p.into(),
            Err(_) => return std::ptr::null_mut(),
        };
        let json = crate::metadata::read_meta(&path);
        jstring_or_null(&mut env, json)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_probe(
    mut env: JNIEnv,
    _this: JObject,
    path: JString,
) -> jstring {
    guard!(std::ptr::null_mut(), {
        let path: String = match env.get_string(&path) {
            Ok(p) => p.into(),
            Err(_) => return std::ptr::null_mut(),
        };
        let json = crate::metadata::probe(&path);
        jstring_or_null(&mut env, json)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_extractArt(
    mut env: JNIEnv,
    _this: JObject,
    path: JString,
    out_path: JString,
) -> jboolean {
    guard!(JNI_FALSE, {
        let path: String = match env.get_string(&path) {
            Ok(p) => p.into(),
            Err(_) => return JNI_FALSE,
        };
        let out: String = match env.get_string(&out_path) {
            Ok(p) => p.into(),
            Err(_) => return JNI_FALSE,
        };
        if crate::metadata::extract_art(&path, &out) {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jorge_flacbox_audio_NativeAudio_version(
    mut env: JNIEnv,
    _this: JObject,
) -> jstring {
    guard!(std::ptr::null_mut(), {
        jstring_or_null(&mut env, env!("CARGO_PKG_VERSION").to_string())
    })
}

#[allow(dead_code)]
fn _unused(_: *mut c_void) {}
