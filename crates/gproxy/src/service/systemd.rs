//! Linux, as a systemd **user** unit.
//!
//! # Why a user unit and not a system one
//!
//! A system unit in `/etc/systemd/system` needs root to write, root to
//! enable, a `User=` line, a `StateDirectory=`, and an answer to "who owns
//! `/var/lib/gproxy`". A user unit needs none of that: it runs as the person
//! who installed it, reads the database that person already owns, and is
//! removable by the same person. That is the deployment gproxy is for — one
//! gateway, one operator, one `$HOME`.
//!
//! The cost is that a user manager normally exists only while that user has a
//! session, which is what `loginctl enable-linger` fixes and what
//! `--autostart` asks for.
//!
//! # Why `systemd-analyze verify` is not run
//!
//! It would be a second opinion on a file this module generates, and a
//! disagreeing one is a bug here rather than something an operator can act
//! on. The real check is `systemctl --user enable`, which parses the unit and
//! refuses it, and whose complaint is passed straight through.
//!
//! # The quoting, which is not uniform
//!
//! `ExecStart=` is split into words and *does* honour double quotes, so every
//! argument is quoted. `WorkingDirectory=` and `EnvironmentFile=` take the
//! rest of the line **raw** — a quoted path there is rejected outright
//! (`WorkingDirectory= path is not absolute: "/srv/a dir"`), and an unquoted
//! one with a space in it is accepted exactly as written. So those two are
//! written bare. Both were checked against systemd 257 rather than reasoned
//! about; see the tests.

use std::path::{Path, PathBuf};

use super::{Plan, Platform, Report, Result, home, remove, run_tool, uid, write_private};

/// The unit's file name, and the name every `systemctl --user` verb takes.
///
/// `super::NAME` with `.service` appended, spelled out because a `const`
/// cannot call `format!`.
pub const UNIT: &str = "gproxy.service";

/// A comment the unit carries so that `uninstall` knows whether *this* command
/// turned linger on.
///
/// The alternative — a marker file somewhere in the data directory — is state
/// about the unit kept away from the unit, which goes stale the first time
/// someone deletes one without the other. Keeping it in the artifact we own
/// means removing the artifact removes the record.
const LINGER_MARKER: &str = "# X-GProxy-Linger=enabled-by-install";

/// systemd, or a clear explanation of its absence.
///
/// A container, a chroot, a minimal distribution with runit or s6: all of them
/// are Linux and none of them has a user manager to talk to. The check is for
/// the manager's own socket rather than for `systemctl` on `PATH`, because a
/// Debian container has the binary installed and nothing listening.
pub fn detect() -> Platform {
    if !Path::new("/run/systemd/system").exists() {
        return Platform::Unsupported(
            "this machine is Linux but is not running systemd — /run/systemd/system does not \
             exist, which is what a container, a chroot or a distribution built on runit, s6 or \
             OpenRC looks like. There is no user unit to install. Run `gproxy serve` as your \
             supervisor's own service: it stays in the foreground, logs to stderr and exits on \
             SIGTERM, which is what every process supervisor wants."
                .to_owned(),
        );
    }
    Platform::Systemd
}

pub fn unit_path() -> Result<PathBuf> {
    let root = match std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(config) => PathBuf::from(config),
        None => home()?.join(".config"),
    };
    Ok(root.join("systemd/user").join(UNIT))
}

// ------------------------------------------------------------- the unit --

