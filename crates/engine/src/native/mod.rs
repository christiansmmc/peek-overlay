//! Dedicated native thread: a message-only window that owns the global hotkey and the timers.
//! Later tasks extend `NativeCmd`/`NativeEvent` and the single dispatch in `handle_cmd`.

use crate::hotkey::{Hotkey, HotkeyError};
use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    KillTimer, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY, WM_DISPLAYCHANGE, WM_HOTKEY, WM_TIMER,
    WNDCLASSW,
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
    DisplayChanged,
}

pub enum NativeCmd {
    RegisterHotkey(Hotkey, Sender<Result<(), HotkeyError>>),
    UnregisterHotkey,
    SetTimers(Timers),
    Quit,
}

/// State owned by the native thread, reachable from the window procedure.
struct ThreadState {
    on_event: Box<dyn Fn(NativeEvent) + Send + 'static>,
    rx: Receiver<NativeCmd>,
    timers: Timers,
    hotkey_registered: bool,
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
        });
    });
    let _ = ready_tx.send(hwnd.0 as isize);

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    STATE.with(|s| *s.borrow_mut() = None);
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
        WM_DISPLAYCHANGE => {
            emit(NativeEvent::DisplayChanged);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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
        NativeCmd::Quit => unsafe {
            // WM_DESTROY ends the loop; timers and hotkey die with the window.
            let _ = DestroyWindow(hwnd);
        },
    }
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
}
