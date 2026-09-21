//! Windows, as a logon-triggered Task Scheduler task.
//!
//! # Why Task Scheduler and not a Windows service
//!
//! A Windows service has to be written as one: it must call
//! `StartServiceCtrlDispatcher` within thirty seconds of starting, answer
//! `SERVICE_CONTROL_STOP`, and report its state back to the SCM. A plain
//! console program registered with `sc create` is started, fails to check in,
//! and is killed with "did not respond in a timely fashion" — so a real
//! service would mean a second entry point in this binary, a `windows-service`
//! dependency, and a code path nothing else in the product exercises.
//! Installing one also needs Administrator.
//!
//! A scheduled task with a logon trigger needs no privilege beyond the user's
//! own, runs an ordinary foreground program, and restarts it on failure. That
//! is the whole of what `serve` needs from a supervisor.
//!
//! # Why `schtasks.exe` and not the COM API
//!
//! `ITaskService` is the richer interface, and taking it would mean the
//! `windows` crate — hundreds of generated bindings, a COM apartment to
//! initialize, `BSTR`s to free, and `HRESULT`s to translate into sentences an
//! operator can act on — in the one crate in this workspace that is supposed
//! to be thin. `schtasks.exe` is in every Windows install, is the interface
//! Microsoft documents for scripting, and takes **the same XML** that
//! `ITaskService::NewTask` produces. So the artifact is identical either way,
//! and the artifact is the part worth getting right: [`task_xml`] is what the
//! tests check, and it would not change if the delivery mechanism did.
//!
//! `/create /xml` rather than `/create /sc onlogon /tr "…"`, for the same
//! reason: `/tr` is one string that Task Scheduler re-splits, it is limited to
//! 261 characters, and it has no room for a restart policy or an execution
//! time limit.
//!
//! # Two gotchas that cost a release each in v3's lifetime
//!
//! - **`schtasks /create /xml` wants UTF-16.** A UTF-8 file, with or without a
//!   BOM, is rejected with `The task XML is malformed` or
//!   `ERROR: Invalid XML syntax` depending on the build. So the file is
//!   written UTF-16LE with a byte-order mark; see [`utf16le`].
//! - **A scheduled task captures no output.** There is no journal and no
//!   `StandardErrorPath`, so the action is `cmd.exe /d /s /c` with a
//!   redirection, and the `/s` is load-bearing: it makes `cmd` strip exactly
//!   the outer pair of quotes instead of guessing, which is the only reliable
//!   way to pass a quoted program path *and* a quoted redirection target in
//!   one string.

use std::path::PathBuf;

use super::{NAME, Plan, Report, Result, run_tool, write_private};

/// The task's name, as `schtasks /tn` takes it. A leading `\` would put it at
/// the root of the task library next to Microsoft's own folders; without one
/// it lands in the root anyway but reads as a relative name, which is what
/// every example in the documentation uses.
pub const TASK: &str = NAME;

/// Where the XML is kept after the task is created.
///
/// Task Scheduler copies the definition into its own store, so this file is
/// not read again. It is kept anyway: it is the record of what was installed,
/// it is what `uninstall` deletes to prove it, and it is the thing to look at
/// when the task in the store and the command an operator expected disagree.
pub fn definition_path(plan: &Plan) -> PathBuf {
    plan.data_dir.join("gproxy-task.xml")
}

// --------------------------------------------------------------- the task --

