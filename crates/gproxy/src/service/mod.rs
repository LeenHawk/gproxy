//! `gproxy service` — handing the instance to the machine's own init system.
//!
//! # Why there is no `--daemon`
//!
//! Backgrounding, restart-on-crash and start-at-boot are three problems every
//! operating system already solved, and solved better than a `fork()` and a
//! pidfile would. A built-in daemon mode means owning a pidfile that goes
//! stale, a log file nobody rotates, a restart loop with no backoff, and a
//! `gproxy stop` that races whatever else is holding the port. So `gproxy`
//! stays a foreground process that dies on `SIGTERM`, and this module writes
//! the unit that a supervisor built for the job reads.
//!
//! What that means in practice: `serve` never learns it is being supervised,
//! `service install` never learns how to supervise, and the seam between them
//! is a file on disk that an operator can read, edit and delete.
//!
//! # Why the four platforms do not share an implementation
//!
//! A systemd unit, a launchd plist, a Task Scheduler XML task and a Termux
//! boot script are not four spellings of one idea:
//!
//! - systemd supervises; launchd supervises; Task Scheduler *starts* and only
//!   restarts on failure if asked; Termux:Boot runs a script once and forgets
//!   it, so the script has to detach the process itself.
//! - systemd reads an `EnvironmentFile=`; launchd and Task Scheduler have no
//!   such thing, so the environment file reaches those two as the
//!   `GPROXY_ENV_FILE` path gproxy itself reads.
//! - systemd and launchd capture output (a journal, a file); Task Scheduler
//!   captures nothing unless the action is a shell that redirects.
//! - "start at boot" means linger on systemd, is impossible for a per-user
//!   launchd agent, needs SYSTEM rights on Windows, and is the only thing
//!   Termux:Boot does at all.
//!
//! A trait over those four would have one method per platform quirk and every
//! implementation returning "not applicable" for three quarters of it. So
//! there is no trait: [`Plan`] describes the invocation to reproduce, each
//! platform module turns it into that platform's artifact in that platform's
//! idiom, and the three entry points here are a `match` over four arms.
//!
//! # What never goes into a unit file
//!
//! A secret. `~/.config/systemd/user/gproxy.service` is a plain file living a
//! directory away from the database it would unlock, it survives every backup
//! of `$HOME`, and `systemctl --user show` prints `Environment=` back to
//! anyone who asks. So the unit carries the *path* of the environment file and
//! never a value from it — and when the master key reached this process some
//! other way, [`install`] says so rather than quietly writing a service that
//! will come up storing secrets in plaintext.

use std::path::{Path, PathBuf};

use crate::{
    Result, Settings,
    cli::{self, ServiceAction},
    config,
};

pub mod launchd;
pub mod schtasks;
pub mod systemd;
pub mod termux;

mod report;
pub use report::Report;

/// A well-formedness check for the plist and the task XML. Test-only: the two
/// generators are deterministic, so the place to catch a malformed document is
/// the suite, not every install.
#[cfg(test)]
mod xml;

/// The service name on systemd, Task Scheduler and Termux.
pub const NAME: &str = "gproxy";

/// The reverse-DNS label launchd wants, and v3's. Renaming it would orphan an
/// agent a v3 machine already has bootstrapped.
pub const LABEL: &str = "io.github.leenhawk.gproxy";

/// Run one `gproxy service …` invocation.
///
/// `config_path` is `--config` as the operator wrote it. It is passed rather
/// than read off [`Settings`], which holds the *result* of the layering and
/// has deliberately forgotten which file produced it — but the unit has to
/// name that file, or the service starts from the defaults instead.
pub fn run(action: &ServiceAction, settings: &Settings, config_path: Option<&Path>) -> Result<()> {
    let report = match action {
        ServiceAction::Install { autostart } => {
            let autostart = match autostart.as_deref() {
                Some(value) => config::boolean(value, cli::AUTOSTART)?,
                None => false,
            };
            install(&Plan::resolve(settings, config_path, autostart)?)
        }
        ServiceAction::Uninstall => uninstall(),
        ServiceAction::Status => status(),
    }?;
    report.print();
    Ok(())
}

