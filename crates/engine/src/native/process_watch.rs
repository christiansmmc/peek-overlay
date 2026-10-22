//! Process-exit notification without polling: a thread-pool wait on the process handle.

use std::ffi::c_void;
use std::sync::{Mutex, PoisonError};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, RegisterWaitForSingleObject, UnregisterWaitEx, WT_EXECUTEONLYONCE,
};

type Callback = Mutex<Option<Box<dyn FnOnce() + Send>>>;

/// A pending wait on a process. Dropping it cancels the wait; if the callback is running, the
/// drop waits for it to return, so the callback must never drop its own `ExitWatch` (hand the
/// work to another thread instead).
pub struct ExitWatch {
    wait: HANDLE,
    process: HANDLE,
    callback: *mut Callback,
}

// SAFETY: the handles are process-wide kernel handles and `callback` is only touched by the
// thread-pool callback and by `drop`, which first waits for that callback to finish.
unsafe impl Send for ExitWatch {}

/// Calls `on_exit` once, on a thread-pool thread, when process `pid` ends (right away if it
/// already ended). `None` when the process can't be opened, e.g. a protected process.
pub fn watch_exit(pid: u32, on_exit: Box<dyn FnOnce() + Send>) -> Option<ExitWatch> {
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok()?;
    let callback: *mut Callback = Box::into_raw(Box::new(Mutex::new(Some(on_exit))));
    let mut wait = HANDLE::default();
    let registered = unsafe {
        RegisterWaitForSingleObject(
            &mut wait,
            process,
            Some(on_signaled),
            Some(callback as *const c_void),
            INFINITE,
            WT_EXECUTEONLYONCE,
        )
    };
    if registered.is_err() {
        unsafe {
            drop(Box::from_raw(callback));
            let _ = CloseHandle(process);
        }
        return None;
    }
    Some(ExitWatch { wait, process, callback })
}

unsafe extern "system" fn on_signaled(context: *mut c_void, _timed_out: bool) {
    // SAFETY: `context` is the `Callback` box of a live `ExitWatch`: its drop frees the box only
    // after `UnregisterWaitEx` has waited for this function to return.
    let callback = unsafe { &*(context as *const Callback) };
    let on_exit = callback.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(on_exit) = on_exit {
        on_exit();
    }
}

impl Drop for ExitWatch {
    fn drop(&mut self) {
        unsafe {
            // INVALID_HANDLE_VALUE: block until a running callback returns, so the box is free.
            let _ = UnregisterWaitEx(self.wait, Some(INVALID_HANDLE_VALUE));
            drop(Box::from_raw(self.callback));
            let _ = CloseHandle(self.process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_exit_fires_when_process_ends() {
        let mut child = std::process::Command::new("cmd").args(["/c", "ping -n 2 127.0.0.1 >nul"]).spawn().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let _w = watch_exit(child.id(), Box::new(move || { let _ = tx.send(()); })).unwrap();
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok());
        child.wait().unwrap();
    }

    #[test]
    fn dropping_the_watch_cancels_it() {
        use std::sync::mpsc::RecvTimeoutError;
        let mut child = std::process::Command::new("cmd").args(["/c", "ping -n 2 127.0.0.1 >nul"]).spawn().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let w = watch_exit(child.id(), Box::new(move || { let _ = tx.send(()); })).unwrap();
        drop(w);
        child.wait().unwrap();
        // The callback was freed without running: its sender is gone and nothing was sent.
        assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)), Err(RecvTimeoutError::Disconnected));
    }

    #[test]
    fn process_that_cannot_be_opened_gives_none() {
        assert!(watch_exit(0, Box::new(|| {})).is_none());
    }
}
