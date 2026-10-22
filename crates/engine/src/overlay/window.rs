//! The overlay's top-level layered window: click-through, topmost, never activated.
//! Lives on the native thread, which forwards `OverlayCmd`s to `OverlayWindow::handle`.

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GWLP_USERDATA, GetForegroundWindow,
    GetWindowLongPtrW, HWND_TOPMOST, PostMessageW, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_APP,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, Result, w};

use super::layout::layout;
use super::position::{Rect, place};
use super::render::Renderer;
use crate::model::MetricsSample;
use crate::settings::{Metrics, Pos, Style};

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayConfig {
    pub style: Style,
    pub pos: Pos,
    pub size: u32,
    pub opacity: u32,
    pub metrics: Metrics,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayCmd {
    Configure(OverlayConfig),
    Data(MetricsSample),
    Show(bool),
    /// Window whose monitor hosts the overlay; `None` follows the foreground window.
    Anchor(Option<isize>),
    ReassertTopmost,
}

/// Posted to the display listener (`set_display_listener`) when the overlay window gets
/// `WM_DISPLAYCHANGE` or `WM_DPICHANGED`; the listener then calls `on_display_changed`.
pub const WM_OVERLAY_DISPLAY_CHANGED: u32 = WM_APP + 2;

pub struct OverlayWindow {
    hwnd: HWND,
    renderer: Renderer,
    config: Option<OverlayConfig>,
    sample: MetricsSample,
    anchor: Option<isize>,
    /// Monitor of the last drawn frame; an anchor on another monitor triggers a redraw.
    monitor: Option<HMONITOR>,
}

impl OverlayWindow {
    pub fn create() -> Result<Self> {
        let renderer = Renderer::new()?;
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let class_name = w!("OverlayEngineOverlayWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            // A second window registers again and gets "class exists"; that is fine.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name,
                w!("perf-overlay"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;
            Ok(OverlayWindow { hwnd, renderer, config: None, sample: MetricsSample::default(), anchor: None, monitor: None })
        }
    }

    pub fn ex_style(&self) -> u32 {
        unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32 }
    }

    /// Window that receives `WM_OVERLAY_DISPLAY_CHANGED` (0 = nobody).
    pub fn set_display_listener(&self, listener: isize) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, listener);
        }
    }

    pub fn handle(&mut self, cmd: OverlayCmd) {
        match cmd {
            OverlayCmd::Configure(config) => {
                self.config = Some(config);
                self.redraw();
            }
            OverlayCmd::Data(sample) => {
                self.sample = sample;
                self.redraw();
            }
            OverlayCmd::Show(visible) => unsafe {
                let _ = ShowWindow(self.hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
            },
            OverlayCmd::Anchor(anchor) => {
                self.anchor = anchor;
                if self.monitor != Some(self.current_monitor()) {
                    self.redraw();
                }
            }
            OverlayCmd::ReassertTopmost => unsafe {
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            },
        }
    }

    /// Resolution, layout or DPI changed: redraw at the new scale and place again.
    pub fn on_display_changed(&mut self) {
        self.redraw();
    }

    fn redraw(&mut self) {
        if let Err(_e) = self.try_redraw() {
            crate::trace!("overlay: redraw failed: {_e}");
        }
    }

    fn try_redraw(&mut self) -> Result<()> {
        let Some(cfg) = &self.config else { return Ok(()) };
        let monitor = self.current_monitor();
        let (rect, dpi_scale) = monitor_geometry(monitor)?;
        let scale = cfg.size as f32 / 100.0 * dpi_scale;
        let l = layout(cfg.style, &cfg.metrics, &self.sample, cfg.opacity, &self.renderer);
        let frame = self.renderer.render(&l, scale)?;
        let (x, y) = place(rect, cfg.pos, frame.w, frame.h, dpi_scale);
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&POINT { x, y }),
                Some(&SIZE { cx: frame.w, cy: frame.h }),
                Some(frame.dc()),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )?;
        }
        self.monitor = Some(monitor);
        Ok(())
    }

    /// Monitor of the anchor window, or of the foreground window without an anchor.
    fn current_monitor(&self) -> HMONITOR {
        unsafe {
            let target = match self.anchor {
                Some(h) => HWND(h as *mut _),
                None => GetForegroundWindow(),
            };
            let m = MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST);
            if m.is_invalid() { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) } else { m }
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// `rcMonitor` in physical pixels and the monitor's effective DPI scale (1.0 = 96 DPI).
fn monitor_geometry(monitor: HMONITOR) -> Result<(Rect, f32)> {
    unsafe {
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return Err(Error::from_thread());
        }
        let rc = mi.rcMonitor;
        let rect = Rect { x: rc.left, y: rc.top, w: rc.right - rc.left, h: rc.bottom - rc.top };
        let (mut dpi_x, mut dpi_y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        Ok((rect, dpi_x as f32 / 96.0))
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // Only top-level windows get these (the native message-only window never does).
        // Posting keeps this procedure away from the native thread's state, which may be
        // borrowed while it runs.
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            let listener = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if listener != 0 {
                unsafe {
                    let _ = PostMessageW(Some(HWND(listener as *mut _)), WM_OVERLAY_DISPLAY_CHANGED, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_window_is_click_through_and_never_activates() {
        let w = OverlayWindow::create().unwrap();
        let need = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE).0;
        assert_eq!(w.ex_style() & need, need);
    }

    fn window_rect(hwnd: HWND) -> Rect {
        let mut rc = windows::Win32::Foundation::RECT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rc).unwrap() };
        Rect { x: rc.left, y: rc.top, w: rc.right - rc.left, h: rc.bottom - rc.top }
    }

    #[test]
    fn configure_scales_frame_and_places_it_on_the_monitor() {
        let mut w = OverlayWindow::create().unwrap();
        let cfg = OverlayConfig { style: Style::Card, pos: Pos::Tr, size: 150, opacity: 80, metrics: Metrics::default() };
        w.handle(OverlayCmd::Configure(cfg.clone()));
        let (monitor, dpi_scale) = monitor_geometry(w.current_monitor()).unwrap();
        let r = window_rect(w.hwnd);
        assert_eq!(r.w, (148.0 * 1.5 * dpi_scale).ceil() as i32);
        assert_eq!((r.x, r.y), place(monitor, Pos::Tr, r.w, r.h, dpi_scale));
        w.handle(OverlayCmd::Configure(OverlayConfig { pos: Pos::Bl, ..cfg }));
        let r = window_rect(w.hwnd);
        assert_eq!((r.x, r.y), place(monitor, Pos::Bl, r.w, r.h, dpi_scale));
    }

    fn monitors() -> Vec<HMONITOR> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC};
        unsafe extern "system" fn push(m: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> windows::core::BOOL {
            unsafe { (*(list.0 as *mut Vec<HMONITOR>)).push(m) };
            true.into()
        }
        let mut list: Vec<HMONITOR> = Vec::new();
        unsafe { EnumDisplayMonitors(None, None, Some(push), LPARAM(&mut list as *mut _ as isize)) }.unwrap();
        list
    }

    #[test]
    fn anchor_moves_the_overlay_to_the_anchor_monitor() {
        // Includes monitors left of the primary (negative coordinates) when the machine has them.
        let mut w = OverlayWindow::create().unwrap();
        let cfg = OverlayConfig { style: Style::Line, pos: Pos::Tr, size: 100, opacity: 80, metrics: Metrics::default() };
        w.handle(OverlayCmd::Configure(cfg));
        for m in monitors() {
            let (rect, dpi_scale) = monitor_geometry(m).unwrap();
            let (cx, cy) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
            let anchor = unsafe {
                CreateWindowExW(Default::default(), w!("STATIC"), None, WS_POPUP, cx, cy, 10, 10, None, None, None, None)
            }
            .unwrap();
            w.handle(OverlayCmd::Anchor(Some(anchor.0 as isize)));
            let r = window_rect(w.hwnd);
            assert_eq!((r.x, r.y), place(rect, Pos::Tr, r.w, r.h, dpi_scale), "monitor {rect:?}");
            unsafe { DestroyWindow(anchor).unwrap() };
        }
    }
}
