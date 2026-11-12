use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    // cargo-ndk apunta a .../usr/lib/<triple>/<api>, pero libc++ está en
    // .../usr/lib/<triple>/ (sin versión de API).
    if let Ok(libs) = env::var("CARGO_NDK_SYSROOT_LIBS_PATH") {
        let api_dir = PathBuf::from(libs);
        if let Some(triple_dir) = api_dir.parent() {
            println!("cargo:rustc-link-search=native={}", triple_dir.display());
        }
    }

    // oboe es C++ y no arrastra el runtime por sí solo: sin esto el .so queda
    // con __cxa_pure_virtual & co. sin resolver y dlopen falla.
    println!("cargo:rustc-link-lib=dylib=c++_shared");
}
