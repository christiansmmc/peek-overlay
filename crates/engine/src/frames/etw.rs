//! Real-time ETW session that turns DXGI/D3D9 `Present` events into frame timestamps.
//!
//! Providers, event IDs, level and keywords follow PresentMon
//! (`PresentData/ETW/Microsoft_Windows_DXGI.h`, `Microsoft_Windows_D3D9.h` and
//! `EnableProvidersListing` in `PresentMonTraceSession.cpp`).

use super::stats::{FrameTracker, PresentObserver};
use ferrisetw::native::{EvntraceNativeError, TraceHandle};
use ferrisetw::provider::{EventFilter, Provider, TraceFlags};
use ferrisetw::trace::{TraceBuilder, TraceError, TraceTrait, UserTrace};
use ferrisetw::{EventRecord, GUID, SchemaLocator};
use std::mem::offset_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, ControlTraceW, EVENT_TRACE_CONTROL, EVENT_TRACE_CONTROL_FLUSH,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_PROPERTIES,
};
use windows::core::PCWSTR;

pub const SESSION_NAME: &str = "PerfOverlay-Frames";

/// Microsoft-Windows-DXGI: `Present_Start` and `PresentMultiplaneOverlay_Start`.
const DXGI: GUID = GUID::from_u128(0xca11c036_0102_4a2d_a6ad_f03cfed5d3c9);
const DXGI_PRESENT_EVENTS: [u16; 2] = [42, 55];
/// Microsoft-Windows-D3D9: `Present_Start`.
const D3D9: GUID = GUID::from_u128(0x783aca0a_790e_4d7f_8451_aa850511c6b9);
const D3D9_PRESENT_EVENTS: [u16; 1] = [1];
/// These events are `win:LogAlways` with keyword `Events | <provider>_Analytic`.
const LEVEL: u8 = 0;
const KEYWORD: u64 = 0x8000_0000_0000_0002;
/// On its own ETW hands real-time buffers to the consumer only about once per
/// second. Flushing more often keeps presents fresh for the 500 ms FPS window
/// and the 250 ms streak gap.
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// Frame state shared by the ETW callback and its readers.
pub struct FrameShared {
    pub tracker: Mutex<FrameTracker>,
    pub observer: Mutex<PresentObserver>,
    /// Pid whose presents feed `tracker`; 0 = none.
    target: AtomicU32,
}

impl FrameShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tracker: Mutex::new(FrameTracker::new()),
            observer: Mutex::new(PresentObserver::new()),
            target: AtomicU32::new(0),
        })
    }

    /// Changes the pid that feeds `tracker`, clearing it when the target changes.
    pub fn set_target(&self, pid: Option<u32>) {
        // The target only changes under the tracker lock, so `on_present`
        // can never push a present of the previous target after the clear.
        let mut tracker = lock(&self.tracker);
        let pid = pid.unwrap_or(0);
        if self.target.swap(pid, Ordering::Relaxed) != pid {
            tracker.clear();
        }
    }

    pub fn target(&self) -> Option<u32> {
        match self.target.load(Ordering::Relaxed) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Every present feeds the observer; only the target's feed the tracker.
    pub fn on_present(&self, pid: u32, t_ns: u64) {
        lock(&self.observer).on_present(pid, t_ns);
        if pid != 0 && self.target.load(Ordering::Relaxed) == pid {
            let mut tracker = lock(&self.tracker);
            if self.target.load(Ordering::Relaxed) == pid {
                tracker.on_present(t_ns);
            }
        }
    }
}

/// A panic elsewhere must not take the ETW callback down with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
pub enum CaptureError {
    /// Real-time ETW sessions need administrator rights.
    AccessDenied,
    Other(String),
}

/// A running ETW session feeding a [`FrameShared`]. Dropping it also stops the
/// session; [`FrameCapture::stop`] additionally waits for its threads.
pub struct FrameCapture {
    session: UserTrace,
    processor: JoinHandle<()>,
    flusher: JoinHandle<()>,
    stop_flusher: Sender<()>,
}

impl FrameCapture {
    pub fn start(shared: Arc<FrameShared>) -> Result<FrameCapture, CaptureError> {
        let (session, handle) = start_session(&shared).map_err(|e| {
            // ferrisetw leaves the session running when enabling a provider
            // or opening the trace fails after StartTraceW succeeded.
            if !is_already_exists(&e) {
                let _ = control_session(EVENT_TRACE_CONTROL_STOP);
            }
            capture_error(e)
        })?;
        let processor = thread::Builder::new()
            .name("etw-frames".into())
            .spawn(move || {
                if let Err(e) = UserTrace::process_from_handle(handle) {
                    crate::trace!("etw: ProcessTrace failed: {e:?}");
                }
            })
            .map_err(|e| CaptureError::Other(e.to_string()))?;
        let (stop_flusher, stop_rx) = mpsc::channel::<()>();
        let flusher = thread::Builder::new()
            .name("etw-flush".into())
            .spawn(move || {
                while let Err(RecvTimeoutError::Timeout) = stop_rx.recv_timeout(FLUSH_INTERVAL) {
                    let _ = control_session(EVENT_TRACE_CONTROL_FLUSH);
                }
            })
            .map_err(|e| CaptureError::Other(e.to_string()))?;
        Ok(FrameCapture { session, processor, flusher, stop_flusher })
    }

