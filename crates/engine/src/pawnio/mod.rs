//! PawnIO: detection, a thin wrapper over the installed `PawnIOLib.dll` and the embedded modules.
//!
//! The driver is installed by the user (`install`); the app never ships it. Opening the device
//! needs elevation, so a non-elevated process gets `PawnIoError::AccessDenied`.

pub mod install;

use libloading::Library;
use libloading::os::windows::{LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32};
use std::ffi::{CString, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_ANY, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegGetValueW,
};
use windows::core::{HRESULT, HSTRING, PCSTR};

/// AMD families 17h to 1Ah: `ioctl_read_smn` (and `ioctl_read_msr`). See resources/pawnio/README.md.
pub static AMD_FAMILY17_BIN: &[u8] = include_bytes!("../../resources/pawnio/AMDFamily17.bin");
/// Intel: `ioctl_read_msr`.
pub static INTEL_MSR_BIN: &[u8] = include_bytes!("../../resources/pawnio/IntelMSR.bin");

const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";
const SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services\PawnIO";
const LIB_NAME: &str = "PawnIOLib.dll";

/// `PawnIOLib.dll` of the installed PawnIO: the installer's `InstallLocation`, else
/// `%ProgramFiles%\PawnIO`. Only returns a path that exists.
pub fn find_lib() -> Option<PathBuf> {
    let registered = reg_path(UNINSTALL_KEY, "InstallLocation");
    let default = std::env::var_os("ProgramFiles").map(|dir| PathBuf::from(dir).join("PawnIO"));
    [registered, default].into_iter().flatten().map(|dir| dir.join(LIB_NAME)).find(|lib| lib.is_file())
}

/// The driver service is registered and its user-mode library is in place.
pub fn is_installed() -> bool {
    reg_value_exists(SERVICE_KEY, "Type") && find_lib().is_some()
}

#[derive(Debug)]
pub enum PawnIoError {
    NotInstalled,
    AccessDenied,
    Failed(String),
}

// Signatures from PawnIOLib.h (namazso/PawnIO); every function returns an HRESULT.
type OpenFn = unsafe extern "system" fn(handle: *mut HANDLE) -> HRESULT;
type LoadFn = unsafe extern "system" fn(handle: HANDLE, blob: *const u8, size: usize) -> HRESULT;
type ExecuteFn = unsafe extern "system" fn(
    handle: HANDLE,
    name: PCSTR,
    input: *const u64,
    in_size: usize,
    out: *mut u64,
    out_size: usize,
    return_size: *mut usize,
) -> HRESULT;
type CloseFn = unsafe extern "system" fn(handle: HANDLE) -> HRESULT;

/// One PawnIO executor handle with (at most) one module loaded.
pub struct PawnIo {
    handle: HANDLE,
    load: LoadFn,
    execute: ExecuteFn,
    close: CloseFn,
    // Keeps the function pointers above valid; dropped after `Drop::drop` closes the handle.
    _lib: Library,
}

// SAFETY: the executor handle is a kernel file handle usable from any thread, the function
// pointers are plain code addresses and `Library` is `Send`.
unsafe impl Send for PawnIo {}

