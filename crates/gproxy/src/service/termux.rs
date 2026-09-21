//! Termux on Android, as a `~/.termux/boot` script.
//!
//! # There is no service manager here
//!
//! Termux is a userland inside an ordinary Android application's data
//! directory. There is no systemd, no launchd, no init to talk to, and no way
//! to ask the system to keep a process alive — Android's own answer to a
//! long-running process is a foreground service with a notification, which a
//! command-line program in someone else's app cannot register.
//!
//! What exists is the **Termux:Boot** add-on: a separate app that, on
//! `BOOT_COMPLETED`, runs every executable file in `~/.termux/boot` once. So
//! "install a service" here means writing one shell script, and everything a
//! supervisor would have done the script has to do itself:
//!
//! - **detach**, with `setsid`, because Termux:Boot waits for what it started
//!   and a script that blocks holds the boot job open;
//! - **redirect**, because there is nowhere for stderr to go otherwise;
//! - **hold a wake lock**, with `termux-wake-lock`, because Android will
//!   otherwise suspend the process and the gateway stops answering;
//! - **carry `LD_LIBRARY_PATH`**, because a gproxy built for Termux links the
//!   NDK's `libc++_shared.so`, which sits beside the executable and is not on
//!   the default search path in a boot context.
//!
//! What it cannot do is restart on a crash. That is stated in the output
//! rather than papered over with a `while true` loop, which would turn a
//! misconfiguration into an infinite restart storm on a phone.
//!
//! # Detection
//!
//! `PREFIX` containing `com.termux`, and nothing else. `uname` says `Linux`;
//! `std::env::consts::OS` says `android`; neither separates Termux from a
//! packaged Android app that bundles this binary, and the packaged app has no
//! `~/.termux/boot` for the add-on to read. `PREFIX` is set by the Termux
//! shell itself and names the package whose data directory the userland lives
//! in, which is exactly the question being asked.

use std::path::{Path, PathBuf};

use super::{Plan, Report, Result, remove, write_private_script};

/// The file name Termux:Boot will find. Any name works — the add-on runs
/// everything in the directory — so this one is named after what it starts.
pub const SCRIPT: &str = "gproxy.sh";

/// The package a Termux `PREFIX` lives under.
const TERMUX_PACKAGE: &str = "com.termux";

/// Termux's `$HOME` and `$PREFIX`, or `None` on anything that is not Termux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Termux {
    pub home: PathBuf,
    pub prefix: PathBuf,
}

pub fn detect() -> Option<Termux> {
    from_environment(
        std::env::var_os("PREFIX").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// The detection, as a pure function of the two variables, so a test on any
/// machine can assert what counts and what does not.
fn from_environment(prefix: Option<PathBuf>, home: Option<PathBuf>) -> Option<Termux> {
    let prefix = prefix?;
    if !prefix
        .components()
        .any(|part| part.as_os_str().to_string_lossy().contains(TERMUX_PACKAGE))
    {
        return None;
    }
    Some(Termux {
        home: home?,
        prefix,
    })
}

impl Termux {
    /// `~/.termux/boot/gproxy.sh` — the one path Termux:Boot reads.
    pub fn boot_script(&self) -> PathBuf {
        self.home.join(".termux/boot").join(SCRIPT)
    }

    /// The shell the shebang names. Termux's `sh` is inside `PREFIX`; there is
    /// no `/bin/sh` on Android.
    pub fn shell(&self) -> PathBuf {
        self.prefix.join("bin/sh")
    }
}

// ------------------------------------------------------------ the script --

/// The boot script for `plan`.
pub fn script(plan: &Plan, shell: &Path, log: &Path) -> String {
    let command = plan
        .command()
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "#!{shell}\n\
         # Written by `gproxy service install`. Termux:Boot runs every executable\n\
         # file in ~/.termux/boot once, after BOOT_COMPLETED.\n\
         \n\
         # Android suspends a process that nothing is holding awake, and a suspended\n\
         # gateway stops answering. Not fatal if termux-api is not installed.\n\
         command -v termux-wake-lock > /dev/null 2>&1 && termux-wake-lock\n\
         \n\
         cd {working_dir} || exit 1\n\
         {env_file}\
         {library_path}\
         set -- {command}\n\
         # Detach: Termux:Boot waits for what it starts, and a boot job that never\n\
         # returns is a boot job Android eventually kills.\n\
         command -v setsid > /dev/null 2>&1 && set -- setsid \"$@\"\n\
         \"$@\" >> {log} 2>&1 &\n",
        shell = shell.display(),
        working_dir = quote(&plan.working_dir.to_string_lossy()),
        env_file = export("GPROXY_ENV_FILE", &plan.env_file.to_string_lossy()),
        library_path = library_path(&plan.executable),
        command = command,
        log = quote(&log.to_string_lossy()),
    )
}

/// `export NAME='value'`, for the environment *file's path*. Never for a
/// value out of it: this script is `0700`, but it is also in `$HOME` on a
/// device that syncs `$HOME`.
fn export(name: &str, value: &str) -> String {
    format!("{name}={}\nexport {name}\n", quote(value))
}

/// The NDK runtime beside the executable.
///
/// A Termux build of gproxy links `libc++_shared.so`, which ships next to the
/// binary. An interactive Termux shell finds it because the launcher sets the
/// path; a boot script does not, and the failure is
/// `library "libc++_shared.so" not found` at exec time — before any gproxy log
/// line exists to explain it.
fn library_path(executable: &Path) -> String {
    match executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(directory) => format!(
            "LD_LIBRARY_PATH={}\"${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport LD_LIBRARY_PATH\n",
            quote(&directory.to_string_lossy())
        ),
        None => String::new(),
    }
}

