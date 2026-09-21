//! The window.
//!
//! Everything this binary does is in the library, for the same reason
//! `gproxy`'s is: an integration test cannot reach inside a bare `[[bin]]`,
//! and a shell that held any logic would be a shell that could not be tested
//! without a display server.
//!
//! `windows_subsystem` keeps a console window from appearing behind the
//! application on Windows, and only in a release build — a debug build wants
//! the terminal, because that is where the log goes.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Up before anything can fail, so a startup error is a log line rather
    // than a silent exit. The desktop shell has no `--log-format`, so this is
    // `gproxy`'s text default with `RUST_LOG` still honoured.
    if let Err(error) = gproxy::telemetry::init(&Default::default()) {
        eprintln!("gproxy: {error}");
    }
    if let Err(error) = gproxy_host_tauri::run() {
        tracing::error!(%error, "the desktop shell could not start");
        eprintln!("gproxy: {error}");
        std::process::exit(1);
    }
}
