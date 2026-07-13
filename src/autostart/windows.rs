use super::*;

pub(super) fn status(_manager: &AutoStartManager) -> AutoStartStatus {
    match entry() {
        Some(path) => AutoStartStatus {
            supported: true,
            enabled: path.exists(),
            platform: "windows",
            detail: None,
        },
        None => AutoStartStatus {
            supported: false,
            enabled: false,
            platform: "windows",
            detail: Some("APPDATA is not set".into()),
        },
    }
}

pub(super) fn set_enabled(manager: &AutoStartManager, enabled: bool) -> anyhow::Result<()> {
    let path = entry().ok_or_else(|| anyhow::anyhow!("APPDATA is not set"))?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let command = manager
        .command_parts()
        .map(windows_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "Set shell = CreateObject(\"WScript.Shell\")\r\nshell.CurrentDirectory = \"{}\"\r\nshell.Run \"{}\", 0, False\r\n",
        vbs_escape(manager.working_dir.as_os_str()),
        vbs_escape(OsStr::new(&command)),
    );
    std::fs::write(path, script)?;
    Ok(())
}

fn entry() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Microsoft/Windows/Start Menu/Programs/Startup/GPROXY.vbs"))
}

fn windows_quote(value: &OsStr) -> String {
    format!("\"{}\"", value.to_string_lossy().replace('"', "\\\""))
}

fn vbs_escape(value: &OsStr) -> String {
    value.to_string_lossy().replace('"', "\"\"")
}
