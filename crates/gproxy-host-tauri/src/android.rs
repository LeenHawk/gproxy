//! What Android needs that a desktop does not, and nothing more.
//!
//! Tauri gives the phone a WebView, an IPC bridge and an APK. It does not
//! give it any of the three behaviours a gateway on a phone actually needs,
//! and all three existed in v3's hand-written Java before this crate did:
//!
//! | v3, hand-written Java | here, Kotlin under `gen/android/app/src/main/java/dev/gproxy/desktop/` |
//! |---|---|
//! | `scripts/android/GproxyService.java.in` | `GproxyService.kt`, over [`start`] |
//! | `scripts/android/GproxyBootReceiver.java.in` | `GproxyBootReceiver.kt`, same service |
//! | `scripts/android/GproxyUpdateActivity.java.in` | `GproxyUpdateActivity.kt` |
//! | `scripts/android/GproxyUpdateProvider.java.in` | `GproxyUpdateProvider.kt` |
//! | `scripts/android/AndroidManifest.xml.in` | `AndroidManifest.xml`, merged by Gradle |
//!
//! # The one thing that is different from v3, and it changes everything
//!
//! v3's service **spawned a binary**. It copied `gproxy.bin` out of the
//! assets, set `LD_LIBRARY_PATH`, started a child process, polled
//! `http://127.0.0.1:8787/admin` until it answered, and read the child's
//! stdout into a ring buffer. The app and the gateway were two processes, and
//! the service's job was to supervise the other one.
//!
//! Here there is no other one. The engine is compiled into
//! `libgproxy_host_tauri.so`, the activity loads it, and the data plane
//! listens from inside the app's own process. So the service's job is the
//! opposite of supervision: it exists to **stop Android from killing the
//! process the engine is already in**. An app with no foreground service is
//! frozen and then killed shortly after the user leaves it, and a gateway that
//! dies when you switch apps is not a gateway.
//!
//! That is also why there is no health poll here. v3 polled because it could
//! not see inside the child; [`crate::engine::ensure_started`] returns the
//! assembled instance or the error that stopped it, on the calling thread.
//!
//! # Who starts what
//!
//! ```text
//!   GproxyNative.configure(dir)   ← the activity and the service, both, first
//!            │
//!   GproxyNative.start()          ← the service's worker thread
//!            │                       (and Tauri's setup, whichever is first)
//!   engine::ensure_started
//! ```
//!
//! The data directory is the **Java side's** to decide, which is the one place
//! this module departs from the desktop. On the desktop Tauri's path resolver
//! answers it. On Android the boot receiver starts the service with no
//! activity in existence, so there is no Tauri app to ask — and a service and
//! an activity that each resolved the path themselves would be one bug away
//! from two databases. One authority, set before anything starts.
//!
//! # Secrets on a phone
//!
//! [`keyring`] has no Android backend: every call returns
//! `Invalid("platform", …)`, which [`crate::secrets`] already reads as "there
//! is no keychain here". So on Android the master key is **not minted and
//! upstream credentials are stored in the clear**, exactly as `gproxy serve`
//! behaves with no `GPROXY_MASTER_KEY`, and `desktop_instance_status` reports
//! `secretsAreSealed: false` so the console can say so. The gateway key falls
//! back to its `0600` file as designed.
//!
//! Android's per-application UID makes that a weaker problem than it is on a
//! desktop — the data directory is unreadable by other apps rather than by
//! other processes of the same user — but it is not no problem: `adb`,
//! a backup agent and root all read it. Wrapping the master key in the
//! **Android Keystore** through a [`SecretStore`](crate::secrets::SecretStore)
//! implementation is the named follow-up, and the trait is why it is a new
//! file rather than a change to the eight call sites.

use std::{
    ffi::{CString, c_char, c_int},
    path::PathBuf,
    sync::OnceLock,
};

use jni::{
    JNIEnv,
    objects::{JObject, JString},
    sys::jstring,
};
use serde::Serialize;

use crate::{StartError, StartResult, engine};

/// The logcat tag. `adb logcat -s gproxy:*` is the whole debugging story on a
/// device, so it is one word and it is the product's name.
const TAG: &str = "gproxy";

/// `ANDROID_LOG_INFO`, from `<android/log.h>`. Everything the pump forwards is
/// already a formatted `tracing` line carrying its own level, so re-deriving a
/// priority here would be guessing at text somebody else has already
/// classified.
const ANDROID_LOG_INFO: c_int = 4;

