//! Dedicated native thread: a message-only window that owns the global hotkey and the timers,
//! the foreground WinEvent hook and the location hook on the watched window, plus the overlay
//! window (created on the first `NativeCmd::Overlay`).
//! Later tasks extend `NativeCmd`/`NativeEvent` and the single dispatch in `handle_cmd`.

pub mod foreground;
pub mod process_watch;

use crate::hotkey::{Hotkey, HotkeyError};
use crate::overlay::window::{OverlayCmd, OverlayWindow, WM_OVERLAY_DISPLAY_CHANGED};
use foreground::ForegroundInfo;
use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE, KillTimer,
    MSG, OBJID_WINDOW, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP, WM_DESTROY, WM_HOTKEY, WM_TIMER, WNDCLASSW,
};
use windows::core::w;

const WM_CMD: u32 = WM_APP + 1;
const HOTKEY_ID: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerKind {
    Sample,
    Graph,
    Detect,
}

impl TimerKind {
    const ALL: [TimerKind; 3] = [TimerKind::Sample, TimerKind::Graph, TimerKind::Detect];

    fn id(self) -> usize {
        match self {
            TimerKind::Sample => 1,
            TimerKind::Graph => 2,
            TimerKind::Detect => 3,
        }
    }

    fn period_ms(self) -> u32 {
        match self {
            TimerKind::Sample => 500,
            TimerKind::Graph => 66,
            TimerKind::Detect => 250,
        }
    }

    fn from_id(id: usize) -> Option<TimerKind> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timers {
    pub sample: bool,
    pub graph: bool,
    pub detect: bool,
}

impl Timers {
    fn is_on(&self, kind: TimerKind) -> bool {
        match kind {
            TimerKind::Sample => self.sample,
            TimerKind::Graph => self.graph,
            TimerKind::Detect => self.detect,
        }
    }
}

pub enum NativeEvent {
    Hotkey,
    Timer(TimerKind),
    /// Display resolution, layout or DPI changed (seen by the overlay window).
    DisplayChanged,
    /// A window of another process came to the foreground. Also sent at startup and after a
    /// display change for the window already in the foreground, and when the window watched by
    /// `WatchLocation` starts or stops covering its monitor.
    Foreground(ForegroundInfo),
}

pub enum NativeCmd {
    RegisterHotkey(Hotkey, Sender<Result<(), HotkeyError>>),
    UnregisterHotkey,
    SetTimers(Timers),
    Overlay(OverlayCmd),
    /// Watches the size of window `hwnd` of process `pid` (`None` stops), so a window that goes
    /// fullscreen after taking the focus is reported without a new focus change.
    WatchLocation(Option<(u32, isize)>),
    Quit,
}

/// Location hook on the watched window, with its last known fullscreen state.
struct LocationWatch {
    hook: HWINEVENTHOOK,
    hwnd: isize,
    covers: bool,
}

/// State owned by the native thread, reachable from the window procedure.
struct ThreadState {
    on_event: Box<dyn Fn(NativeEvent) + Send + 'static>,
    rx: Receiver<NativeCmd>,
    timers: Timers,
    hotkey_registered: bool,
    overlay: Option<OverlayWindow>,
    location: Option<LocationWatch>,
}

thread_local! {
    static STATE: RefCell<Option<ThreadState>> = const { RefCell::new(None) };
}

pub struct NativeHandle {
    tx: Sender<NativeCmd>,
    hwnd: isize,
    thread: JoinHandle<()>,
}

impl NativeHandle {
    pub fn spawn(on_event: Box<dyn Fn(NativeEvent) + Send + 'static>) -> NativeHandle {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<isize>();
        let thread = std::thread::Builder::new()
            .name("overlay-native".into())
            .spawn(move || run_thread(on_event, rx, ready_tx))
            .expect("failed to spawn native thread");
        let hwnd = ready_rx.recv().expect("native thread failed to create its window");
        NativeHandle { tx, hwnd, thread }
    }

