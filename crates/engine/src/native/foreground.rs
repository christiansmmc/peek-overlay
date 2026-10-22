//! Foreground window description for game detection (fed by the native thread's WinEvent hook).

use crate::overlay::position::Rect;
use crate::process_info;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
};

#[derive(Clone, Debug)]
pub struct ForegroundInfo {
    pub hwnd: isize,
    pub pid: u32,
    /// File name of the process image (e.g. "witcher3.exe"); empty when the process can't be opened.
    pub exe_name: String,
    pub title: String,
    /// The window fills its monitor: fullscreen or borderless.
    pub covers_monitor: bool,
}

/// Slack, per edge, between a fullscreen window and its monitor.
const SLACK_PX: i32 = 1;

/// `true` when every edge of `window` is within 1 px of the same edge of `monitor`. A maximized
/// window with a caption doesn't count: it stops at the taskbar, or overshoots by its invisible
/// resize borders when the taskbar auto-hides.
pub fn covers_monitor(window: Rect, monitor: Rect) -> bool {
    let near = |a: i32, b: i32| (a - b).abs() <= SLACK_PX;
    near(window.x, monitor.x)
        && near(window.y, monitor.y)
        && near(window.x + window.w, monitor.x + monitor.w)
        && near(window.y + window.h, monitor.y + monitor.h)
}

fn rect(rc: RECT) -> Rect {
    Rect { x: rc.left, y: rc.top, w: rc.right - rc.left, h: rc.bottom - rc.top }
}

/// Owner process, executable, title and fullscreen state of a top-level window; `None` for a
/// null or destroyed window.
pub fn describe(hwnd: isize) -> Option<ForegroundInfo> {
    if hwnd == 0 || !is_window_alive(hwnd) {
        return None;
    }
    let h = HWND(hwnd as *mut _);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let exe_name = process_info::exe_path(pid)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Some(ForegroundInfo { hwnd, pid, exe_name, title: window_title(h), covers_monitor: fills_its_monitor(h) })
}

pub fn is_window_alive(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(HWND(hwnd as *mut _))) }.as_bool()
}

fn window_title(h: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(h) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn fills_its_monitor(h: HWND) -> bool {
    unsafe {
        let mut wr = RECT::default();
        if GetWindowRect(h, &mut wr).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }
        covers_monitor(rect(wr), rect(mi.rcMonitor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_monitor_allows_one_pixel_slack() {
        let m = Rect { x: 0, y: 0, w: 1920, h: 1080 };
        assert!(covers_monitor(Rect { x: -1, y: -1, w: 1922, h: 1082 }, m));
        assert!(covers_monitor(m, m));
        assert!(!covers_monitor(Rect { x: 0, y: 0, w: 1920, h: 1040 }, m));
    }

    #[test]
    fn maximized_window_with_invisible_borders_is_not_fullscreen() {
        let m = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        assert!(covers_monitor(Rect { x: -1920, y: 0, w: 1920, h: 1080 }, m));
        assert!(!covers_monitor(Rect { x: -1928, y: -8, w: 1936, h: 1096 }, m));
    }

    #[test]
    fn describe_reads_owner_title_and_size_of_a_live_window() {
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
        use windows::core::w;
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("Jogo Teste"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        let info = describe(hwnd.0 as isize).expect("live window");
        assert_eq!(info.pid, std::process::id());
        assert_eq!(info.title, "Jogo Teste");
        assert!(info.exe_name.to_ascii_lowercase().ends_with(".exe"), "{}", info.exe_name);
        assert!(!info.covers_monitor);
        unsafe { DestroyWindow(hwnd).unwrap() };
        assert!(!is_window_alive(hwnd.0 as isize));
        assert!(describe(hwnd.0 as isize).is_none());
        assert!(describe(0).is_none());
    }
}
