//! Tauri's build step: it reads `tauri.conf.json`, embeds it, and generates
//! the capability/permission tables `tauri::generate_context!` expands into.
//!
//! Nothing else happens here. In particular there is no frontend build: the
//! console is built by its own toolchain into `ui/`, and a Rust build that
//! shelled out to `pnpm` would make `cargo check` depend on Node.
fn main() {
    tauri_build::build();
}