/// One POSIX shell word, single-quoted. `'` is closed, escaped and reopened,
/// which is the only form that is safe for every byte a path can hold.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// ----------------------------------------------------------------- verbs --

pub fn install(plan: &Plan) -> Result<Report> {
    let termux = detect()
        .ok_or_else(|| crate::Error::other("gproxy service: this is not a Termux environment"))?;
    std::fs::create_dir_all(&plan.data_dir).map_err(|error| {
        crate::Error::io(format!("creating {}", plan.data_dir.display()), error)
    })?;
    let path = termux.boot_script();
    let log = plan.log_file();
    write_private_script(&path, &script(plan, &termux.shell(), &log))?;

    Ok(
        Report::new("gproxy will start at boot, through Termux:Boot.")
            .row("script", path.display())
            .row("command", plan.command().join(" "))
            .row("data", plan.data_dir.display())
            .row("listen", format!("{}:{}", plan.host, plan.port))
            .row("logs", log.display())
            // The point the operator must not miss: this script is inert without
            // the add-on, and nothing on the device will say so.
            .note(
                "Termux:Boot must be installed, or this script will never run. It is a separate \
             app — install it from F-Droid or the Play Store, then open it once so Android \
             grants it the permission to start at boot. A script in ~/.termux/boot with no \
             Termux:Boot on the device is a file nothing reads, and there is no way for this \
             command to check.",
            )
            .note(
                "Termux has no service manager, so nothing restarts gproxy if it crashes and \
             nothing starts it now — this script runs at the next boot. Start it in this \
             session with `gproxy serve &`, and check on it with `gproxy service status`.",
            )
            .note(plan.key_note()),
    )
}

pub fn uninstall() -> Result<Report> {
    let termux = detect()
        .ok_or_else(|| crate::Error::other("gproxy service: this is not a Termux environment"))?;
    let path = termux.boot_script();
    if remove(&path)? {
        Ok(Report::new("gproxy will no longer start at boot.")
            .row("removed", path.display())
            .note(
                "A gproxy already running in this session is still running: the script was the \
                 boot hook, not the process. Stop it with `pkill -f 'gproxy serve'`.",
            ))
    } else {
        Ok(Report::new("There was no gproxy boot script to remove.")
            .row("looked in", path.display()))
    }
}