/// The task definition for `plan`.
pub fn task_xml(plan: &Plan) -> String {
    let log = plan.log_file();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
         <Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
         \t<RegistrationInfo>\n\
         \t\t<Description>GPROXY — one gateway in front of many LLM providers. Written by \
         `gproxy service install`.</Description>\n\
         \t\t<URI>\\{task}</URI>\n\
         \t</RegistrationInfo>\n\
         \t<Triggers>\n\
         \t\t<LogonTrigger>\n\
         \t\t\t<Enabled>true</Enabled>\n\
         \t\t</LogonTrigger>\n\
         \t</Triggers>\n\
         \t<Principals>\n\
         \t\t<Principal id=\"Author\">\n\
         \t\t\t<LogonType>InteractiveToken</LogonType>\n\
         \t\t\t<RunLevel>LeastPrivilege</RunLevel>\n\
         \t\t</Principal>\n\
         \t</Principals>\n\
         \t<Settings>\n\
         \t\t<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
         \t\t<!-- A gateway is not a laptop-battery concern: a task that stops when the\n\
         \t\t     machine unplugs is a gateway that stops answering. -->\n\
         \t\t<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
         \t\t<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
         \t\t<AllowHardTerminate>true</AllowHardTerminate>\n\
         \t\t<StartWhenAvailable>true</StartWhenAvailable>\n\
         \t\t<RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n\
         \t\t<!-- The default is PT72H, after which Task Scheduler kills a healthy\n\
         \t\t     server. PT0S is the documented spelling of no limit. -->\n\
         \t\t<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
         \t\t<Enabled>true</Enabled>\n\
         \t\t<Hidden>false</Hidden>\n\
         \t\t<RunOnlyIfIdle>false</RunOnlyIfIdle>\n\
         \t\t<WakeToRun>false</WakeToRun>\n\
         \t\t<Priority>5</Priority>\n\
         \t\t<RestartOnFailure>\n\
         \t\t\t<Interval>PT1M</Interval>\n\
         \t\t\t<Count>3</Count>\n\
         \t\t</RestartOnFailure>\n\
         \t</Settings>\n\
         \t<Actions Context=\"Author\">\n\
         \t\t<Exec>\n\
         \t\t\t<Command>cmd.exe</Command>\n\
         \t\t\t<Arguments>{arguments}</Arguments>\n\
         \t\t\t<WorkingDirectory>{working_dir}</WorkingDirectory>\n\
         \t\t</Exec>\n\
         \t</Actions>\n\
         </Task>\n",
        task = escape(TASK),
        arguments = escape(&cmd_arguments(plan, &log)),
        working_dir = escape(&plan.working_dir.to_string_lossy()),
    )
}

/// The `cmd.exe` argument string: the command, with its output appended to a
/// log beside the database.
///
/// `/d` skips `AutoRun` from the registry, which would otherwise run a
/// stranger's command before gproxy. `/s` and the outer quotes are the pair
/// that makes the rest work: with `/s`, `cmd` removes the first and last
/// character of the remainder when both are quotes and runs what is left
/// verbatim — no counting, no guessing, and the inner quotes around the
/// program path and the log path survive.
fn cmd_arguments(plan: &Plan, log: &std::path::Path) -> String {
    let command = plan
        .command()
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "/d /s /c \"{command} >> {} 2>&1\"",
        quote(&log.to_string_lossy())
    )
}

/// One argument for `cmd.exe`, double-quoted.
///
/// Windows has no escape for a double quote inside a `cmd` string that also
/// survives `CommandLineToArgvW`, so a path containing one cannot be passed
/// and is refused by the only means available here: the quote is dropped, and
/// the test records that it is. In practice `"` is not a legal character in a
/// Windows path at all, which is why this is a note rather than a `Result`.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', ""))
}

/// The five XML entities.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// UTF-16LE with a byte-order mark, which is what `schtasks /create /xml`
/// insists on and what nothing about the flag's documentation mentions.
pub fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

// ----------------------------------------------------------------- verbs --