    pub fn send(&self, cmd: NativeCmd) {
        if self.tx.send(cmd).is_ok() {
            // Wake the thread; if the window is already gone the post just fails.
            unsafe {
                let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_CMD, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Registers `hk` as the global hotkey, replacing any previous one of this handle.
    /// Blocks until the native thread answers; a failing `RegisterHotKey` becomes `InUse`.
    pub fn register_hotkey(&self, hk: Hotkey) -> Result<(), HotkeyError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send(NativeCmd::RegisterHotkey(hk, reply_tx));
        reply_rx.recv().unwrap_or(Err(HotkeyError::InUse))
    }

    pub fn quit(self) {
        self.send(NativeCmd::Quit);
        let _ = self.thread.join();
    }
}

fn run_thread(
    on_event: Box<dyn Fn(NativeEvent) + Send + 'static>,
    rx: Receiver<NativeCmd>,
    ready_tx: Sender<isize>,
) {
    let hwnd = match create_window() {
        Some(h) => h,
        None => return, // dropping ready_tx makes spawn() panic with a clear message
    };
    STATE.with(|s| {
        *s.borrow_mut() = Some(ThreadState {
            on_event,
            rx,
            timers: Timers::default(),
            hotkey_registered: false,
            overlay: None,
            location: None,
        });
    });
    let _ = ready_tx.send(hwnd.0 as isize);

    // Out-of-context: the callback runs on this thread, from the message loop below.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_invalid() {
        crate::trace!("native: SetWinEventHook(EVENT_SYSTEM_FOREGROUND) failed");
    }
    // The hook only reports changes; start from the window that already has the focus.
    emit_current_foreground();

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hook.is_invalid() {
            let _ = UnhookWinEvent(hook);
        }
    }
    watch_location(None);
    // Drop the state (and the overlay window) with the borrow already released.
    let state = STATE.with(|s| s.borrow_mut().take());
    drop(state);
}

fn create_window() -> Option<HWND> {
    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?;
        let class_name = w!("OverlayEngineNativeWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        // Registering twice (second NativeHandle) fails with "class exists"; that is fine.
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("overlay-native"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance.into()),
            None,
        )
        .ok()
    }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
        return;
    }
    if let Some(info) = foreground::describe(hwnd.0 as isize) {
        emit(NativeEvent::Foreground(info));
    }
}

/// Reports the current foreground window, unless it is one of ours (the hook skips those too).
fn emit_current_foreground() {
    let current = unsafe { GetForegroundWindow() };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(current, Some(&mut pid)) };
    if pid == unsafe { GetCurrentProcessId() } {
        return;
    }
    if let Some(info) = foreground::describe(current.0 as isize) {
        emit(NativeEvent::Foreground(info));
    }
}

/// Replaces the location hook. The new hook is scoped to `pid`, so only that process's events
/// wake this thread, and they are dropped unless they move the watched window.
fn watch_location(target: Option<(u32, isize)>) {
    let old = STATE.with(|s| s.borrow_mut().as_mut().and_then(|st| st.location.take()));
    if let Some(old) = old {
        unsafe {
            let _ = UnhookWinEvent(old.hook);
        }
    }
    let Some((pid, hwnd)) = target else { return };
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(on_location_event),
            pid,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_invalid() {
        crate::trace!("native: SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE, pid {pid}) failed");
        return;
    }
    crate::trace!("native: watching the location of pid={pid}");
    // Only windows that did not cover their monitor are watched; it may have changed already.
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.location = Some(LocationWatch { hook, hwnd, covers: false });
        }
    });
    check_location(hwnd);
}

unsafe extern "system" fn on_location_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_OBJECT_LOCATIONCHANGE || id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
        return;
    }
    check_location(hwnd.0 as isize);
}

/// Emits `Foreground` for the watched window when it starts or stops covering its monitor.
fn check_location(hwnd: isize) {
    let watched = STATE.with(|s| s.borrow().as_ref().and_then(|st| st.location.as_ref().map(|l| (l.hwnd, l.covers))));
    let Some((watched, covers)) = watched else { return };
    if hwnd != watched || foreground::covers_its_monitor(hwnd) == covers {
        return;
    }
    STATE.with(|s| {
        if let Some(l) = s.borrow_mut().as_mut().and_then(|st| st.location.as_mut()) {
            l.covers = !covers;
        }
    });
    if let Some(info) = foreground::describe(hwnd) {
        emit(NativeEvent::Foreground(info));
    }
}

