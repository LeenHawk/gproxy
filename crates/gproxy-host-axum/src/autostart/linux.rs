use std::path::Path;

use super::*;

pub(super) fn status(_manager: &Manager) -> Status {
    let Some(path) = user_entry() else {
        return unsupported("home");
    };
    if std::env::var_os("container").is_some() || Path::new("/.dockerenv").exists() {
        return unsupported("container");
    }
    if ["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]
        .iter()
        .all(|name| std::env::var_os(name).is_none())
    {
        return unsupported("desktop");
    }
    Status {
        supported: true,
        enabled: entry_enabled(&path),
        platform: "linux".into(),
        detail: None,
    }
}

pub(super) fn set_enabled(manager: &Manager, enabled: bool) -> Result<(), Error> {
    let path = user_entry().ok_or(Error::Unsupported)?;
    write_entry(manager, &path, enabled)
}

fn write_entry(manager: &Manager, path: &Path, enabled: bool) -> Result<(), Error> {
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
        .map(desktop_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let working_dir = desktop_value(manager.working_dir.as_os_str());
    std::fs::write(
        path,
        format!(
            "[Desktop Entry]\nType=Application\nName=GPROXY\nExec={command}\nPath={working_dir}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        ),
    )?;
    Ok(())
}

fn entry_enabled(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|contents| !contents.lines().any(|line| line.trim() == "Hidden=true"))
        .unwrap_or(false)
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

fn unsupported(detail: &str) -> Status {
    Status {
        supported: false,
        enabled: false,
        platform: "linux".into(),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_round_trips_command_with_master_key() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("gproxy.desktop");
        let manager = Manager {
            data_dir: root.path().join("data"),
            executable: PathBuf::from("/opt/GPROXY bin/gproxy"),
            args: vec![
                "--port".into(),
                "9000".into(),
                "--master-key".into(),
                "test-key".into(),
            ],
            working_dir: PathBuf::from("/srv/gproxy data"),
        };

        write_entry(&manager, &path, true).unwrap();
        let entry = std::fs::read_to_string(&path).unwrap();
        assert!(entry_enabled(&path));
        assert!(entry.contains(
            "\"/opt/GPROXY bin/gproxy\" \"--port\" \"9000\" \"--master-key\" \"test-key\""
        ));
        write_entry(&manager, &path, false).unwrap();
        assert!(!path.exists());
    }
}