unsafe extern "C" {
    /// `liblog`, linked by `build.rs` for the Android target only.
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// Where this instance lives, as the Java side decided.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The data directory, or the refusal that says who was supposed to set it.
///
/// A missing value is a wiring bug and not a condition to paper over: a
/// default guessed here would be a second opinion about where the database is,
/// and the failure mode of getting that wrong is a phone with two of them.
pub fn data_dir() -> StartResult<PathBuf> {
    DATA_DIR.get().cloned().ok_or_else(|| {
        StartError::App(gproxy_app::AppError::internal(
            "this Android process was never told where its data directory is: \
             GproxyNative.configure() runs before the activity's super.onCreate() \
             and before the service starts the instance, and neither happened",
        ))
    })
}

/// Record the data directory. The first call wins; later ones are ignored
/// rather than refused, because the activity and the service both make it and
/// they are both passing the same `File(filesDir, "gproxy")`.
pub fn set_data_dir(path: PathBuf) {
    if let Err(ignored) = DATA_DIR.set(path)
        && DATA_DIR.get() != Some(&ignored)
    {
        tracing::warn!(
            requested = %ignored.display(),
            in_force = %DATA_DIR.get().map(|path| path.display().to_string()).unwrap_or_default(),
            "two data directories were configured in one process; keeping the first"
        );
    }
}

/// Put the log where a person can read it, once per process.
///
/// Android throws away a process's standard output, and this process has no
/// terminal behind it. So the two file descriptors are spliced into a pipe and
/// a thread forwards whole lines to `liblog` — which means `gproxy`'s own text
/// subscriber, installed unchanged, ends up in `adb logcat` without this crate
/// owning a second opinion about log formatting. Installing a `tracing` layer
/// instead would have meant exactly that second opinion, and would have missed
/// everything a dependency prints directly.
pub fn install_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        redirect_standard_streams();
        if let Err(error) = gproxy::telemetry::init(&Default::default()) {
            write_line(&format!("gproxy: {error}"));
        }
    });
}

fn redirect_standard_streams() {
    let mut ends = [0 as c_int; 2];
    // SAFETY: `ends` is a two-element array, which is what `pipe` writes. The
    // `dup2` calls replace descriptors 1 and 2 before anything in this process
    // has written to either — `install_logging` is the first thing the entry
    // point does.
    let read = unsafe {
        if libc::pipe(ends.as_mut_ptr()) != 0 {
            return;
        }
        let (read, write) = (ends[0], ends[1]);
        libc::dup2(write, libc::STDOUT_FILENO);
        libc::dup2(write, libc::STDERR_FILENO);
        libc::close(write);
        read
    };
    let spawned = std::thread::Builder::new()
        .name("gproxy-logcat".to_owned())
        .spawn(move || pump(read));
    if spawned.is_err() {
        // SAFETY: nothing else holds this descriptor; the thread that would
        // have owned it was never created.
        unsafe { libc::close(read) };
    }
}

/// Read the pipe forever, forwarding one log entry per line.
///
/// Line-buffered rather than chunk-forwarded because logcat's unit is an
/// entry: a 4 KiB read that happens to split a line mid-word would otherwise
/// become two entries with the seam in the middle of a word.
fn pump(read: c_int) {
    let mut buffer = [0_u8; 4096];
    let mut line = Vec::<u8>::new();
    loop {
        // SAFETY: `read` is the pipe's read end, owned by this thread, and the
        // buffer is `buffer.len()` bytes long.
        let count = unsafe { libc::read(read, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count <= 0 {
            // SAFETY: as above, and nothing reads the descriptor after this.
            unsafe { libc::close(read) };
            return;
        }
        for &byte in &buffer[..count as usize] {
            match byte {
                b'\n' => {
                    write_line(&String::from_utf8_lossy(&line));
                    line.clear();
                }
                // A line that never ends is a line that would grow until the
                // process ran out of memory. `logcat`'s own entries are capped
                // well below this anyway.
                _ if line.len() < 8192 => line.push(byte),
                _ => {}
            }
        }
    }
}

fn write_line(text: &str) {
    let text = text.trim_end();
    if text.is_empty() {
        return;
    }
    // An interior nul would truncate the entry at the C boundary, so it is
    // replaced rather than allowed to silently eat the rest of the line.
    let Ok(message) = CString::new(text.replace('\0', "\u{fffd}")) else {
        return;
    };
    let Ok(tag) = CString::new(TAG) else {
        return;
    };
    // SAFETY: both pointers are nul-terminated and outlive the call.
    unsafe { __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), message.as_ptr()) };
}

/// What the service puts in its notification and what the console shows.
///
/// Flat, small and serialised as JSON because it crosses JNI, where a struct
/// would be a Kotlin data class to keep in step and a JSON string is one
/// `String`. It is not the IPC surface: `desktop_instance_status` is, and it
/// answers the same facts to the window over the typed bridge.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Whether this process has an assembled instance serving the data plane.
    pub running: bool,
    /// What a client on this device puts in front of `/v1/messages`.
    pub base_url: Option<String>,
    /// `false` on Android today; see the module note on secrets.
    pub secrets_are_sealed: bool,
    /// Why there is no instance, when there is none.
    pub error: Option<String>,
}

impl Status {
    fn of(desktop: &crate::Desktop) -> Self {
        Self {
            running: true,
            base_url: Some(desktop.data_plane().base_url.clone()),
            secrets_are_sealed: desktop.secrets().secrets_are_sealed(),
            error: None,
        }
    }

    fn failed(error: &StartError) -> Self {
        Self {
            running: false,
            base_url: None,
            secrets_are_sealed: false,
            error: Some(error.to_string()),
        }
    }

