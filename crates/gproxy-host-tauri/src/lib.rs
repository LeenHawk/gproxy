//! The GPROXY v4 desktop host.
//!
//! One process, one instance, two front doors.
//!
//! ```text
//!         the window                     Claude Code, the Codex CLI
//!             │                                      │
//!        Tauri IPC                              HTTP, 127.0.0.1
//!             │                                      │
//!      ipc::table (this crate)          gproxy-host-axum, data plane only
//!             └───────────────┬──────────────────────┘
//!                        one gproxy_app::App
//!                    one Gproxy · one snapshot · one cache
//! ```
//!
//! # Why the data plane is HTTP even here
//!
//! The clients of a gateway's data plane are other programs, and they speak
//! HTTP. A desktop shell cannot give them IPC, so it runs the real host for
//! them on loopback rather than pretending to be one. That is the arrangement
//! `design/crates.md` specifies, and the reason the tauri column of its
//! endpoint table has a cross in the data-plane row: this crate does not
//! implement a data plane, it hosts the one that exists.
//!
//! The management surfaces go the other way. They are the window's, so they
//! are IPC commands over the typed operations, and the embedded HTTP host
//! refuses `/admin/api` and `/portal/api` outright.
//!
//! # What this crate deliberately does not do
//!
//! Auto-update, launch-at-login, a tray icon, and mobile. Those are v3
//! packaging concerns, and every one of them is a decision about how software
//! is *distributed* rather than about what it does. They can be added when
//! somebody is actually running the desktop shell and wants them; adding them
//! first would mean maintaining an update channel for an application with no
//! users.
//!
//! It also does not build the console. P13 and P14 do, into `ui/`, from the
//! same source the server serves — see [`ipc`] for the one seam that differs.
//!
//! # Reused rather than reimplemented
//!
//! [`gproxy`] — the command line's library half — already turns an
//! `AppConfig` into a running instance, rotates the master key, creates the
//! first administrator and installs the log subscriber. All four are called
//! here instead of being written again. What the CLI has that a window cannot
//! use is its *configuration layering*: `clap` over the environment over
//! `.env` is the operator's interface, and a desktop application has none of
//! those three. [`config`] builds the same `Settings` from the two sources a
//! desktop instance does have.
//!
//! # Starting it
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use gproxy_host_tauri::{Desktop, secrets::Keychain};
//!
//! let desktop = Desktop::start("/home/me/.local/share/gproxy".into(), &Keychain).await?;
//! println!("data plane on {}", desktop.data_plane().base_url);
//! desktop.shutdown();
//! # Ok(())
//! # }
//! ```
//!
//! [`run`] does the same thing inside a Tauri window, which is what the
//! `gproxy-desktop` binary calls.

pub mod config;
pub mod dataplane;
pub mod desktop;
pub mod error;
pub mod ipc;
pub mod secrets;

pub use desktop::{DataPlane, Desktop, SecretPlacement};
pub use error::{IpcError, IpcResult, StartError, StartResult};
pub use ipc::{OPERATIONS, Operation};

use tauri::Manager;

/// Open the window and run until it closes.
///
/// The order matters and is the opposite of what it looks like: the instance
/// is assembled in Tauri's `setup` hook, *before* the window is shown, so a
/// failure to open the database or a master key that has gone missing is a
/// startup error with a message rather than an empty window that does not
/// work. `setup` runs on the main thread, so the assembly is driven on the
/// runtime Tauri already has.
pub fn run() -> StartResult<()> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| StartError::io("starting the async runtime", error))?;
    let handle = runtime.handle().clone();

    tauri::Builder::default()
        .invoke_handler(ipc::invoke_handler::<tauri::Wry>())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let desktop = handle.block_on(Desktop::start(data_dir, &secrets::Keychain))?;
            app.manage(desktop);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The instance owns a listening socket and a background sync, and
            // a process that exits without stopping them leaves the socket in
            // `TIME_WAIT` and the next launch unable to bind the fixed port.
            if let tauri::WindowEvent::Destroyed = event
                && let Some(desktop) = window.try_state::<Desktop>()
            {
                desktop.shutdown();
            }
        })
        .run(tauri::generate_context!())
        .map_err(|error| StartError::App(gproxy_app::AppError::internal(error.to_string())))
}

/// Wall clock in milliseconds. The operations that take one take it from the
/// host, never from the webview: a caller that could choose "now" could read a
/// budget window that has not opened or purge sessions that have not expired.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
