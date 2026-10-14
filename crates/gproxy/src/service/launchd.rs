//! macOS, as a per-user `LaunchAgent`.
//!
//! # Why an agent and not a daemon
//!
//! A `LaunchDaemon` in `/Library/LaunchDaemons` starts at boot with nobody
//! logged in — which is what `--autostart` asks for — but it runs as root
//! unless told otherwise, needs `sudo` to install, and would have gproxy
//! writing a database into a directory root owns. A gateway does not need
//! root, and a command that quietly asks for it is a command an operator
//! should not have run. So this installs a `LaunchAgent` under
//! `~/Library/LaunchAgents`, which runs as the operator, starts when they log
//! in, and is theirs to remove.
//!
//! The honest consequence is that `--autostart` cannot be honoured on macOS,
//! and [`install`] says so instead of pretending.
//!
//! # `bootstrap`/`bootout`, not `load`/`unload`
//!
//! `launchctl load` and `unload` are the pre-10.10 subcommands. They still
//! work and they still print nothing useful when they fail. `bootstrap gui/<uid>`
//! and `bootout gui/<uid>/<label>` are the supported spelling, take the domain
//! explicitly — so there is no question about which session the agent landed
//! in — and report a real error. `kickstart -k` is how a running agent is
//! restarted, and `print` is the only thing that answers what launchd
//! actually believes.

use std::path::{Path, PathBuf};

use super::{LABEL, Plan, Report, Result, home, remove, run_tool, uid, write_private};

const LEGACY_LABEL: &str = "io.github.leenhawk.gproxy";

fn registration_in(directory: &Path) -> (PathBuf, &'static str) {
    let current = directory.join(format!("{LABEL}.plist"));
    let legacy = directory.join(format!("{LEGACY_LABEL}.plist"));
    if !current.exists() && legacy.is_file() {
        (legacy, LEGACY_LABEL)
    } else {
        (current, LABEL)
    }
}

fn registration() -> Result<(PathBuf, &'static str)> {
    Ok(registration_in(&home()?.join("Library/LaunchAgents")))
}

pub fn plist_path() -> Result<PathBuf> {
    Ok(registration()?.0)
}

/// `gui/501`, the domain a login session's agents live in.
fn domain() -> Result<String> {
    Ok(format!("gui/{}", uid()?))
}

fn service_target(domain: &str, label: &str) -> String {
    format!("{domain}/{label}")
}

// ------------------------------------------------------------- the plist --

/// The agent's property list for `plan`.
///
/// Written by hand rather than through a plist crate. The document is nine
/// keys, the format is frozen, and a dependency here would be a dependency the
/// other three platforms pay for in compile time and nobody audits. The test
/// below parses what comes out, so "by hand" does not mean "unverified".
pub fn plist(plan: &Plan) -> String {
    plist_with_label(plan, LABEL)
}

fn plist_with_label(plan: &Plan, label: &str) -> String {
    let arguments = plan
        .command()
        .iter()
        .map(|arg| format!("\t\t<string>{}</string>\n", escape(arg)))
        .collect::<String>();
    let log = plan.log_file();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n{arguments}\t</array>\n\
         \t<key>WorkingDirectory</key>\n\
         \t<string>{working_dir}</string>\n\
         \t<!-- The path of the environment file, never its contents. launchd has no\n\
         \t     EnvironmentFile; gproxy reads this one itself. -->\n\
         \t<key>EnvironmentVariables</key>\n\
         \t<dict>\n\
         \t\t<key>GPROXY_ENV_FILE</key>\n\
         \t\t<string>{env_file}</string>\n\
         \t</dict>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<!-- Restart on a crash, not on a clean exit. A bare <true/> here, which is\n\
         \t     what v3 wrote, respawns a process the operator deliberately stopped. -->\n\
         \t<key>KeepAlive</key>\n\
         \t<dict>\n\
         \t\t<key>SuccessfulExit</key>\n\
         \t\t<false/>\n\
         \t</dict>\n\
         \t<key>ProcessType</key>\n\
         \t<string>Background</string>\n\
         \t<!-- launchd has no journal, so stderr goes somewhere an operator can find\n\
         \t     it: beside the database it belongs to. -->\n\
         \t<key>StandardOutPath</key>\n\
         \t<string>{log}</string>\n\
         \t<key>StandardErrorPath</key>\n\
         \t<string>{log}</string>\n\
         </dict>\n\
         </plist>\n",
        label = label,
        working_dir = escape(&plan.working_dir.to_string_lossy()),
        env_file = escape(&plan.env_file.to_string_lossy()),
        log = escape(&log.to_string_lossy()),
    )
}

