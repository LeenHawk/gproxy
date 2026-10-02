//! Tauri's build step: it reads `tauri.conf.json`, embeds it, and generates
//! the capability/permission tables `tauri::generate_context!` expands into.
//! It is also what sets `cfg(desktop)` and `cfg(mobile)`, and the
//! `TAURI_ANDROID_PACKAGE_NAME_*` variables that `#[tauri::mobile_entry_point]`
//! reads to name the JNI symbols the activity looks for.
//!
//! The one thing added to it is a library to link, and only on Android:
//! `liblog` is where `android::install_logging` sends this process's standard
//! output, because Android discards it otherwise and there is no terminal
//! behind a phone.
//!
//! There is no frontend build here. The console is built by its own toolchain
//! into `ui/`, and a Rust build that shelled out to `pnpm` would make
//! `cargo check` depend on Node.
fn main() {
    println!("cargo:rerun-if-env-changed=GPROXY_ANDROID_DISTRIBUTION");
    println!("cargo:rustc-check-cfg=cfg(store_distribution)");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        let distribution =
            std::env::var("GPROXY_ANDROID_DISTRIBUTION").unwrap_or_else(|_| "direct".to_owned());
        match distribution.as_str() {
            "direct" => {}
            "fdroid" | "google-play" | "appgallery" => {
                println!("cargo:rustc-cfg=store_distribution");
            }
            _ => panic!("unknown GPROXY_ANDROID_DISTRIBUTION: {distribution}"),
        }
    }
    tauri_build::build();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-lib=log");
    }
}
