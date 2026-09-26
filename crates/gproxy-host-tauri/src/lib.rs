//! The GPROXY v4 application host: one shell for the desktop and for Android.
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
//! The same sentence is why the phone is useful at all: an app on the device
//! points at `http://127.0.0.1:8787` and is talking to this process. It still
//! needs a key, for exactly the reason the desktop's socket does — every
//! process on the device can reach loopback.
//!
//! The management surfaces go the other way. They are the window's, so they
//! are IPC commands over the typed operations, and the embedded HTTP host
//! refuses `/admin/api` and `/portal/api` outright.
//!
//! # The two platforms differ in three places and no others
//!
//! - **who starts the instance.** [`engine`] is a once-per-process instance
//!   that the window starts on the desktop and that a foreground service can
//!   also start on Android — the boot receiver has no window to start one
//!   from.
//! - **who decides the data directory.** Tauri's path resolver on the desktop;
//!   the Java side on Android, because the service can run without an
//!   activity. See [`android`].
//! - **what closing the window means.** On the desktop it means the process is
//!   ending, so the socket is closed on the way out. On Android it means the
//!   user switched apps, and closing the socket there would be the bug.
//!
//! Everything above those three — the command table, the configuration, the
//! secrets, the reduction in front of the router — is one implementation.
//!
//! # First launch and desktop integration
//!
//! New instances show a setup wizard before opening the database. Existing
//! instances start directly. The wizard configures the listener, database,
//! administrator, gateway key, startup behavior and optional import.
//! Desktop builds support a system tray and launch at login; Android uses a
//! foreground service, boot receiver and permission prompts. Desktop automatic
//! updates remain outside this host.
//!
//! It also does not build the console. `pnpm build` in `console/` does, into
//! `ui/`, from the same source the server serves — see [`console`] for how its
//! requests are answered.
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
//! `gproxy-desktop` binary calls and what Android's activity calls through
//! [`start`].

#[cfg(target_os = "android")]
pub mod android;
pub mod config;
pub mod console;
pub mod dataplane;
pub mod desktop;
pub mod engine;
pub mod error;
pub mod ipc;
pub mod secrets;
pub mod setup;
mod startup;
#[cfg(desktop)]
mod tray;

pub use desktop::{DataPlane, Desktop, SecretPlacement};
pub use error::{IpcError, IpcResult, StartError, StartResult};
pub use ipc::{OPERATIONS, Operation};

use tauri::Manager;

/// The credential store a shipped build uses.
///
/// One expression, so that the desktop and the phone cannot drift into two
/// answers. On Android [`keyring`](secrets::Keychain) has no platform store to
/// reach and every call fails, which [`secrets`] already treats as "there is
/// no keychain here" — see that module for what each secret then does. The
/// Android Keystore is a named follow-up rather than a silent gap.
fn store() -> &'static dyn secrets::SecretStore {
    &secrets::Keychain
}

/// Open the window and run until it closes.
///
/// Existing instances are assembled in Tauri's setup hook. New instances only
/// register the first-run state there; the wizard starts the engine after the
/// user chooses its configuration. A failed setup stays pending for retry.
///
/// The assembly goes through [`engine::ensure_started`], so on Android a
/// window opened after the foreground service has already started the instance
/// finds it running and shows it rather than building a second one.
pub fn run() -> StartResult<()> {
    let handle = engine::runtime()?.handle().clone();

    let builder = tauri::Builder::default()
        .invoke_handler(ipc::invoke_handler::<tauri::Wry>())
        .setup(move |app| {
            let data_dir = engine::data_dir(app.handle())?;
            let setup = setup::Setup::new(data_dir);
            let choices = setup.choices()?;
            if choices.completed {
                let desktop =
                    handle.block_on(engine::ensure_started(&choices.data_dir, store()))?;
                app.manage(desktop);
                #[cfg(desktop)]
                if choices.tray {
                    tray::install(app.handle())?;
                }
            }
            app.manage(setup);
            Ok(())
        });

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_window(app)
        }))
        .plugin(tauri_plugin_dialog::init());

    // Desktop only, and the asymmetry is the point. On the desktop a destroyed
    // window means the process is ending, and an instance that exits without
    // stopping its listener leaves the socket in `TIME_WAIT` and the next
    // launch unable to bind the fixed port. On Android a destroyed window
    // means the user switched apps or swiped the task away; the foreground
    // service is still holding the process up on purpose, and stopping the
    // data plane here would be precisely the failure the service exists to
    // prevent.
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event
            && let Some(setup) = window.try_state::<setup::Setup>()
            && setup
                .choices()
                .is_ok_and(|choices| choices.completed && choices.tray)
        {
            api.prevent_close();
            let _ = window.hide();
            return;
        }
        if let tauri::WindowEvent::Destroyed = event
            && let Some(desktop) = window.try_state::<Desktop>()
        {
            desktop.shutdown();
        }
    });

    builder
        .run(tauri::generate_context!())
        .map_err(|error| StartError::App(gproxy_app::AppError::internal(error.to_string())))
}

/// Android's entry point, called by the activity through JNI.
///
/// There is no `main` on a phone. The activity loads
/// `libgproxy_host_tauri.so` and Tauri's binding calls this, which is why the
/// library carries a `cdylib` crate type; `src/main.rs` is not built for
/// Android at all. The two things it does before [`run`] are the two a
/// process with no terminal needs: put the log somewhere a person can read it,
/// and refuse to lose a startup error to a closed standard error.
#[cfg(target_os = "android")]
#[tauri::mobile_entry_point]
pub fn start() {
    android::install_logging();
    if let Err(error) = run() {
        tracing::error!(%error, "the application could not start");
    }
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