/// The five XML entities. `<` and `&` are the two that matter — the others are
/// escaped because an unescaped `>` in a `<string>` is legal XML and confusing
/// to read, and because a parser in strict mode is entitled to object.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------- verbs --

pub fn install(plan: &Plan) -> Result<Report> {
    let (path, label) = registration()?;
    let domain = domain()?;
    let target = service_target(&domain, label);
    std::fs::create_dir_all(&plan.data_dir).map_err(|error| {
        crate::Error::io(format!("creating {}", plan.data_dir.display()), error)
    })?;
    write_private(&path, plist_with_label(plan, label).as_bytes())?;

    // Boot it out first, ignoring the failure that means it was not loaded:
    // `bootstrap` over an already-loaded label fails with "service already
    // loaded" and would leave the old plist running against the new file.
    let _ = run_tool("launchctl", &["bootout", &target]);
    let plist_arg = path.to_string_lossy().into_owned();
    run_tool("launchctl", &["bootstrap", &domain, &plist_arg])?
        .require(&format!("launchctl bootstrap {domain} {plist_arg}"))?;
    // `RunAtLoad` starts it, but only on a domain that was not already up.
    // `kickstart` makes "installed" and "running" the same moment either way.
    let _ = run_tool("launchctl", &["kickstart", "-k", &target]);

    let mut report = Report::new("gproxy is now a launchd user agent.")
        .row("plist", path.display())
        .row("label", label)
        .row("command", plan.command().join(" "))
        .row("data", plan.data_dir.display())
        .row("listen", format!("{}:{}", plan.host, plan.port))
        .row("logs", plan.log_file().display());

    if plan.autostart {
        report = report.note(
            "--autostart cannot be honoured on macOS, and nothing was done about it. A \
             LaunchAgent belongs to a login session: it starts when you log in and stops when \
             you log out. Starting with nobody logged in means a LaunchDaemon in \
             /Library/LaunchDaemons, which needs sudo and runs as root — and a gateway that \
             writes its database as root is worse than one that waits for a login. Install the \
             daemon by hand if that trade is the right one for this machine.",
        );
    }
    Ok(report.note(plan.key_note()))
}

pub fn uninstall() -> Result<Report> {
    let (path, label) = registration()?;
    let target = service_target(&domain()?, label);
    // Before the file is removed: `bootout` resolves the label through the
    // domain rather than through the file, but a plist that is gone makes the
    // failure message unhelpful.
    let booted = run_tool("launchctl", &["bootout", &target])?;
    let existed = remove(&path)?;

    let mut report = if existed {
        Report::new("gproxy is no longer a launchd user agent.").row("removed", path.display())
    } else {
        Report::new("There was no gproxy launch agent to remove.").row("looked in", path.display())
    };
    if !booted.ok && existed {
        report = report.note(format!(
            "`launchctl bootout {target}` complained: {}. The plist is gone, so it will not come \
             back at the next login.",
            booted.message()
        ));
    }
    Ok(report)
}

pub fn status() -> Result<Report> {
    let (path, label) = registration()?;
    let target = service_target(&domain()?, label);
    let printed = run_tool("launchctl", &["print", &target])?;
    if !printed.ok {
        return Ok(Report::new("launchd has no gproxy agent loaded.")
            .row("looked in", path.display())
            .row("label", label)
            .maybe(
                "plist",
                path.exists()
                    .then_some("present but not loaded — `gproxy service install` loads it"),
            )
            .note(printed.message()));
    }
    let fields = fields_of(&printed.stdout);
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    Ok(Report::new("gproxy, as launchd sees it.")
        .row("plist", path.display())
        .row("label", label)
        .maybe("state", field("state"))
        .maybe("pid", field("pid"))
        .maybe("runs", field("runs"))
        .maybe("last exit", field("last exit code"))
        .row("logs", "the StandardErrorPath in the plist"))
}