// ------------------------------------------------------------------ plan --

/// The invocation a unit file has to reproduce.
///
/// Every path here is absolute. A unit runs with a working directory the init
/// system chose — `$HOME` for a systemd user service, `/` for a launchd agent
/// — so a relative `data` that meant "the directory I ran `gproxy` in" would
/// quietly become a different database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// This binary, as an absolute path.
    pub executable: PathBuf,
    /// Where the service runs. The directory the install was run from, which
    /// is what the relative paths in a `--config` file resolve against.
    pub working_dir: PathBuf,
    /// `--data-dir`, made absolute.
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    /// `--config`, made absolute, when one was named.
    pub config: Option<PathBuf>,
    /// The environment file the service is pointed at. Absolute, and never
    /// read by this module — only named.
    pub env_file: PathBuf,
    /// Where `GPROXY_MASTER_KEY` comes from, which decides what the command
    /// has to tell the operator.
    pub master_key: KeySource,
    /// Whether the operator asked for start-at-boot as well as start-at-login.
    pub autostart: bool,
}

/// How the master key reaches the service — never *what* it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// No key at all. The instance stores secrets in plaintext, and always
    /// did; installing a service does not change that, but it is worth saying
    /// while the operator is looking.
    Absent,
    /// [`Plan::env_file`] defines `GPROXY_MASTER_KEY`. The unit names the
    /// file; the value stays in it.
    EnvFile,
    /// A key is configured but no file behind it — an exported variable in the
    /// shell that ran the install, or a `--master-key` on the command line.
    /// The unit cannot carry it and will not copy it.
    Ambient,
}

impl Plan {
    /// Resolve the current invocation into something reproducible.
    pub fn resolve(
        settings: &Settings,
        config_path: Option<&Path>,
        autostart: bool,
    ) -> Result<Self> {
        let working_dir = std::env::current_dir()
            .map_err(|error| crate::Error::io("reading the working directory", error))?;
        let executable = std::env::current_exe()
            .map_err(|error| crate::Error::io("locating this executable", error))?;
        let data_dir = absolute(
            settings
                .config
                .data_dir
                .as_deref()
                .unwrap_or(config::DEFAULT_DATA_DIR),
            &working_dir,
        );
        let env_file = absolute(crate::env::path(), &working_dir);
        Ok(Self {
            executable,
            data_dir,
            host: settings.config.host.clone(),
            port: settings.config.port,
            config: config_path.map(|path| absolute(path, &working_dir)),
            master_key: key_source(settings, &env_file),
            env_file,
            working_dir,
            autostart,
        })
    }

    /// The arguments after the executable: a `serve` that reproduces this
    /// invocation.
    ///
    /// Deliberately short. `--host`, `--port` and `--data-dir` say where the
    /// instance is; everything else — a DSN with a password in it, a Redis
    /// URL, an admin password — belongs in the environment file or the config
    /// file, because those are files an operator can give a mode to and a unit
    /// is not.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["serve".to_owned()];
        if let Some(config) = &self.config {
            args.push("--config".to_owned());
            args.push(config.to_string_lossy().into_owned());
        }
        args.push("--host".to_owned());
        args.push(self.host.clone());
        args.push("--port".to_owned());
        args.push(self.port.to_string());
        args.push("--data-dir".to_owned());
        args.push(self.data_dir.to_string_lossy().into_owned());
        args
    }

    /// The executable and its arguments, which is what every one of the four
    /// artifacts is ultimately a way of spelling.
    pub fn command(&self) -> Vec<String> {
        std::iter::once(self.executable.to_string_lossy().into_owned())
            .chain(self.args())
            .collect()
    }

    /// Where this platform sends the service's output. systemd has a journal
    /// and needs none; the other three do.
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join("service.log")
    }

    /// What the operator has to be told about the key, in one sentence.
    pub fn key_note(&self) -> String {
        match self.master_key {
            KeySource::Absent => format!(
                "no master key is configured, so the service will store upstream credential \
                 secrets UNENCRYPTED. Put {key}=<32 bytes as 64 hex characters or base64> in \
                 {file} (mode 600) and reinstall.",
                key = cli::MASTER_KEY,
                file = self.env_file.display(),
            ),
            KeySource::EnvFile => format!(
                "the master key stays in {file}, which the service reads for itself. It is not \
                 written into the unit.",
                file = self.env_file.display(),
            ),
            KeySource::Ambient => format!(
                "{key} reached this command from the environment or the command line, and a key \
                 in a unit file is a key in plaintext next to the database it protects — so it \
                 was NOT written into the unit. Put {key}=… in {file} (mode 600), which the \
                 service reads, or it will come up storing secrets unencrypted.",
                key = cli::MASTER_KEY,
                file = self.env_file.display(),
            ),
        }
    }
}