pub fn install(plan: &Plan) -> Result<Report> {
    std::fs::create_dir_all(&plan.data_dir).map_err(|error| {
        crate::Error::io(format!("creating {}", plan.data_dir.display()), error)
    })?;
    let definition = definition_path(plan);
    write_private(&definition, &utf16le(&task_xml(plan)))?;

    let definition_arg = definition.to_string_lossy().into_owned();
    // `/f` so a reinstall replaces the task instead of asking a question
    // nothing is there to answer.
    run_tool(
        "schtasks.exe",
        &["/create", "/tn", TASK, "/xml", &definition_arg, "/f"],
    )?
    .require(&format!("schtasks /create /tn {TASK} /xml"))?;
    // A logon trigger fires at the *next* logon, so the install has to start
    // it, or `install` reports a service that is not running.
    let started = run_tool("schtasks.exe", &["/run", "/tn", TASK])?;

    let mut report = Report::new("gproxy is now a scheduled task.")
        .row("task", format!("\\{TASK}"))
        .row("definition", definition.display())
        .row("command", plan.command().join(" "))
        .row("data", plan.data_dir.display())
        .row("listen", format!("{}:{}", plan.host, plan.port))
        .row("logs", plan.log_file().display())
        .row("trigger", "at logon, and restarted 3 times on failure");

    if !started.ok {
        report = report.note(format!(
            "`schtasks /run /tn {TASK}` complained: {}. The task is registered and will start at \
             the next logon.",
            started.message()
        ));
    }
    if plan.autostart {
        report = report.note(
            "--autostart cannot be honoured on Windows without elevation, and nothing was done \
             about it. This task runs under your own interactive token, which means it starts \
             when you log on. A task that starts at boot with nobody logged on must run as \
             SYSTEM or store your password in the task store — the first runs the gateway with \
             more privilege than it needs, the second puts a password where a unit file's worth \
             of people can read it. Neither is a default this command will pick for you.",
        );
    }
    Ok(report.note(plan.key_note()))
}

pub fn uninstall() -> Result<Report> {
    let deleted = run_tool("schtasks.exe", &["/delete", "/tn", TASK, "/f"])?;
    // The definition lives in the data directory, which this command does not
    // know without a plan. `uninstall` takes no arguments on purpose — an
    // operator removing a service should not have to remember how they
    // installed it — so the file is left, and the report says where to look.
    let mut report = if deleted.ok {
        Report::new("gproxy is no longer a scheduled task.").row("removed", format!("\\{TASK}"))
    } else {
        Report::new("There was no gproxy task to remove.")
            .row("looked for", format!("\\{TASK}"))
            .note(deleted.message())
    };
    report = report.note(
        "The `gproxy-task.xml` in the data directory is the record of what was installed, not \
         the task itself — Task Scheduler copied the definition into its own store. It is \
         harmless to keep and harmless to delete.",
    );
    Ok(report)
}

pub fn status() -> Result<Report> {
    let queried = run_tool(
        "schtasks.exe",
        &["/query", "/tn", TASK, "/fo", "LIST", "/v"],
    )?;
    if !queried.ok {
        return Ok(Report::new("Task Scheduler has no gproxy task.")
            .row("looked for", format!("\\{TASK}"))
            .note("`gproxy service install` registers it.")
            .note(queried.message()));
    }
    let fields = fields_of(&queried.stdout);
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty() && value != "N/A")
    };
    Ok(Report::new("gproxy, as Task Scheduler sees it.")
        .row("task", format!("\\{TASK}"))
        .maybe("status", field("Status"))
        .maybe("enabled", field("Scheduled Task State"))
        .maybe("last run", field("Last Run Time"))
        // `Last Result` is the process's exit code, as a signed decimal. `0`
        // is a clean exit and `267009` is "the task is still running", which
        // is the one value that looks like a failure and is not.
        .maybe("last exit", field("Last Result"))
        .maybe("next run", field("Next Run Time"))
        .maybe("runs as", field("Run As User")))
}