/// `launchctl print`'s body, which is `key = value` at some indentation inside
/// nested braces.
///
/// Only the flat `key = value` lines are taken, and the nesting is ignored:
/// the five facts this reads — `state`, `pid`, `runs`, `last exit code` — are
/// all at the top level of the service block, and a parser that understood the
/// braces would be a parser for a format Apple has changed before.
fn fields_of(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(" = "))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::tests::plan;

    /// A plist that is not well-formed XML is a plist `launchctl bootstrap`
    /// rejects with "Bootstrap failed: 5: Input/output error", which says
    /// nothing at all about what is wrong with it. So the document is parsed
    /// here, on every platform, rather than discovered on a Mac.
    #[test]
    fn the_plist_is_well_formed_xml() {
        let plist = plist(&plan());
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(plist.contains("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\""));
        assert!(plist.trim_end().ends_with("</plist>"));
        crate::service::xml::check(&plist).unwrap_or_else(|error| panic!("{error}\n{plist}"));
    }

    #[test]
    fn the_keys_launchd_needs_are_all_there_and_the_agent_is_a_background_one() {
        let plist = plist(&plan());
        for key in [
            "<key>Label</key>",
            "<key>ProgramArguments</key>",
            "<key>WorkingDirectory</key>",
            "<key>RunAtLoad</key>",
            "<key>KeepAlive</key>",
            "<key>ProcessType</key>",
            "<key>StandardErrorPath</key>",
        ] {
            assert!(plist.contains(key), "missing {key} in:\n{plist}");
        }
        assert!(plist.contains(&format!("<string>{LABEL}</string>")));
        assert!(plist.contains("<string>Background</string>"));
    }

    /// `KeepAlive` as a bare `<true/>` — which is what v3 wrote — respawns a
    /// process that exited cleanly, so a `launchctl bootout` races a restart.
    #[test]
    fn keep_alive_restarts_a_crash_and_not_a_clean_exit() {
        let plist = plist(&plan());
        assert!(
            plist.contains("<key>SuccessfulExit</key>\n\t\t<false/>"),
            "{plist}"
        );
        assert!(
            !plist.contains("<key>KeepAlive</key>\n\t<true/>"),
            "{plist}"
        );
    }

    #[test]
    fn every_argument_is_its_own_string_so_a_path_with_a_space_survives() {
        let plist = plist(&plan());
        assert!(
            plist.contains("<string>/opt/GPROXY bin/gproxy</string>"),
            "{plist}"
        );
        assert!(plist.contains("<string>--data-dir</string>"), "{plist}");
        assert!(
            plist.contains("<string>/srv/gproxy data/db</string>"),
            "{plist}"
        );
        // `ProgramArguments` is the executable plus its arguments, each once.
        assert_eq!(plist.matches("<string>serve</string>").count(), 1);
    }

    #[test]
    fn no_secret_reaches_the_plist_only_the_path_of_the_file_holding_one() {
        let plist = plist(&plan());
        assert!(!plist.contains("GPROXY_MASTER_KEY"), "{plist}");
        assert!(plist.contains("<key>GPROXY_ENV_FILE</key>"), "{plist}");
        assert!(
            plist.contains("<string>/srv/gproxy data/.env</string>"),
            "{plist}"
        );
    }

    #[test]
    fn xml_metacharacters_in_a_path_are_escaped() {
        let mut awkward = plan();
        awkward.data_dir = PathBuf::from("/srv/a&b/<c>/\"d\"");
        let plist = plist(&awkward);
        assert!(
            plist.contains("/srv/a&amp;b/&lt;c&gt;/&quot;d&quot;"),
            "{plist}"
        );
        crate::service::xml::check(&plist).unwrap();
    }

    #[test]
    fn the_domain_and_the_target_are_the_ones_launchctl_takes() {
        // `id -u` is POSIX, so the domain resolves on the test machine too.
        let domain = domain().unwrap();
        assert!(domain.starts_with("gui/"), "{domain}");
        assert!(domain["gui/".len()..].chars().all(|c| c.is_ascii_digit()));
        assert_eq!(service_target(&domain, LABEL), format!("{domain}/{LABEL}"));
    }
    #[test]
    fn existing_legacy_agents_keep_their_path_and_label() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join(format!("{LABEL}.plist"));
        let legacy = directory.path().join(format!("{LEGACY_LABEL}.plist"));
        assert_eq!(registration_in(directory.path()), (current.clone(), LABEL));

        std::fs::write(&legacy, plist_with_label(&plan(), LEGACY_LABEL)).unwrap();
        let (path, label) = registration_in(directory.path());
        assert_eq!(path, legacy);
        assert_eq!(
            service_target("gui/501", label),
            format!("gui/501/{LEGACY_LABEL}")
        );
        assert!(
            plist_with_label(&plan(), label).contains(&format!("<string>{LEGACY_LABEL}</string>"))
        );

        std::fs::write(&current, plist(&plan())).unwrap();
        assert_eq!(registration_in(directory.path()), (current, LABEL));
    }
    #[test]
    fn print_output_is_read_as_flat_key_value_pairs() {
        let fields = fields_of(
            "io.github.leenhawk.gproxy = {\n\tstate = running\n\tpid = 4711\n\t\
             last exit code = 0\n\tdomain = gui/501\n}\n",
        );
        assert!(fields.contains(&("state".to_owned(), "running".to_owned())));
        assert!(fields.contains(&("pid".to_owned(), "4711".to_owned())));
        assert!(fields.contains(&("last exit code".to_owned(), "0".to_owned())));
    }
}