/// Whether a key was configured, and whether the environment file is what
/// supplied it.
///
/// The test is deliberately textual: the file is opened, and it counts only if
/// a line in it assigns `GPROXY_MASTER_KEY`. Comparing values would mean
/// holding the secret to compare, and a `.env` that sets the key but loses to
/// a real environment variable is still a file the service can read a key
/// from.
fn key_source(settings: &Settings, env_file: &Path) -> KeySource {
    use gproxy_app::config::MasterKey;
    if matches!(settings.config.master_key.key, MasterKey::None) {
        return KeySource::Absent;
    }
    match std::fs::read_to_string(env_file) {
        Ok(text) if assigns(&text, cli::MASTER_KEY) => KeySource::EnvFile,
        _ => KeySource::Ambient,
    }
}

/// Whether a `.env` body assigns `name`. `export NAME=` counts; a comment does
/// not.
fn assigns(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        line.strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

fn absolute(path: impl AsRef<Path>, base: &Path) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

// -------------------------------------------------------------- platform --

/// Which supervisor this machine has.
///
/// Resolved at run time rather than by `#[cfg]`, for two reasons. The honest
/// one: `#[cfg(target_os)]` cannot tell Termux from any other Linux userland,
/// and a Termux build *is* an Android build, so the question has to be asked
/// of the process anyway. The useful one: every generator below then compiles
/// on every host, so the plist is parsed by a test on Linux and the unit file
/// is parsed by a test on a Mac.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    Systemd,
    Launchd,
    TaskScheduler,
    Termux,
    /// No supervisor this command knows how to drive. The string is what the
    /// operator is told, and it is the reason rather than the name.
    Unsupported(String),
}

pub fn detect() -> Platform {
    // Termux first and by its own evidence. `uname` on a Termux device says
    // `Linux`, `std::env::consts::OS` says `android`, and neither
    // distinguishes Termux from an Android app that happens to bundle this
    // binary. `PREFIX` pointing inside `com.termux` does: it is set by the
    // Termux shell itself and names the package whose data directory the whole
    // userland lives in.
    if termux::detect().is_some() {
        return Platform::Termux;
    }
    match std::env::consts::OS {
        "linux" if cfg!(target_env = "ohos") => Platform::Unsupported(
            "OpenHarmony service registration is not supported; run gproxy in the foreground."
                .to_owned(),
        ),
        "linux" => systemd::detect(),
        "macos" => Platform::Launchd,
        "windows" => Platform::TaskScheduler,
        // Android without Termux: the binary is inside some app's sandbox,
        // where there is no init system to talk to and no `~/.termux/boot` for
        // Termux:Boot to read.
        "android" => Platform::Unsupported(
            "this is Android but not Termux, so there is no boot hook to install into. Run \
             gproxy from Termux with the Termux:Boot add-on, or keep it in the foreground."
                .to_owned(),
        ),
        other => Platform::Unsupported(format!(
            "gproxy has no service integration for {other}; run `gproxy serve` under whatever \
             supervisor this system provides."
        )),
    }
}