/// `schtasks /fo LIST`'s output, which is `Label: value` with the label padded.
fn fields_of(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::tests::plan;

    #[test]
    fn the_task_is_well_formed_xml() {
        let xml = task_xml(&plan());
        crate::service::xml::check(&xml).unwrap_or_else(|error| panic!("{error}\n{xml}"));
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n"));
        assert!(xml.trim_end().ends_with("</Task>"));
    }

    /// The elements Task Scheduler refuses a definition without: a namespace,
    /// a trigger, a principal and an action.
    #[test]
    fn the_task_has_the_elements_task_scheduler_requires() {
        let xml = task_xml(&plan());
        for required in [
            "xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"",
            "<RegistrationInfo>",
            "<Triggers>",
            "<LogonTrigger>",
            "<Principals>",
            "<Settings>",
            "<Actions Context=\"Author\">",
            "<Exec>",
            "<Command>cmd.exe</Command>",
            "<Arguments>",
            "<WorkingDirectory>",
        ] {
            assert!(xml.contains(required), "missing {required} in:\n{xml}");
        }
    }

    /// The default `ExecutionTimeLimit` is PT72H, which kills a healthy
    /// gateway after three days.
    #[test]
    fn the_task_has_no_execution_time_limit_and_restarts_on_failure() {
        let xml = task_xml(&plan());
        assert!(
            xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"),
            "{xml}"
        );
        assert!(xml.contains("<RestartOnFailure>"), "{xml}");
        assert!(xml.contains("<Count>3</Count>"), "{xml}");
        // And a laptop unplugging is not a reason to stop answering.
        assert!(
            xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"),
            "{xml}"
        );
    }

    /// The action, with the `/d /s /c` and the quoting that makes a path with
    /// a space in it survive both `cmd` and `CommandLineToArgvW`.
    #[test]
    fn the_action_wraps_the_command_in_cmd_with_a_redirection() {
        let plan = plan();
        let arguments = cmd_arguments(&plan, &plan.log_file());
        assert_eq!(
            arguments,
            "/d /s /c \"\"/opt/GPROXY bin/gproxy\" \"serve\" \"--host\" \"127.0.0.1\" \
             \"--port\" \"9000\" \"--data-dir\" \"/srv/gproxy data/db\" >> \
             \"/srv/gproxy data/db/service.log\" 2>&1\""
        );
        // The outer pair `/s` strips, and nothing else.
        assert!(arguments.starts_with("/d /s /c \""));
        assert!(arguments.ends_with('"'));
    }

    #[test]
    fn the_redirection_is_escaped_in_the_xml_rather_than_left_to_break_it() {
        let xml = task_xml(&plan());
        // `>>` and `2>&1` inside <Arguments> would otherwise be markup.
        assert!(xml.contains("&gt;&gt;"), "{xml}");
        assert!(xml.contains("2&gt;&amp;1"), "{xml}");
        assert!(!xml.contains("2>&1"), "{xml}");
        crate::service::xml::check(&xml).unwrap();
    }

    /// The gotcha: `schtasks /create /xml` rejects a UTF-8 file.
    #[test]
    fn the_definition_is_written_utf16le_with_a_byte_order_mark() {
        let bytes = utf16le("<a/>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        assert_eq!(&bytes[2..], &[b'<', 0, b'a', 0, b'/', 0, b'>', 0]);
        // And the declaration says UTF-16, matching the bytes.
        assert!(task_xml(&plan()).contains("encoding=\"UTF-16\""));
        assert_eq!(utf16le("").len(), 2);
    }

    #[test]
    fn a_non_ascii_path_round_trips_through_the_encoding() {
        let mut plan = plan();
        plan.data_dir = PathBuf::from("C:\\Users\\Zoë\\gproxy");
        let bytes = utf16le(&task_xml(&plan));
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_le_bytes)
            .collect();
        let back = String::from_utf16(&units).unwrap();
        assert!(back.contains("Zoë"), "{back}");
        crate::service::xml::check(&back).unwrap();
    }

    #[test]
    fn no_secret_reaches_the_task() {
        let xml = task_xml(&plan());
        assert!(!xml.contains("GPROXY_MASTER_KEY"), "{xml}");
        // Task Scheduler has no environment block at all, so the key can only
        // arrive through the file gproxy reads for itself.
        assert!(!xml.contains("<Environment"), "{xml}");
    }

    #[test]
    fn the_definition_is_kept_beside_the_database_it_describes() {
        let plan = plan();
        assert_eq!(
            definition_path(&plan),
            plan.data_dir.join("gproxy-task.xml")
        );
    }

    #[test]
    fn list_output_is_read_as_label_and_value() {
        let fields = fields_of(
            "Folder: \\\r\nHostName:      BOX\r\nTaskName:      \\gproxy\r\n\
             Last Run Time: 21/09/2026 10:00:00\r\nStatus:        Running\r\n\
             Last Result:   267009\r\n",
        );
        assert!(fields.contains(&("Status".to_owned(), "Running".to_owned())));
        assert!(fields.contains(&("Last Result".to_owned(), "267009".to_owned())));
        // A value containing a colon keeps the rest of it.
        let time = fields
            .iter()
            .find(|(key, _)| key == "Last Run Time")
            .unwrap();
        assert_eq!(time.1, "21/09/2026 10:00:00");
    }
}