fn emit(event: NativeEvent) {
    STATE.with(|s| {
        if let Some(st) = s.borrow().as_ref() {
            (st.on_event)(event);
        }
    });
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CMD => {
            drain_commands(hwnd);
            LRESULT(0)
        }
        WM_HOTKEY => {
            emit(NativeEvent::Hotkey);
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(kind) = TimerKind::from_id(wparam.0) {
                emit(NativeEvent::Timer(kind));
            }
            LRESULT(0)
        }
        // Forwarded by the overlay window: a message-only window never gets WM_DISPLAYCHANGE.
        WM_OVERLAY_DISPLAY_CHANGED => {
            with_overlay(hwnd, false, OverlayWindow::on_display_changed);
            emit(NativeEvent::DisplayChanged);
            // An exclusive-fullscreen mode switch changes the display without a focus change.
            emit_current_foreground();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn drain_commands(hwnd: HWND) {
    loop {
        // Take one command with the borrow released before handling it.
        let cmd = STATE.with(|s| s.borrow().as_ref().and_then(|st| st.rx.try_recv().ok()));
        match cmd {
            Some(cmd) => handle_cmd(hwnd, cmd),
            None => break,
        }
    }
}

/// Single dispatch point for commands; later tasks add their variants here.
fn handle_cmd(hwnd: HWND, cmd: NativeCmd) {
    match cmd {
        NativeCmd::RegisterHotkey(hk, reply) => {
            let result = register_hotkey(hwnd, hk);
            let _ = reply.send(result);
        }
        NativeCmd::UnregisterHotkey => unregister_hotkey(hwnd),
        NativeCmd::SetTimers(wanted) => set_timers(hwnd, wanted),
        NativeCmd::Overlay(cmd) => with_overlay(hwnd, true, |o| o.handle(cmd)),
        NativeCmd::WatchLocation(target) => watch_location(target),
        NativeCmd::Quit => unsafe {
            // WM_DESTROY ends the loop; timers and hotkey die with the window.
            let _ = DestroyWindow(hwnd);
        },
    }
}

/// Runs `f` on the overlay window, creating it first when `create` is set. The window is
/// taken out of the state while `f` runs, so messages it triggers never meet a borrowed state.
fn with_overlay(hwnd: HWND, create: bool, f: impl FnOnce(&mut OverlayWindow)) {
    let taken = STATE.with(|s| s.borrow_mut().as_mut().and_then(|st| st.overlay.take()));
    let mut overlay = match taken {
        Some(o) => o,
        None if create => match OverlayWindow::create() {
            Ok(o) => {
                o.set_display_listener(hwnd.0 as isize);
                o
            }
            Err(e) => {
                crate::trace!("overlay: window creation failed: {e}");
                return;
            }
        },
        None => return,
    };
    f(&mut overlay);
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.overlay = Some(overlay);
        }
    });
}

fn register_hotkey(hwnd: HWND, hk: Hotkey) -> Result<(), HotkeyError> {
    unregister_hotkey(hwnd);
    let ok = unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, HOT_KEY_MODIFIERS(hk.mods), hk.vk) }.is_ok();
    if ok {
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                st.hotkey_registered = true;
            }
        });
        Ok(())
    } else {
        Err(HotkeyError::InUse)
    }
}

fn unregister_hotkey(hwnd: HWND) {
    let was = STATE.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|st| std::mem::replace(&mut st.hotkey_registered, false))
            .unwrap_or(false)
    });
    if was {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
        }
    }
}