// ----------------------------------------------------------- the actions --

pub fn install(plan: &Plan) -> Result<Report> {
    match detect() {
        Platform::Systemd => systemd::install(plan),
        Platform::Launchd => launchd::install(plan),
        Platform::TaskScheduler => schtasks::install(plan),
        Platform::Termux => termux::install(plan),
        Platform::Unsupported(why) => Err(unsupported(&why)),
    }
}

pub fn uninstall() -> Result<Report> {
    match detect() {
        Platform::Systemd => systemd::uninstall(),
        Platform::Launchd => launchd::uninstall(),
        Platform::TaskScheduler => schtasks::uninstall(),
        Platform::Termux => termux::uninstall(),
        Platform::Unsupported(why) => Err(unsupported(&why)),
    }
}

pub fn status() -> Result<Report> {
    match detect() {
        Platform::Systemd => systemd::status(),
        Platform::Launchd => launchd::status(),
        Platform::TaskScheduler => schtasks::status(),
        Platform::Termux => termux::status(),
        Platform::Unsupported(why) => Err(unsupported(&why)),
    }
}

/// A machine that cannot host a service is told so, plainly, once — not handed
/// a unit file that nothing will ever read.
fn unsupported(why: &str) -> crate::Error {
    crate::Error::other(format!("gproxy service: {why}"))
}

// ------------------------------------------------------------- utilities --

/// `$HOME`, which every one of the four platforms needs and none of them can
/// do without.
pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            crate::Error::other(
                "gproxy service: neither HOME nor USERPROFILE is set, so there is no per-user \
                 location to install into",
            )
        })
}

/// Run a tool and collect what it said, without letting it inherit this
/// process's terminal.
///
/// Every platform below drives its supervisor through the supervisor's own
/// command line rather than its API. That is not laziness: `systemctl`,
/// `launchctl` and `schtasks` are the interfaces those systems document,
/// version and keep compatible, and a failure from one of them is a message an
/// operator can paste into a search engine — which a `libsystemd` error code
/// is not.
pub fn run_tool(program: &str, args: &[&str]) -> Result<ToolOutput> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                crate::Error::other(format!(
                    "gproxy service: `{program}` is not on PATH, so this machine's service \
                     manager cannot be reached"
                ))
            } else {
                crate::Error::io(format!("running `{program}`"), error)
            }
        })?;
    Ok(ToolOutput {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// This user's numeric id, as `loginctl` and `launchctl` both want it.
///
/// From `id -u` rather than from a `getuid()` FFI declaration: one number is
/// not worth an `unsafe extern` block in the crate that is meant to be thin,
/// and `id` is POSIX and present wherever either of those tools is.
///
/// The uid rather than a name, and never an omitted argument.
/// `loginctl show-user` with nothing after it reports the *login manager's*
/// properties, not the calling user's — it answers, it exits 0, and it says
/// nothing about linger at all. `$USER` is not set in every context a service
/// command runs in either.
pub fn uid() -> Result<String> {
    let id = run_tool("id", &["-u"])?;
    id.require("id -u")?;
    let uid = id.stdout.trim().to_owned();
    if uid.is_empty() || !uid.chars().all(|c| c.is_ascii_digit()) {
        return Err(crate::Error::other(format!(
            "gproxy service: `id -u` answered `{uid}`, which is not a uid"
        )));
    }
    Ok(uid)
}
impl ToolOutput {
    /// The output as one message, for an error. `stderr` first because that is
    /// where all three tools put their complaints.
    pub fn message(&self) -> String {
        let text = format!("{}\n{}", self.stderr.trim(), self.stdout.trim());
        let text = text.trim();
        if text.is_empty() {
            "no output".to_owned()
        } else {
            text.to_owned()
        }
    }

    /// Fail with what the tool said, naming the command that said it.
    pub fn require(&self, what: &str) -> Result<()> {
        if self.ok {
            Ok(())
        } else {
            Err(crate::Error::other(format!(
                "gproxy service: {what} failed: {}",
                self.message()
            )))
        }
    }
}

/// Write a file only this user can read, replacing whatever was there.
///
/// The mode matters even though nothing secret is written: a unit file is what
/// decides which binary runs as this user at every boot, and a world-writable
/// one is a way to make it run something else.
pub fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| crate::Error::io(format!("creating {}", parent.display()), error))?;
    }
    std::fs::write(path, contents)
        .map_err(|error| crate::Error::io(format!("writing {}", path.display()), error))?;
    set_mode(path, 0o600)
}