impl PawnIo {
    pub fn open(lib: &Path) -> Result<Self, PawnIoError> {
        // SAFETY: PawnIOLib.dll runs no initialization code (its entry point just returns TRUE).
        // The search flags keep its imports to its own folder and System32.
        let lib: Library = unsafe {
            libloading::os::windows::Library::load_with_flags(
                lib,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .map_err(|e| PawnIoError::Failed(format!("loading {}: {e}", lib.display())))?
        .into();
        let open = *symbol::<OpenFn>(&lib, b"pawnio_open\0")?;
        let load = *symbol::<LoadFn>(&lib, b"pawnio_load\0")?;
        let execute = *symbol::<ExecuteFn>(&lib, b"pawnio_execute\0")?;
        let close = *symbol::<CloseFn>(&lib, b"pawnio_close\0")?;
        let mut handle = HANDLE::default();
        check("pawnio_open", unsafe { open(&mut handle) })?;
        Ok(Self { handle, load, execute, close, _lib: lib })
    }

    pub fn load_module(&mut self, blob: &[u8]) -> Result<(), PawnIoError> {
        check("pawnio_load", unsafe { (self.load)(self.handle, blob.as_ptr(), blob.len()) })
    }

    /// Runs `func` of the loaded module; returns the `out_len` or fewer values it wrote.
    pub fn execute(&self, func: &str, input: &[u64], out_len: usize) -> Result<Vec<u64>, PawnIoError> {
        let name = CString::new(func).map_err(|_| PawnIoError::Failed(format!("bad function name {func:?}")))?;
        let mut out = vec![0u64; out_len];
        let mut written = 0usize;
        let hr = unsafe {
            (self.execute)(
                self.handle,
                PCSTR(name.as_ptr().cast()),
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written,
            )
        };
        check(func, hr)?;
        out.truncate(written);
        Ok(out)
    }
}

impl Drop for PawnIo {
    fn drop(&mut self) {
        // Nothing to do if closing fails; the handle is not used again either way.
        let _ = unsafe { (self.close)(self.handle) };
    }
}

fn symbol<'lib, T>(lib: &'lib Library, name: &[u8]) -> Result<libloading::Symbol<'lib, T>, PawnIoError> {
    // SAFETY: `T` is the signature PawnIOLib.h declares for `name`.
    unsafe { lib.get::<T>(name) }.map_err(|e| PawnIoError::Failed(e.to_string()))
}

fn check(func: &str, hr: HRESULT) -> Result<(), PawnIoError> {
    if hr.is_ok() { Ok(()) } else { Err(error_from_hresult(func, hr)) }
}

/// PawnIOLib turns NTSTATUS into `HRESULT_FROM_WIN32(RtlNtStatusToDosError(status))`: a device
/// that does not exist (driver gone) is "file not found", a non-elevated caller is access denied.
fn error_from_hresult(func: &str, hr: HRESULT) -> PawnIoError {
    if hr == E_ACCESSDENIED {
        PawnIoError::AccessDenied
    } else if hr == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) || hr == HRESULT::from_win32(ERROR_PATH_NOT_FOUND.0) {
        PawnIoError::NotInstalled
    } else {
        PawnIoError::Failed(format!("{func} failed: {:#010X}", hr.0))
    }
}

/// Reads a value under HKLM from the 64-bit registry view.
fn reg_get(subkey: &str, value: &str, flags: REG_ROUTINE_FLAGS, data: Option<&mut [u16]>) -> Option<u32> {
    let mut size = data.as_ref().map_or(0, |d| (d.len() * 2) as u32);
    let data = data.map(|d| d.as_mut_ptr().cast());
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(subkey),
            &HSTRING::from(value),
            flags | RRF_SUBKEY_WOW6464KEY,
            None,
            data,
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(size)
}

fn reg_value_exists(subkey: &str, value: &str) -> bool {
    reg_get(subkey, value, RRF_RT_ANY, None).is_some()
}

fn reg_path(subkey: &str, value: &str) -> Option<PathBuf> {
    let size = reg_get(subkey, value, RRF_RT_REG_SZ, None)?;
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    reg_get(subkey, value, RRF_RT_REG_SZ, Some(&mut buf))?;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    (len > 0).then(|| OsString::from_wide(&buf[..len]).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_modules_are_present() {
        assert!(AMD_FAMILY17_BIN.len() > 64 && INTEL_MSR_BIN.len() > 64);
    }
    #[test]
    fn hresults_map_to_errors() {
        assert!(matches!(error_from_hresult("pawnio_open", E_ACCESSDENIED), PawnIoError::AccessDenied));
        let missing = HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0);
        assert!(matches!(error_from_hresult("pawnio_open", missing), PawnIoError::NotInstalled));
        match error_from_hresult("ioctl_read_smn", HRESULT(0x8007_0032_u32 as i32)) {
            PawnIoError::Failed(msg) => assert_eq!(msg, "ioctl_read_smn failed: 0x80070032"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