fn set_timers(hwnd: HWND, wanted: Timers) {
    let current = STATE.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|st| std::mem::replace(&mut st.timers, wanted))
            .unwrap_or_default()
    });
    for kind in TimerKind::ALL {
        match (current.is_on(kind), wanted.is_on(kind)) {
            (false, true) => unsafe {
                SetTimer(Some(hwnd), kind.id(), kind.period_ms(), None);
            },
            (true, false) => unsafe {
                let _ = KillTimer(Some(hwnd), kind.id());
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_registration_of_same_combo_reports_in_use() {
        let a = NativeHandle::spawn(Box::new(|_| {}));
        let b = NativeHandle::spawn(Box::new(|_| {}));
        let combo: Vec<String> = ["Ctrl", "Alt", "Shift", "F11"].iter().map(|x| x.to_string()).collect();
        let hk = crate::hotkey::parse(&combo).unwrap();
        assert_eq!(a.register_hotkey(hk), Ok(()));
        assert_eq!(b.register_hotkey(hk), Err(HotkeyError::InUse));
        a.quit();
        b.quit();
    }

    #[test]
    fn timers_fire_only_when_enabled() {
        use std::{sync::mpsc, time::Duration};
        let (tx, rx) = mpsc::channel();
        let h = NativeHandle::spawn(Box::new(move |e| {
            if let NativeEvent::Timer(k) = e {
                let _ = tx.send(k);
            }
        }));
        assert!(rx.recv_timeout(Duration::from_millis(700)).is_err());
        h.send(NativeCmd::SetTimers(Timers { sample: true, ..Timers::default() }));
        assert_eq!(rx.recv_timeout(Duration::from_millis(1200)), Ok(TimerKind::Sample));
        h.send(NativeCmd::SetTimers(Timers::default()));
        std::thread::sleep(Duration::from_millis(100));
        while rx.try_recv().is_ok() {}
        assert!(rx.recv_timeout(Duration::from_millis(700)).is_err());
        h.quit();
    }

    /// The overlay window created by `h` (its display listener is `h`'s window).
    fn overlay_of(h: &NativeHandle) -> Option<HWND> {
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GWLP_USERDATA, GetWindowLongPtrW};
        let mut after = None;
        loop {
            let found = unsafe { FindWindowExW(None, after, w!("OverlayEngineOverlayWindow"), None) }.ok()?;
            if unsafe { GetWindowLongPtrW(found, GWLP_USERDATA) } == h.hwnd {
                return Some(found);
            }
            after = Some(found);
        }
    }

    #[test]
    fn overlay_window_display_and_dpi_changes_emit_events() {
        use crate::overlay::window::OverlayConfig;
        use crate::settings::{Metrics, Pos, Style};
        use std::{sync::mpsc, time::Duration};
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_DISPLAYCHANGE, WM_DPICHANGED};
        let (tx, rx) = mpsc::channel();
        let h = NativeHandle::spawn(Box::new(move |e| {
            if let NativeEvent::DisplayChanged = e {
                let _ = tx.send(());
            }
        }));
        let cfg = OverlayConfig { style: Style::Line, pos: Pos::Tc, size: 100, opacity: 80, metrics: Metrics::default() };
        h.send(NativeCmd::Overlay(OverlayCmd::Configure(cfg)));
        let mut overlay = None;
        for _ in 0..50 {
            overlay = overlay_of(&h);
            if overlay.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let overlay = overlay.expect("overlay window not created");
        // WM_DPICHANGED carries the suggested window rect; the system rejects a null one.
        let suggested = windows::Win32::Foundation::RECT { left: 0, top: 0, right: 10, bottom: 10 };
        let dpi = LPARAM(&suggested as *const _ as isize);
        for (msg, wparam, lparam) in [(WM_DISPLAYCHANGE, WPARAM(32), LPARAM(0)), (WM_DPICHANGED, WPARAM(96 << 16 | 96), dpi)] {
            unsafe { SendMessageW(overlay, msg, Some(wparam), Some(lparam)) };
            assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(()), "message {msg:#x}");
        }
        h.quit();
        assert!(window_gone(overlay));
    }

    #[test]
    fn location_watch_reports_when_the_window_starts_covering_its_monitor() {
        use std::{sync::mpsc, time::Duration};
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
        use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WS_POPUP};
        let hwnd = unsafe {
            CreateWindowExW(Default::default(), w!("STATIC"), w!("Jogo Teste"), WS_POPUP, 0, 0, 10, 10, None, None, None, None)
        }
        .unwrap();
        let watched = hwnd.0 as isize;
        let (tx, rx) = mpsc::channel();
        let h = NativeHandle::spawn(Box::new(move |e| {
            // The startup report of the real foreground window is someone else's.
            if let NativeEvent::Foreground(info) = e
                && info.hwnd == watched
            {
                let _ = tx.send(info.covers_monitor);
            }
        }));
        h.send(NativeCmd::WatchLocation(Some((std::process::id(), watched))));
        let place = |x: i32, y: i32, w: i32, cy: i32| unsafe {
            SetWindowPos(hwnd, None, x, y, w, cy, SWP_NOZORDER | SWP_NOACTIVATE).unwrap();
        };
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        assert!(unsafe { GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) }.as_bool());
        let m = mi.rcMonitor;

        place(5, 5, 20, 20);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "a move that stays windowed is not reported");
        place(m.left, m.top, m.right - m.left, m.bottom - m.top);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(true));
        place(0, 0, 10, 10);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(false));

        h.send(NativeCmd::WatchLocation(None));
        std::thread::sleep(Duration::from_millis(100));
        place(m.left, m.top, m.right - m.left, m.bottom - m.top);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "unwatched windows are not reported");
        h.quit();
        unsafe { DestroyWindow(hwnd).unwrap() };
    }

    fn window_gone(hwnd: HWND) -> bool {
        !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)) }.as_bool()
    }
}