/// Same, for something that has to be executable.
pub fn write_private_script(path: &Path, contents: &str) -> Result<()> {
    write_private(path, contents.as_bytes())?;
    set_mode(path, 0o700)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| crate::Error::io(format!("setting the mode of {}", path.display()), error))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    // Windows has no mode bits to set. The task XML lands under the user's own
    // profile, which is where its access control already comes from.
    Ok(())
}

/// Delete a file, reporting whether there was one.
pub fn remove(path: &Path) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::Error::io(
            format!("removing {}", path.display()),
            error,
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use gproxy_app::config::{AppConfig, MasterKey, MasterKeyConfig};

    /// A plan with awkward-but-legal paths in it, so every generator is tested
    /// against the quoting it will actually meet.
    pub(crate) fn plan() -> Plan {
        Plan {
            executable: PathBuf::from("/opt/GPROXY bin/gproxy"),
            working_dir: PathBuf::from("/srv/gproxy data"),
            data_dir: PathBuf::from("/srv/gproxy data/db"),
            host: "127.0.0.1".into(),
            port: 9000,
            config: None,
            env_file: PathBuf::from("/srv/gproxy data/.env"),
            master_key: KeySource::EnvFile,
            autostart: true,
        }
    }

    fn settings(data_dir: Option<&str>, key: MasterKey) -> Settings {
        Settings {
            config: AppConfig {
                host: "0.0.0.0".into(),
                port: 8080,
                data_dir: data_dir.map(str::to_owned),
                master_key: MasterKeyConfig {
                    key,
                    ..MasterKeyConfig::default()
                },
                ..AppConfig::default()
            },
            admin: Default::default(),
            telemetry: Default::default(),
            instance_id: None,
        }
    }

    #[test]
    fn the_command_is_a_serve_that_reproduces_the_invocation() {
        let plan = plan();
        assert_eq!(
            plan.command(),
            vec![
                "/opt/GPROXY bin/gproxy",
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                "9000",
                "--data-dir",
                "/srv/gproxy data/db",
            ]
        );
    }

    #[test]
    fn a_named_config_file_travels_into_the_unit() {
        let mut plan = plan();
        plan.config = Some(PathBuf::from("/etc/gproxy.toml"));
        let command = plan.command();
        assert!(
            command
                .windows(2)
                .any(|pair| pair == ["--config".to_owned(), "/etc/gproxy.toml".to_owned()]),
            "{command:?}"
        );
    }

    #[test]
    fn a_relative_data_directory_is_made_absolute_against_the_install_directory() {
        // A unit runs with a working directory the init system chose, so a
        // relative `data` would silently become a different database.
        let cwd = std::env::current_dir().unwrap();
        let plan = Plan::resolve(&settings(Some("data"), MasterKey::None), None, false).unwrap();
        assert_eq!(plan.data_dir, cwd.join("data"));
        assert!(plan.env_file.is_absolute());
        assert!(plan.working_dir.is_absolute());
    }

    #[test]
    fn a_relative_config_file_is_made_absolute_too() {
        let cwd = std::env::current_dir().unwrap();
        let plan = Plan::resolve(
            &settings(Some("/srv/gproxy"), MasterKey::None),
            Some(Path::new("gproxy.toml")),
            false,
        )
        .unwrap();
        assert_eq!(plan.config, Some(cwd.join("gproxy.toml")));
    }

    #[test]
    fn no_master_key_is_reported_as_plaintext_rather_than_silently_accepted() {
        let plan =
            Plan::resolve(&settings(Some("/srv/gproxy"), MasterKey::None), None, false).unwrap();
        assert_eq!(plan.master_key, KeySource::Absent);
        assert!(
            plan.key_note().contains("UNENCRYPTED"),
            "{}",
            plan.key_note()
        );
    }

    /// The rule the whole module exists to keep: a key that came from the
    /// environment is reported, not copied.
    #[test]
    fn an_ambient_key_is_refused_a_place_in_the_unit() {
        let plan = Plan::resolve(
            &settings(Some("/srv/gproxy"), MasterKey::Hex("11".repeat(32))),
            None,
            false,
        )
        .unwrap();
        assert_eq!(plan.master_key, KeySource::Ambient);
        let note = plan.key_note();
        assert!(note.contains("NOT written into the unit"), "{note}");
        // And it names the file that would fix it.
        assert!(note.contains(".env"), "{note}");
    }

    #[test]
    fn an_env_file_that_assigns_the_key_is_what_the_unit_points_at() {
        let dir = tempfile::tempdir().unwrap();
        let env_file = dir.path().join(".env");
        std::fs::write(&env_file, "# a comment\nexport GPROXY_MASTER_KEY=abc\n").unwrap();
        let source = key_source(
            &settings(Some("/srv/gproxy"), MasterKey::Hex("11".repeat(32))),
            &env_file,
        );
        assert_eq!(source, KeySource::EnvFile);
    }

    #[test]
    fn a_commented_out_key_does_not_count_as_a_source() {
        for body in [
            "# GPROXY_MASTER_KEY=abc\n",
            "GPROXY_MASTER_KEY_NEXT=abc\n",
            "GPROXY_PORT=7070\n",
            "",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let env_file = dir.path().join(".env");
            std::fs::write(&env_file, body).unwrap();
            assert_eq!(
                key_source(
                    &settings(Some("/srv/gproxy"), MasterKey::Hex("11".repeat(32))),
                    &env_file,
                ),
                KeySource::Ambient,
                "{body:?}"
            );
        }
    }

    #[test]
    fn an_unsupported_platform_is_a_refusal_rather_than_a_broken_unit() {
        let error = unsupported("there is no init system here");
        assert!(error.to_string().starts_with("gproxy service:"));
    }

    #[test]
    fn a_missing_tool_names_itself_rather_than_the_syscall() {
        let error = run_tool("gproxy-no-such-service-manager", &["--version"]).unwrap_err();
        assert!(error.to_string().contains("is not on PATH"), "{error}");
    }

    /// `loginctl show-user` and `launchctl bootstrap` both want the uid, and
    /// neither accepts being told nothing.
    #[cfg(unix)]
    #[test]
    fn the_uid_is_a_number_this_machine_agrees_with() {
        let uid = uid().unwrap();
        assert!(!uid.is_empty());
        assert!(uid.chars().all(|c| c.is_ascii_digit()), "{uid}");
    }

    #[cfg(unix)]
    #[test]
    fn what_is_written_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let unit = dir.path().join("nested/gproxy.service");
        write_private(&unit, b"[Unit]\n").unwrap();
        assert_eq!(
            std::fs::metadata(&unit).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let script = dir.path().join("gproxy.sh");
        write_private_script(&script, "#!/bin/sh\n").unwrap();
        assert_eq!(
            std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
            0o700
        );

        assert!(remove(&unit).unwrap());
        // And removing what is not there is not a failure: `uninstall` has to
        // be runnable twice.
        assert!(!remove(&unit).unwrap());
    }
}
