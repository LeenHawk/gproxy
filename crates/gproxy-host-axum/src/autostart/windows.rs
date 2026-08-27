use std::process::Command;

use super::*;

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "GPROXY";

pub(super) fn status(_manager: &Manager) -> Status {
    Status {
        supported: true,
        enabled: Command::new("reg.exe")
            .args(["query", KEY, "/v", VALUE])
            .status()
            .is_ok_and(|status| status.success()),
        platform: "windows".into(),
        detail: None,
    }
}

pub(super) fn set_enabled(manager: &Manager, enabled: bool) -> Result<(), Error> {
    let mut command = Command::new("reg.exe");
    if enabled {
        let value = run_command(manager);
        command.args([
            "add",
            KEY,
            "/v",
            VALUE,
            "/t",
            "REG_SZ",
            "/d",
            value.as_str(),
            "/f",
        ]);
    } else {
        if !status(manager).enabled {
            return Ok(());
        }
        command.args(["delete", KEY, "/v", VALUE, "/f"]);
    }
    if command.status()?.success() {
        Ok(())
    } else {
        Err(Error::Unsupported)
    }
}

fn run_command(manager: &Manager) -> String {
    let parts = manager
        .command_parts()
        .map(powershell_literal)
        .collect::<Vec<_>>();
    let executable = &parts[0];
    let args = parts[1..].join(",");
    format!(
        "powershell.exe -NoProfile -WindowStyle Hidden -Command \"Set-Location -LiteralPath {}; & {} {}\"",
        powershell_literal(manager.working_dir.as_os_str()),
        executable,
        args
    )
}

fn powershell_literal(value: &OsStr) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "''"))
}
