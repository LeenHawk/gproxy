#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

fn launch() -> std::io::Result<()> {
    let script = std::env::current_exe()?.with_file_name("GPROXY.ps1");
    let system = std::env::var_os("SystemRoot").expect("Windows provides SystemRoot");
    let powershell =
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg("-Packaged")
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !std::env::args_os().any(|arg| arg == "--autostart") {
        command.arg("-OpenConsole");
    }
    command.spawn()?;
    Ok(())
}

fn main() {
    if let Err(error) = launch() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut core::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                kind: u32,
            ) -> i32;
        }
        let text = format!("GPROXY could not start: {error}\0")
            .encode_utf16()
            .collect::<Vec<_>>();
        let title = "GPROXY\0".encode_utf16().collect::<Vec<_>>();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
        }
        std::process::exit(1);
    }
}