/// The unit file for `plan`.
///
/// Pure, and public, so a test on any machine can assert on what a Linux box
/// would be handed.
pub fn unit(plan: &Plan) -> String {
    let mut out = String::new();
    out.push_str("# Written by `gproxy service install`. Reinstall to change it; this file is\n");
    out.push_str("# overwritten, not merged.\n");
    if plan.autostart {
        out.push_str(LINGER_MARKER);
        out.push('\n');
    }
    out.push_str(
        "\n[Unit]\n\
         Description=GPROXY — one gateway in front of many LLM providers\n\
         Documentation=https://github.com/LeenHawk/gproxy\n",
    );
    // No `After=network-online.target`. A user manager has its own, much
    // smaller set of targets and `network-online.target` is not one of them —
    // ordering against it would name a unit that does not exist, and `Wants=`
    // on it would fail to start. The default bind is loopback, which is up
    // before any user unit can run; an operator who binds one interface's
    // address gets `Restart=on-failure` and `RestartSec=` instead, which is
    // what actually handles an address that is not there yet.
    out.push_str(
        "\n[Service]\n\
         # `exec` rather than `simple`: the start is not reported as successful\n\
         # until execve() has, so a moved or unbuilt binary fails the start\n\
         # instead of failing silently a moment later.\n\
         Type=exec\n",
    );
    out.push_str(&format!(
        "WorkingDirectory={}\n",
        setting(&plan.working_dir)
    ));
    // `-` so a missing file is not a failed start. The file is where the
    // master key and anything else secret lives; the unit names it and never
    // reads it.
    out.push_str(&format!("EnvironmentFile=-{}\n", setting(&plan.env_file)));
    out.push_str(&format!(
        "ExecStart={}\n",
        plan.command()
            .iter()
            .map(|arg| argument(arg))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    out.push_str(
        "Restart=on-failure\n\
         RestartSec=5\n\
         # `serve` deliberately imposes no shutdown deadline of its own — a\n\
         # streamed completion legitimately runs for minutes — so the deadline\n\
         # is here, where a supervisor can be told to change it.\n\
         TimeoutStopSec=120\n\
         KillSignal=SIGTERM\n\
         # Output goes to the journal: `journalctl --user -u gproxy -f`.\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         SyslogIdentifier=gproxy\n",
    );
    out.push_str(
        "\n[Install]\n\
         # The user manager's own default target, which is what\n\
         # `systemctl --user enable` needs something to be wanted by.\n\
         WantedBy=default.target\n",
    );
    out
}

/// One `ExecStart=` argument. Always quoted, because systemd splits the line
/// on whitespace and then unquotes.
///
/// Three escapes, and each one is a real hazard rather than caution:
/// `%` starts a specifier (`%h` is the home directory), `$` starts a variable
/// expansion, and `\` is systemd's own escape character inside a quoted word.
fn argument(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

/// A `WorkingDirectory=`/`EnvironmentFile=` value: raw, because those settings
/// take the rest of the line and reject quotes.
///
/// `%` still has to be escaped — specifier expansion happens before any
/// setting is parsed. `$` does not: variable expansion is a command-line
/// feature, and a `$` in a path is a `$` here.
fn setting(path: &Path) -> String {
    path.to_string_lossy().replace('%', "%%")
}

// ---------------------------------------------------------------- verbs --

pub fn install(plan: &Plan) -> Result<Report> {
    let path = unit_path()?;
    // The data directory before the unit: a service whose `--data-dir` does
    // not exist starts, creates it as root-owned-by-nobody or fails on the
    // first write, and either way the failure arrives after the install
    // reported success.
    std::fs::create_dir_all(&plan.data_dir).map_err(|error| {
        crate::Error::io(format!("creating {}", plan.data_dir.display()), error)
    })?;
    write_private(&path, unit(plan).as_bytes())?;

    run_tool("systemctl", &["--user", "daemon-reload"])?
        .require("systemctl --user daemon-reload")?;
    // `enable --now` in one step: enabling without starting leaves an operator
    // looking at a service that will exist after the next login and does not
    // now, which is the single most confusing state this command could produce.
    run_tool("systemctl", &["--user", "enable", "--now", UNIT])?
        .require(&format!("systemctl --user enable --now {UNIT}"))?;

    let mut report = Report::new("gproxy is now a systemd user service.")
        .row("unit", path.display())
        .row("command", plan.command().join(" "))
        .row("data", plan.data_dir.display())
        .row("listen", format!("{}:{}", plan.host, plan.port))
        .row("logs", "journalctl --user -u gproxy -f");

    if plan.autostart {
        // The uid, spelled out. `loginctl enable-linger` with no argument does
        // imply the calling user, but the matching `show-user` does not —
        // naming the user in all three places is what keeps `install`,
        // `uninstall` and `status` talking about the same thing.
        let uid = uid()?;
        let linger = run_tool("loginctl", &["enable-linger", &uid])?;
        if linger.ok {
            report = report.row("linger", "enabled, so it starts at boot");
        } else {
            // Not fatal. The service is installed and running; only the
            // survive-a-logout part failed, and the operator can do it by hand.
            report = report.note(format!(
                "`loginctl enable-linger {uid}` failed: {}. The service is installed and running, \
                 but it will stop when you log out and will not come back until you log in \
                 again. Run `loginctl enable-linger {uid}` as root to fix that.",
                linger.message()
            ));
        }
    } else {
        report = report.note(
            "This service starts when you log in and stops when your last session ends. Pass \
             --autostart to run `loginctl enable-linger` as well, which is what makes it come up \
             at boot and stay up with nobody logged in.",
        );
    }

    Ok(report.note(plan.key_note()))
}

pub fn uninstall() -> Result<Report> {
    let path = unit_path()?;
    // Read before removing: the unit is where the record of who turned linger
    // on lives.
    let ours = std::fs::read_to_string(&path)
        .map(|text| text.lines().any(|line| line.trim() == LINGER_MARKER))
        .unwrap_or(false);

    // `disable --now` before the file goes away. The other order leaves the
    // symlink in `default.target.wants` pointing at nothing, which systemd
    // then complains about on every `daemon-reload`.
    let stopped = run_tool("systemctl", &["--user", "disable", "--now", UNIT])?;
    let existed = remove(&path)?;
    run_tool("systemctl", &["--user", "daemon-reload"])?
        .require("systemctl --user daemon-reload")?;
    // After the reload, so systemd forgets a unit it has already been told is
    // gone rather than keeping it in a `not-found` state. A failure here means
    // there was nothing to reset, which is the normal case.
    let _ = run_tool("systemctl", &["--user", "reset-failed", UNIT]);

    let mut report = if existed {
        Report::new("gproxy is no longer a systemd user service.").row("removed", path.display())
    } else {
        Report::new("There was no gproxy user unit to remove.").row("looked in", path.display())
    };
    if !stopped.ok && existed {
        report = report.note(format!(
            "`systemctl --user disable --now {UNIT}` complained: {}. The unit file is gone, so \
             the next `daemon-reload` settles it.",
            stopped.message()
        ));
    }

    // Only what this command turned on is turned back off. An operator who had
    // linger enabled for their own reasons before ever meeting gproxy keeps it.
    if ours {
        let uid = uid()?;
        let linger = run_tool("loginctl", &["disable-linger", &uid])?;
        report = if linger.ok {
            report.row(
                "linger",
                "disabled, as `install --autostart` had enabled it",
            )
        } else {
            report.note(format!(
                "`loginctl disable-linger {uid}` failed: {}. Linger is still on; `gproxy service \
                 install --autostart` is what turned it on, so it is safe to turn off by hand.",
                linger.message()
            ))
        };
    }
    Ok(report)
}

pub fn status() -> Result<Report> {
    let path = unit_path()?;
    // One `show` rather than `is-active`, `is-enabled` and `status`: three
    // calls can disagree with each other about a unit that changed state
    // between them, and `show` answers in a form that does not need a human to
    // read it.
    let properties = [
        "LoadState",
        "ActiveState",
        "SubState",
        "UnitFileState",
        "MainPID",
        "ExecMainStatus",
        "ExecMainCode",
        "Result",
        "NRestarts",
        "FragmentPath",
    ];
    let mut args = vec!["--user", "show", UNIT];
    let flags: Vec<String> = properties
        .iter()
        .map(|name| format!("--property={name}"))
        .collect();
    args.extend(flags.iter().map(String::as_str));
    let shown = run_tool("systemctl", &args)?;
    let fields = properties_of(&shown.stdout);
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };

    // A unit systemd has never heard of is not an error: `show` answers
    // `LoadState=not-found` and exits 0. Only a `show` that said nothing at
    // all is a failure worth passing on.
    if fields.is_empty() {
        shown.require(&format!("systemctl --user show {UNIT}"))?;
    }
    if field("LoadState").as_deref() == Some("not-found") && !path.exists() {
        return Ok(
            Report::new("gproxy is not installed as a systemd user service.")
                .row("looked in", path.display())
                .note("`gproxy service install` writes it."),
        );
    }

    let mut report = Report::new("gproxy, as the systemd user manager sees it.")
        .maybe(
            "unit",
            field("FragmentPath").or(Some(path.display().to_string())),
        )
        .maybe("loaded", field("LoadState"))
        .maybe("active", field("ActiveState"))
        .maybe("sub-state", field("SubState"))
        .maybe("enabled", field("UnitFileState"))
        // A pid of 0 means "no main process", which is what `active` already
        // said; printing it invites the reader to go looking for process 0.
        .maybe("pid", field("MainPID").filter(|pid| pid != "0"))
        .maybe("restarts", field("NRestarts").filter(|n| n != "0"))
        .maybe("result", field("Result"))
        // The last exit, spelled the way `systemctl status` spells it, because
        // `ExecMainCode=1 ExecMainStatus=0` on its own means nothing to anybody.
        .maybe("last exit", last_exit(&fields));

    let linger = uid()
        .ok()
        .and_then(|uid| run_tool("loginctl", &["show-user", &uid, "--property=Linger"]).ok())
        .filter(|output| output.ok)
        .and_then(|output| {
            properties_of(&output.stdout)
                .into_iter()
                .find(|(key, _)| key == "Linger")
                .map(|(_, value)| value)
        });
    report = report.maybe("linger", linger);
    Ok(report)
}

/// `KEY=VALUE` lines, as `systemctl show` and `loginctl show-user` both emit.
///
/// A value may itself contain `=`, so the split is on the *first* one only.
fn properties_of(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// How the main process last ended, from the pair of numbers systemd reports
/// it as.
fn last_exit(fields: &[(String, String)]) -> Option<String> {
    let get = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let status = get("ExecMainStatus")?;
    match get("ExecMainCode")? {
        // Never started, or still running and never exited.
        "0" => None,
        // CLD_EXITED
        "1" => Some(format!("exited with status {status}")),
        // CLD_KILLED, CLD_DUMPED
        "2" => Some(format!("killed by signal {status}")),
        "3" => Some(format!("dumped core on signal {status}")),
        other => Some(format!("code {other}, status {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::tests::plan;

    /// The sections and keys `systemctl --user enable` needs to exist at all.
    /// A unit without `[Install] WantedBy=` is enabled with "the unit files
    /// have no installation config", and that is exactly the kind of thing
    /// that is discovered on a customer's machine rather than here.
    #[test]
    fn the_unit_has_the_sections_and_keys_systemd_requires() {
        let unit = unit(&plan());
        for required in [
            "[Unit]",
            "Description=",
            "[Service]",
            "Type=exec",
            "ExecStart=",
            "WorkingDirectory=",
            "Restart=on-failure",
            "[Install]",
            "WantedBy=default.target",
        ] {
            assert!(unit.contains(required), "missing {required} in:\n{unit}");
        }
        // The three sections, in the order systemd's own documentation writes
        // them, and each exactly once.
        let unit_at = unit.find("[Unit]").unwrap();
        let service_at = unit.find("[Service]").unwrap();
        let install_at = unit.find("[Install]").unwrap();
        assert!(unit_at < service_at && service_at < install_at);
        assert_eq!(unit.matches("[Service]").count(), 1);
    }

    #[test]
    fn every_key_is_inside_a_section_and_no_line_is_stray() {
        let unit = unit(&plan());
        let mut section = None;
        for line in unit.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                assert!(line.ends_with(']'), "{line}");
                section = Some(line);
                continue;
            }
            assert!(section.is_some(), "`{line}` is before any section");
            assert!(line.contains('='), "`{line}` is not a key=value");
        }
    }

    /// The point of the module: the command line is reproduced, with the
    /// quoting systemd needs and none of the values it would expand.
    #[test]
    fn exec_start_quotes_every_argument_including_the_path_with_a_space() {
        let unit = unit(&plan());
        let line = unit
            .lines()
            .find(|line| line.starts_with("ExecStart="))
            .unwrap();
        assert_eq!(
            line,
            "ExecStart=\"/opt/GPROXY bin/gproxy\" \"serve\" \"--host\" \"127.0.0.1\" \"--port\" \
             \"9000\" \"--data-dir\" \"/srv/gproxy data/db\""
        );
    }

    /// `%h` in a path would become the home directory and `$HOME` would become
    /// a variable expansion — both silently, and both pointing the service at
    /// the wrong database.
    #[test]
    fn a_specifier_or_a_variable_in_a_path_is_escaped_rather_than_expanded() {
        assert_eq!(argument("/srv/100%/gproxy"), "\"/srv/100%%/gproxy\"");
        assert_eq!(argument("/srv/$HOME/gproxy"), "\"/srv/$$HOME/gproxy\"");
        assert_eq!(argument("/srv/a\"b"), "\"/srv/a\\\"b\"");
        assert_eq!(argument("/srv/a\\b"), "\"/srv/a\\\\b\"");
    }

    /// Checked against systemd 257, not reasoned about: a quoted
    /// `WorkingDirectory=` is refused with "path is not absolute", and an
    /// unquoted one with a space in it is accepted verbatim.
    #[test]
    fn the_raw_settings_are_unquoted_because_systemd_rejects_quotes_there() {
        let unit = unit(&plan());
        assert!(
            unit.contains("\nWorkingDirectory=/srv/gproxy data\n"),
            "{unit}"
        );
        assert!(
            unit.contains("\nEnvironmentFile=-/srv/gproxy data/.env\n"),
            "{unit}"
        );
        // A specifier still has to be escaped, because that expansion happens
        // for every setting.
        assert_eq!(setting(Path::new("/srv/100%/x")), "/srv/100%%/x");
    }

    /// The whole reason this module does not simply carry `argv` over.
    #[test]
    fn no_secret_reaches_the_unit() {
        let unit = unit(&plan());
        assert!(!unit.contains("GPROXY_MASTER_KEY="), "{unit}");
        assert!(!unit.contains("Environment="), "{unit}");
        // The file is named, and only named.
        assert!(unit.contains("EnvironmentFile=-"), "{unit}");
    }

    #[test]
    fn the_linger_marker_records_only_an_autostart_install() {
        assert!(unit(&plan()).contains(LINGER_MARKER));
        let mut quiet = plan();
        quiet.autostart = false;
        assert!(!unit(&quiet).contains(LINGER_MARKER));
        // And it is a comment, so systemd never sees it as configuration.
        assert!(LINGER_MARKER.starts_with('#'));
    }

    #[test]
    fn properties_split_on_the_first_equals_only() {
        let fields = properties_of("ActiveState=active\nExecStart={ path=/bin/x ; argv[]=a=b }\n");
        assert_eq!(fields[0], ("ActiveState".into(), "active".into()));
        assert_eq!(fields[1].0, "ExecStart");
        assert!(fields[1].1.contains("argv[]=a=b"));
    }

    #[test]
    fn the_last_exit_is_spelled_the_way_systemctl_status_spells_it() {
        let field = |code: &str, status: &str| {
            vec![
                ("ExecMainCode".to_owned(), code.to_owned()),
                ("ExecMainStatus".to_owned(), status.to_owned()),
            ]
        };
        assert_eq!(last_exit(&field("0", "0")), None);
        assert_eq!(
            last_exit(&field("1", "1")).as_deref(),
            Some("exited with status 1")
        );
        assert_eq!(
            last_exit(&field("2", "15")).as_deref(),
            Some("killed by signal 15")
        );
        assert_eq!(last_exit(&[]), None);
    }
}
