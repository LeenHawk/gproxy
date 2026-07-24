use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_FAILED};
use windows_sys::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

pub(crate) fn wait_for_parent(parent_pid: u32) -> anyhow::Result<()> {
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_pid) };
    if process.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(());
        }
        return Err(error.into());
    }

    tracing::info!(parent_pid, "waiting for previous version to exit");
    let result = unsafe { WaitForSingleObject(process, INFINITE) };
    let wait_error = (result == WAIT_FAILED).then(std::io::Error::last_os_error);
    unsafe { CloseHandle(process) };
    if let Some(error) = wait_error {
        anyhow::bail!("failed waiting for previous process: {error}");
    }
    Ok(())
}
