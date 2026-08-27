use super::*;

pub(super) fn status(_manager: &Manager) -> Status {
    match entry() {
        Some(path) => Status {
            supported: true,
            enabled: path.exists(),
            platform: "macos".into(),
            detail: None,
        },
        None => Status {
            supported: false,
            enabled: false,
            platform: "macos".into(),
            detail: Some("home".into()),
        },
    }
}

pub(super) fn set_enabled(manager: &Manager, enabled: bool) -> Result<(), Error> {
    let path = entry().ok_or(Error::Unsupported)?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let args = manager
        .command_parts()
        .map(|arg| format!("    <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");
    let working_dir = xml_escape(manager.working_dir.as_os_str());
    std::fs::write(
        path,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>io.github.leenhawk.gproxy</string>\n<key>ProgramArguments</key><array>\n{args}\n</array>\n<key>WorkingDirectory</key><string>{working_dir}</string>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>ProcessType</key><string>Background</string>\n</dict></plist>\n"
        ),
    )?;
    Ok(())
}

fn entry() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/LaunchAgents/io.github.leenhawk.gproxy.plist"))
}

fn xml_escape(value: &OsStr) -> String {
    value
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