pub fn status() -> Result<Report> {
    let termux = detect()
        .ok_or_else(|| crate::Error::other("gproxy service: this is not a Termux environment"))?;
    let path = termux.boot_script();
    let installed = path.is_file();
    // Termux:Boot skips a file it cannot execute, silently, so a script with
    // the wrong mode is a service that will never start and nothing will say
    // why.
    let executable = match (installed, is_executable(&path)) {
        (false, _) => None,
        (true, true) => Some("yes"),
        (true, false) => Some("NO — Termux:Boot only runs executable files"),
    };

    let mut report = Report::new(if installed {
        "gproxy has a Termux:Boot script."
    } else {
        "gproxy has no Termux:Boot script."
    })
    .row("script", path.display())
    .row("installed", installed)
    .maybe("executable", executable);

    if installed {
        report = report.note(
            "That is all this can report. Termux has no service manager to ask, so there is no \
             `active`, no `enabled` and no last exit code — only whether the file the add-on \
             would run is there. Whether Termux:Boot itself is installed cannot be seen from \
             inside Termux either.",
        );
    }
    Ok(report)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o100 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::tests::plan;

    fn termux() -> Termux {
        Termux {
            home: PathBuf::from("/data/data/com.termux/files/home"),
            prefix: PathBuf::from("/data/data/com.termux/files/usr"),
        }
    }

    /// The detection the brief insists on: `PREFIX`, not a guess from `uname`.
    #[test]
    fn termux_is_detected_by_its_prefix_and_nothing_else() {
        assert_eq!(
            from_environment(
                Some(PathBuf::from("/data/data/com.termux/files/usr")),
                Some(PathBuf::from("/data/data/com.termux/files/home")),
            ),
            Some(termux())
        );
        // An ordinary Linux box, which `uname` would not distinguish.
        assert_eq!(
            from_environment(Some(PathBuf::from("/usr")), Some(PathBuf::from("/home/x"))),
            None
        );
        // No PREFIX at all.
        assert_eq!(from_environment(None, Some(PathBuf::from("/home/x"))), None);
        // A PREFIX that is Termux but no HOME: there is nowhere to put the
        // script, so this is not a Termux we can install into.
        assert_eq!(
            from_environment(Some(PathBuf::from("/data/data/com.termux/files/usr")), None),
            None
        );
    }

    /// A Termux fork — Termux:Float, a repackaged build — keeps the package
    /// name in its prefix, so the substring test is the right one.
    #[test]
    fn a_repackaged_termux_still_counts() {
        assert!(
            from_environment(
                Some(PathBuf::from("/data/data/com.termux.float/files/usr")),
                Some(PathBuf::from("/data/data/com.termux.float/files/home")),
            )
            .is_some()
        );
    }

    #[test]
    fn the_script_lives_where_the_add_on_looks_and_names_termuxs_own_shell() {
        assert_eq!(
            termux().boot_script(),
            PathBuf::from("/data/data/com.termux/files/home/.termux/boot/gproxy.sh")
        );
        // There is no /bin/sh on Android.
        assert_eq!(
            termux().shell(),
            PathBuf::from("/data/data/com.termux/files/usr/bin/sh")
        );
    }

    /// A script without a shebang is a script Termux:Boot hands to whatever
    /// the kernel guesses, which on Android is nothing.
    #[test]
    fn the_script_starts_with_a_shebang_naming_termuxs_shell() {
        let script = script(
            &plan(),
            &termux().shell(),
            Path::new("/data/data/com.termux/files/home/service.log"),
        );
        assert!(
            script.starts_with("#!/data/data/com.termux/files/usr/bin/sh\n"),
            "{script}"
        );
        // And on the first line, which is the only place a shebang works.
        assert_eq!(script.lines().next().unwrap().find("#!"), Some(0));
    }

    #[test]
    fn the_script_does_everything_a_supervisor_would_have_done() {
        let log = Path::new("/data/data/com.termux/files/home/service.log");
        let script = script(&plan(), &termux().shell(), log);
        // A wake lock, or Android suspends it.
        assert!(script.contains("termux-wake-lock"), "{script}");
        // The working directory, quoted, because it has a space in it.
        assert!(
            script.contains("\ncd '/srv/gproxy data' || exit 1\n"),
            "{script}"
        );
        // The NDK runtime beside the executable.
        assert!(
            script.contains(
                "\nLD_LIBRARY_PATH='/opt/GPROXY bin'\"${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}\"\n"
            ),
            "{script}"
        );
        // Detached, so the boot job can finish.
        assert!(script.contains("setsid"), "{script}");
        // Redirected and backgrounded, in that order.
        assert!(
            script.ends_with("\"$@\" >> '/data/data/com.termux/files/home/service.log' 2>&1 &\n"),
            "{script}"
        );
    }

    #[test]
    fn every_argument_is_single_quoted_so_a_space_or_a_quote_survives() {
        let script = script(&plan(), &termux().shell(), Path::new("run.log"));
        assert!(
            script.contains(
                "\nset -- '/opt/GPROXY bin/gproxy' 'serve' '--host' '127.0.0.1' '--port' '9000' \
                 '--data-dir' '/srv/gproxy data/db'\n"
            ),
            "{script}"
        );
        // The one form that is safe for every byte a path can hold.
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn a_bare_executable_name_carries_no_library_path() {
        let mut plan = plan();
        plan.executable = PathBuf::from("gproxy");
        let script = script(&plan, &termux().shell(), Path::new("run.log"));
        assert!(!script.contains("LD_LIBRARY_PATH"), "{script}");
    }

    #[test]
    fn no_secret_reaches_the_script_only_the_path_of_the_file_holding_one() {
        let script = script(&plan(), &termux().shell(), Path::new("run.log"));
        assert!(!script.contains("GPROXY_MASTER_KEY"), "{script}");
        assert!(
            script.contains("GPROXY_ENV_FILE='/srv/gproxy data/.env'\nexport GPROXY_ENV_FILE\n"),
            "{script}"
        );
    }
}
