use std::path::Path;

use super::*;

pub(super) fn status(_manager: &AutoStartManager) -> AutoStartStatus {
    let Some(path) = user_entry() else {
        return unsupported("HOME is not set");
    };
    if std::env::var_os("container").is_some() || Path::new("/.dockerenv").exists() {
        return unsupported("disabled inside containers");
    }
    if ["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]
        .iter()
        .all(|name| std::env::var_os(name).is_none())
    {
        return unsupported("no desktop session detected");
    }
    let enabled = match std::fs::read_to_string(path) {
        Ok(contents) => !contents.lines().any(|line| line.trim() == "Hidden=true"),
        Err(_) => Path::new("/etc/xdg/autostart/gproxy.desktop").exists(),
    };
    AutoStartStatus {
        supported: true,
        enabled,
        platform: "linux",
        detail: None,
    }
}

pub(super) fn set_enabled(manager: &AutoStartManager, enabled: bool) -> anyhow::Result<()> {
    let path = user_entry().ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = if enabled {
        format!(
            "[Desktop Entry]\nType=Application\nName=GPROXY\nComment=GPROXY background service\nExec={}\nPath={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            manager
                .command_parts()
                .map(desktop_quote)
                .collect::<Vec<_>>()
                .join(" "),
            desktop_value(manager.working_dir.as_os_str()),
        )
    } else if Path::new("/etc/xdg/autostart/gproxy.desktop").exists() {
        "[Desktop Entry]\nType=Application\nName=GPROXY\nHidden=true\n".to_string()
    } else {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    };
    std::fs::write(path, contents)?;
    Ok(())
}

fn user_entry() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(root).join("autostart/gproxy.desktop"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/autostart/gproxy.desktop"))
}

fn desktop_quote(value: &OsStr) -> String {
    let escaped = value
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

fn desktop_value(value: &OsStr) -> String {
    value
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn unsupported(detail: &str) -> AutoStartStatus {
    AutoStartStatus {
        supported: false,
        enabled: false,
        platform: "linux",
        detail: Some(detail.to_string()),
    }
}
