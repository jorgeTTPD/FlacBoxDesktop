pub mod decoder;
pub mod engine;
#[cfg(target_os = "android")]
mod jni_api;
pub mod metadata;
pub mod player;
pub mod shared;
pub mod tags;
#[cfg(target_os = "android")]
mod output;
#[cfg(not(target_os = "android"))]
mod output_cpal;

#[cfg(target_os = "android")]
mod android_log {
    use std::ffi::CString;

    extern "C" {
        fn __android_log_write(
        prio: i32,
        tag: *const std::os::raw::c_char,
        text: *const std::os::raw::c_char,
    ) -> i32;
    }

    struct AndroidLog;

    impl log::Log for AndroidLog {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record) {
            let tag = c"flacbox";
            let msg = format!("[{}] {}", record.level(), record.args());
            if let Ok(msg) = CString::new(msg) {
                unsafe {
                    __android_log_write(4, tag.as_ptr(), msg.as_ptr());
                }
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: AndroidLog = AndroidLog;

    pub fn install() {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(_vm: jni::JavaVM, _reserved: *mut std::os::raw::c_void) -> jni::sys::jint {
    android_log::install();
    log::info!("flacplayer nativo cargado");
    jni::sys::JNI_VERSION_1_6
}