    fn idle() -> Self {
        Self {
            running: false,
            base_url: None,
            secrets_are_sealed: false,
            error: None,
        }
    }

    /// Serialising a struct of four owned scalars cannot fail, and a fallback
    /// that Kotlin can still parse is better than an `unwrap` in a JNI frame.
    fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"running":false,"baseUrl":null,"secretsAreSealed":false,"error":"status could not be rendered"}"#.to_owned()
        })
    }
}

/// Assemble the instance if this process has not already, and say what
/// happened.
///
/// Blocking, on purpose, on whichever thread called: the service calls it from
/// a worker it started for the purpose, and Tauri's `setup` calls the same
/// [`engine::ensure_started`] from the main thread. Whoever arrives second
/// waits on the gate and gets the instance the first one built.
pub fn start() -> Status {
    let outcome = data_dir().and_then(|data_dir| {
        let choices = crate::setup::read_choices(&data_dir)?;
        if !choices.completed {
            return Ok(None);
        }
        engine::runtime()?
            .block_on(engine::ensure_started(&choices.data_dir, crate::store()))
            .map(Some)
    });
    match outcome {
        Ok(None) => Status::idle(),
        Ok(Some(desktop)) => {
            let status = Status::of(&desktop);
            tracing::info!(base_url = ?status.base_url, "the Android instance is serving");
            status
        }
        Err(error) => {
            tracing::error!(%error, "the Android instance could not start");
            Status::failed(&error)
        }
    }
}

/// This process's instance, as the service's notification wants it.
pub fn status() -> Status {
    engine::started().map_or_else(Status::idle, Status::of)
}

// ---------------------------------------------------------------------------
// The JNI surface.
//
// Four functions, all on `dev.gproxy.desktop.GproxyNative`, all of them thin:
// decode, call one function above, encode. A panic unwinding out of an
// `extern "system"` frame aborts the process, so each one catches — a failed
// start has to be a message in a notification, not a dead app.
//
// In a *release* APK it catches nothing, and that is not an oversight: the
// workspace's release profile is `panic = "abort"`, so there is no unwinding
// to intercept and a panic ends the process either way. The guard is worth
// keeping regardless. It is what makes a debug build report the panic through
// the notification instead of vanishing, which is the build somebody is
// running when they are trying to find out why.
// ---------------------------------------------------------------------------

/// Never unwinds into the JVM. See the note above for what this does and does
/// not buy in a release build.
fn guarded<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// A Java `String` for `text`, or null. Null is what Kotlin's platform type
/// already forces the caller to handle, so it is a signal rather than a
/// crash.
fn java_string(env: &mut JNIEnv<'_>, text: &str) -> jstring {
    env.new_string(text)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// `GproxyNative.configure(dir)`: name the data directory and start logging.
///
/// Called by the activity before `super.onCreate()` and by the service before
/// it starts anything, so that the engine is never assembled by a caller that
/// had to guess.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gproxy_desktop_GproxyNative_nativeConfigure(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
    data_dir: JString<'_>,
) {
    guarded((), || {
        install_logging();
        match env.get_string(&data_dir) {
            Ok(path) => set_data_dir(PathBuf::from(String::from(path))),
            Err(error) => tracing::error!(%error, "the configured data directory was unreadable"),
        }
    });
}

/// `GproxyNative.start()`: the status JSON, after assembling if needed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gproxy_desktop_GproxyNative_nativeStart(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let json = start().json();
        java_string(&mut env, &json)
    })
}

/// `GproxyNative.status()`: the status JSON, assembling nothing.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gproxy_desktop_GproxyNative_nativeStatus(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let json = status().json();
        java_string(&mut env, &json)
    })
}

/// Fetch and verify an APK on the update activity's worker thread. Installation
/// stays with Android, which asks the user before replacing the application.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gproxy_desktop_GproxyNative_nativeUpdate(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let result = (|| -> Result<bool, String> {
            let dir = data_dir().map_err(|error| error.to_string())?;
            let updater = gproxy::update::Updater::new(&dir, Default::default())
                .map_err(|error| error.to_string())?;
            engine::runtime()
                .map_err(|error| error.to_string())?
                .block_on(async {
                    let _download = DOWNLOAD.lock().await;
                    updater.stage_apk().await
                })
                .map(|apk| apk.is_some())
                .map_err(|error| error.to_string())
        })();
        let response = match result {
            Ok(ready) => serde_json::json!({"ready": ready}),
            Err(error) => serde_json::json!({"ready": false, "error": error}),
        };
        java_string(&mut env, &response.to_string())
    })
}

/// `GproxyNative.shutdown()`: close the socket and stop the sync.
///
/// The caller ends the process immediately afterwards — see
/// [`crate::engine`] on why stopping is ending the process — so this is the
/// last chance to let the listener go and the background sync finish, and it
/// is why Stop is not just `Process.killProcess`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gproxy_desktop_GproxyNative_nativeShutdown(
    _env: JNIEnv<'_>,
    _this: JObject<'_>,
) {
    guarded((), || {
        tracing::info!("stopping: the foreground service was asked to");
        engine::shutdown();
    });
}
