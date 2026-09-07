use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::Manager;

pub(super) const SCRIPT_NAME: &str = "gproxy.sh";

pub(super) fn boot_script(home: &Path) -> PathBuf {
    home.join(".termux/boot").join(SCRIPT_NAME)
}

// Detach the server so the boot job can finish. Android needs the bundled
// libc++_shared.so beside the executable, as in the interactive launcher.
pub(super) fn script(manager: &Manager, shell: &Path, log: &Path) -> String {
    let command = manager
        .command_parts()
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "#!{shell}\n\
         # Written by GPROXY. Runs at boot through the Termux:Boot add-on.\n\
         termux-wake-lock\n\
         cd {working_dir} || exit 1\n\
         {library_path}\
         set -- {command}\n\
         command -v setsid > /dev/null 2>&1 && set -- setsid \"$@\"\n\
         \"$@\" > {log} 2>&1 &\n",
        shell = shell.display(),
        working_dir = quote(manager.working_dir.as_os_str()),
        library_path = library_path(&manager.executable),
        log = quote(log.as_os_str()),
    )
}

fn library_path(executable: &Path) -> String {
    match executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(directory) => format!(
            "LD_LIBRARY_PATH={}\"${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport LD_LIBRARY_PATH\n",
            quote(directory.as_os_str())
        ),
        None => String::new(),
    }
}

fn quote(value: &OsStr) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> Manager {
        Manager {
            data_dir: PathBuf::from("/data/data/com.termux/files/home/gproxy/data"),
            executable: PathBuf::from("/home/user/gproxy bin/gproxy.bin"),
            args: vec![
                "--port".into(),
                "9000".into(),
                "--master-key".into(),
                "it's".into(),
            ],
            working_dir: PathBuf::from("/home/user/gproxy bin"),
        }
    }

    #[test]
    fn boot_script_detaches_the_server_and_quotes_every_part() {
        let script = script(
            &manager(),
            Path::new("/data/data/com.termux/files/usr/bin/sh"),
            Path::new("/home/user/data/autostart.log"),
        );

        assert!(script.starts_with("#!/data/data/com.termux/files/usr/bin/sh\n"));
        assert!(script.contains("\ntermux-wake-lock\n"));
        assert!(script.contains("\ncd '/home/user/gproxy bin' || exit 1\n"));
        assert!(script.contains(
            "\nLD_LIBRARY_PATH='/home/user/gproxy bin\'\"${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}\"\n"
        ));
        assert!(script.contains(
            "\nset -- '/home/user/gproxy bin/gproxy.bin' '--port' '9000' '--master-key' 'it'\\''s'\n"
        ));
        assert!(script.ends_with("\"$@\" > '/home/user/data/autostart.log' 2>&1 &\n"));
    }

    #[test]
    fn a_bare_executable_name_carries_no_library_path() {
        let mut manager = manager();
        manager.executable = PathBuf::from("gproxy");

        let script = script(&manager, Path::new("/usr/bin/sh"), Path::new("run.log"));

        assert!(!script.contains("LD_LIBRARY_PATH"));
    }

    #[test]
    fn the_boot_script_lives_where_the_add_on_looks() {
        assert_eq!(
            boot_script(Path::new("/data/data/com.termux/files/home")),
            PathBuf::from("/data/data/com.termux/files/home/.termux/boot/gproxy.sh")
        );
    }
}
