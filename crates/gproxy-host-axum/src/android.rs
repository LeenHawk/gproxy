//! Android runs one binary two ways: the `.apk` starts it from the packaged
//! app's foreground service, and the `.zip` is unpacked into Termux and typed
//! at a shell prompt. Both artifacts come out of the same build, so
//! `INSTALLATION_KIND` describes how the binary was *built* and cannot say how
//! it is *running* — that is probed here instead.

use std::path::PathBuf;

/// Set by `GproxyService` on the process it launches.
const APP_MARKER: &str = "GPROXY_ANDROID_APP";

pub(crate) enum Runtime {
    /// Launched by the packaged app: the app owns startup and updates itself
    /// by installing a new APK.
    App,
    /// A Termux session, with the shell the boot add-on needs for a shebang.
    Termux { home: PathBuf, shell: PathBuf },
    /// An adb shell, an init script, a rooted launcher — no boot integration.
    Bare,
}

impl Runtime {
    pub(crate) fn is_app(&self) -> bool {
        matches!(self, Self::App)
    }
}

pub(crate) fn runtime() -> Runtime {
    if std::env::var_os(APP_MARKER).is_some() {
        return Runtime::App;
    }
    let (Some(home), Some(prefix)) = (std::env::var_os("HOME"), std::env::var_os("PREFIX")) else {
        return Runtime::Bare;
    };
    // Termux forks relocate the prefix, so the shell is taken from $PREFIX
    // rather than assuming `com.termux`.
    let shell = PathBuf::from(prefix).join("bin/sh");
    if shell.is_file() {
        Runtime::Termux {
            home: home.into(),
            shell,
        }
    } else {
        Runtime::Bare
    }
}
