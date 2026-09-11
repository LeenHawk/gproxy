use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

pub(crate) fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0;
        // A null buffer queries the required length without reading package data.
        let result = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
        match result {
            APPMODEL_ERROR_NO_PACKAGE => false,
            ERROR_INSUFFICIENT_BUFFER => true,
            error => panic!("Windows package identity query failed: {error}"),
        }
    })
}
