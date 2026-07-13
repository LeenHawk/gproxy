use super::*;

pub(super) fn status(_manager: &AutoStartManager) -> AutoStartStatus {
    match entry() {
        Some(path) => AutoStartStatus {
            supported: true,
            enabled: path.exists(),
            platform: "macos",
            detail: None,
        },
        None => unsupported(),
    }
}

pub(super) fn set_enabled(manager: &AutoStartManager, enabled: bool) -> anyhow::Result<()> {
    let path = entry().ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
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
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>io.github.leenhawk.gproxy</string>\n  <key>ProgramArguments</key>\n  <array>\n{args}\n  </array>\n  <key>WorkingDirectory</key><string>{}</string>\n  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n  <key>ProcessType</key><string>Background</string>\n</dict>\n</plist>\n",
        xml_escape(manager.working_dir.as_os_str()),
    );
    std::fs::write(path, plist)?;
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

fn unsupported() -> AutoStartStatus {
    AutoStartStatus {
        supported: false,
        enabled: false,
        platform: "macos",
        detail: Some("HOME is not set".into()),
    }
}
