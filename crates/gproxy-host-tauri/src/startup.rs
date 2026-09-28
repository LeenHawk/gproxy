//! Per-user launch-at-login registration. The application runs without elevation.

#[cfg(not(target_os = "android"))]
use std::path::Path;

pub fn set(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        // Android's boot receiver reads the completed first-run settings.
        let _ = enabled;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        platform(&executable, enabled)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn save(path: &Path, content: &str, enabled: bool) -> Result<(), String> {
    if !enabled {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, content).map_err(|error| error.to_string())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn home() -> Result<std::path::PathBuf, String> {
    std::env::var_os("HOME")
        .map(Into::into)
        .ok_or_else(|| "could not locate the user's home directory".into())
}

#[cfg(target_os = "linux")]
fn platform(executable: &Path, enabled: bool) -> Result<(), String> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(path) => std::path::PathBuf::from(path),
        None => home()?.join(".config"),
    };
    let path = base.join("autostart/dev.gproxy.desktop.desktop");
    save(&path, &desktop_entry(executable)?, enabled)
}

#[cfg(target_os = "linux")]
fn desktop_entry(executable: &Path) -> Result<String, String> {
    let text = executable
        .to_str()
        .ok_or("the executable path is not valid UTF-8")?;
    if text.contains(['\n', '\r']) {
        return Err("the executable path contains a newline".into());
    }
    // Escape the Exec argument, then the desktop-entry string value itself.
    let argument = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    let exec = format!("\"{argument}\"").replace('\\', "\\\\");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=GPROXY\nExec={exec}\nIcon=gproxy-desktop\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    ))
}

#[cfg(target_os = "macos")]
fn platform(executable: &Path, enabled: bool) -> Result<(), String> {
    let path = home()?.join("Library/LaunchAgents/dev.gproxy.desktop.plist");
    let executable = executable
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>dev.gproxy.desktop</string><key>ProgramArguments</key><array><string>{executable}</string></array><key>RunAtLoad</key><true/></dict></plist>\n"
    );
    save(&path, &content, enabled)
}

#[cfg(target_os = "windows")]
fn platform(executable: &Path, enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows::Win32::{
        Foundation::APPMODEL_ERROR_NO_PACKAGE, Storage::Packaging::Appx::GetCurrentPackageFullName,
    };
    let mut length = 0;
    // A size query has no buffer to write and only tells us whether we have package identity.
    let result = unsafe { GetCurrentPackageFullName(&mut length, None) };
    if result != APPMODEL_ERROR_NO_PACKAGE {
        return packaged(enabled);
    }
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut command = Command::new("reg.exe");
    command
        .creation_flags(0x08000000)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if enabled {
        command
            .args(["ADD", key, "/v", "GPROXY Desktop", "/t", "REG_SZ", "/d"])
            .arg(format!("\"{}\"", executable.display()))
            .arg("/f");
    } else {
        let exists = Command::new("reg.exe")
            .args(["QUERY", key, "/v", "GPROXY Desktop"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| error.to_string())?
            .success();
        if !exists {
            return Ok(());
        }
        command.args(["DELETE", key, "/v", "GPROXY Desktop", "/f"]);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not configure launch at login: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(target_os = "windows")]
fn packaged(enabled: bool) -> Result<(), String> {
    use windows::{
        ApplicationModel::{StartupTask, StartupTaskState},
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    };
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| error.to_string())?;
    let result = (|| -> windows::core::Result<()> {
        let task = StartupTask::GetAsync(&"GproxyStartup".into())?.join()?;
        if enabled {
            let state = task.RequestEnableAsync()?.join()?;
            if state != StartupTaskState::Enabled && state != StartupTaskState::EnabledByPolicy {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80070005_u32 as i32),
                    "Windows disabled startup for GPROXY. Enable it in Settings > Apps > Startup.",
                ));
            }
        } else {
            task.Disable()?;
        }
        Ok(())
    })();
    unsafe { RoUninitialize() };
    result.map_err(|error| error.to_string())
}