    /// Stops the session and waits for its threads to finish.
    pub fn stop(self) {
        let FrameCapture { session, processor, flusher, stop_flusher } = self;
        drop(stop_flusher);
        let _ = flusher.join();
        if let Err(e) = session.stop() {
            crate::trace!("etw: stopping the session failed: {e:?}");
            // ferrisetw skips the stop when closing the trace fails, and ProcessTrace would
            // then never return: stop the session by name so the join below cannot hang.
            if let Err(e) = control_session(EVENT_TRACE_CONTROL_STOP) {
                crate::trace!("etw: stopping the session by name failed: {e:?}");
            }
        }
        // ProcessTrace returns once the trace is closed or the session stopped.
        let _ = processor.join();
    }
}

/// Starts the session, replacing a stale one left behind by a crashed run (once).
fn start_session(shared: &Arc<FrameShared>) -> Result<(UserTrace, TraceHandle), TraceError> {
    match session_builder(shared).start() {
        Err(e) if is_already_exists(&e) => {
            let _ = control_session(EVENT_TRACE_CONTROL_STOP);
            crate::trace!("stopped stale session");
            session_builder(shared).start()
        }
        result => result,
    }
}

fn session_builder(shared: &Arc<FrameShared>) -> TraceBuilder<UserTrace> {
    UserTrace::new()
        .named(SESSION_NAME.to_string())
        .enable(present_provider(DXGI, &DXGI_PRESENT_EVENTS, shared))
        .enable(present_provider(D3D9, &D3D9_PRESENT_EVENTS, shared))
}

fn present_provider(guid: GUID, event_ids: &[u16], shared: &Arc<FrameShared>) -> Provider {
    let shared = Arc::clone(shared);
    Provider::by_guid(guid)
        .level(LEVEL)
        .any(KEYWORD)
        .all(KEYWORD)
        .trace_flags(TraceFlags::EVENT_ENABLE_PROPERTY_IGNORE_KEYWORD_0)
        .add_filter(EventFilter::ByEventIds(event_ids.to_vec()))
        .add_callback(move |record: &EventRecord, _: &SchemaLocator| {
            // The consumer gets FILETIME ticks (100 ns): the same clock as `now_ns`.
            shared.on_present(record.process_id(), record.raw_timestamp() as u64 * 100);
        })
        .build()
}

/// `ControlTraceW` by session name, for a session this process holds no handle to.
fn control_session(code: EVENT_TRACE_CONTROL) -> Result<(), WIN32_ERROR> {
    #[repr(C)]
    struct Properties {
        props: EVENT_TRACE_PROPERTIES,
        logger_name: [u16; 1024],
    }
    let mut p = Properties { props: EVENT_TRACE_PROPERTIES::default(), logger_name: [0; 1024] };
    p.props.Wnode.BufferSize = size_of::<Properties>() as u32;
    p.props.LoggerNameOffset = offset_of!(Properties, logger_name) as u32;
    let name: Vec<u16> = SESSION_NAME.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `name` is NUL-terminated and outlives the call; the pointer covers the
    // whole `Properties` block, whose size is in `Wnode.BufferSize`, so ControlTraceW
    // may write the logger name back at `LoggerNameOffset`.
    let err = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(name.as_ptr()),
            (&raw mut p).cast::<EVENT_TRACE_PROPERTIES>(),
            code,
        )
    };
    if err == ERROR_SUCCESS { Ok(()) } else { Err(err) }
}

fn is_already_exists(e: &TraceError) -> bool {
    matches!(e, TraceError::EtwNativeError(EvntraceNativeError::AlreadyExist))
}

fn capture_error(e: TraceError) -> CaptureError {
    match &e {
        TraceError::EtwNativeError(EvntraceNativeError::IoError(io))
            if io.raw_os_error().is_some_and(is_access_denied) =>
        {
            CaptureError::AccessDenied
        }
        _ => CaptureError::Other(format!("{e:?}")),
    }
}

/// ferrisetw reports Win32 failures as HRESULTs (0x80070005) wrapped in an OS
/// error; the plain Win32 code is accepted too.
fn is_access_denied(code: i32) -> bool {
    code == ERROR_ACCESS_DENIED.0 as i32 || code == ERROR_ACCESS_DENIED.to_hresult().0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_routes_only_target_presents_to_tracker() {
        let s = FrameShared::new();
        s.set_target(Some(2));
        for i in 0..10u64 { s.on_present(1, i * 16_666_666); s.on_present(2, i * 16_666_666); }
        assert_eq!(s.tracker.lock().unwrap().len(), 10);
        assert!(s.observer.lock().unwrap().continuous_ms(1, 9 * 16_666_666) > 0);
        s.set_target(Some(1));
        assert_eq!(s.tracker.lock().unwrap().len(), 0);
    }

    #[test]
    fn access_denied_is_recognized_as_win32_code_and_hresult() {
        for code in [5, 0x8007_0005_u32 as i32] {
            let e = TraceError::EtwNativeError(EvntraceNativeError::IoError(
                std::io::Error::from_raw_os_error(code),
            ));
            assert!(matches!(capture_error(e), CaptureError::AccessDenied), "{code:#x}");
        }
        let other = TraceError::EtwNativeError(EvntraceNativeError::InvalidHandle);
        assert!(matches!(capture_error(other), CaptureError::Other(_)));
    }
}
