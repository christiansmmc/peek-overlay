//! Fake game for exercising frame capture and game detection: a "Fake Game" window
//! that presents through a flip-model DXGI swap chain, clearing to a shifting color.
//! This example is the only code in the project that creates a D3D device.
//! Usage: fake_game [--windowed] [--fps N] [--seconds N]
//!   --windowed   800x600 window (default: borderless popup covering the primary monitor)
//!   --fps N      0 = vsync with Present(1, 0) (default); otherwise Present(0, 0) paced to N fps
//!   --seconds N  how long to run (default 60); Esc or closing the window also quits
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GetSystemMetrics, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW,
    PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, SetForegroundWindow,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_DESTROY, WM_KEYDOWN, WM_QUIT, WNDCLASSW,
    WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::core::{Error, Interface, Result, w};

struct Args {
    windowed: bool,
    fps: u32,
    seconds: u64,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    Args {
        windowed: args.iter().any(|a| a == "--windowed"),
        fps: value("--fps").unwrap_or(0) as u32,
        seconds: value("--seconds").unwrap_or(60),
    }
}

fn main() -> Result<()> {
    let args = parse_args();
    println!("pid={}", std::process::id());
    unsafe {
        // Physical pixels, so the popup covers the monitor at any display scaling.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hwnd = create_window(args.windowed)?;
        let (device, context) = create_device()?;
        let swap_chain = create_swap_chain(&device, hwnd)?;
        // With D3D11, buffer 0 always refers to the current back buffer.
        let back_buffer: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
        let mut rtv = None;
        device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv))?;
        let rtv = rtv.ok_or_else(Error::empty)?;
        let result = render_loop(&args, &context, &swap_chain, &rtv);
        let _ = DestroyWindow(hwnd);
        result
    }
}

unsafe fn render_loop(
    args: &Args,
    context: &ID3D11DeviceContext,
    swap_chain: &IDXGISwapChain1,
    rtv: &ID3D11RenderTargetView,
) -> Result<()> {
    let start = Instant::now();
    let end = start + Duration::from_secs(args.seconds);
    let period = (args.fps > 0).then(|| Duration::from_secs_f64(1.0 / args.fps as f64));
    let mut next = start;
    let mut frame = 0u64;
    while Instant::now() < end && unsafe { pump_messages() } {
        unsafe {
            context.ClearRenderTargetView(rtv, &frame_color(frame));
            match period {
                None => swap_chain.Present(1, DXGI_PRESENT(0)).ok()?,
                Some(_) => swap_chain.Present(0, DXGI_PRESENT(0)).ok()?,
            }
        }
        if let Some(period) = period {
            // Absolute deadlines keep the average rate exact despite sleep jitter.
            next += period;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > period {
                next = now; // fell behind (window dragged, etc.): don't burst to catch up
            }
        }
        frame += 1;
    }
    Ok(())
}

fn frame_color(frame: u64) -> [f32; 4] {
    let t = frame as f32 * 0.02;
    let channel = |phase: f32| 0.5 + 0.5 * (t + phase).sin();
    [channel(0.0), channel(2.1), channel(4.2), 1.0]
}

/// Dispatches pending messages; false once the window is gone.
unsafe fn pump_messages() -> bool {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

unsafe fn create_window(windowed: bool) -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("FakeGameWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let (style, x, y, width, height) = if windowed {
            let mut r = RECT { left: 0, top: 0, right: 800, bottom: 600 };
            AdjustWindowRect(&mut r, WS_OVERLAPPEDWINDOW, false)?;
            (WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top)
        } else {
            // The primary monitor always starts at (0, 0).
            (WS_POPUP, 0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        };
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Fake Game"),
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        Ok(hwnd)
    }
}

unsafe fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((device.ok_or_else(Error::empty)?, context.ok_or_else(Error::empty)?))
}

unsafe fn create_swap_chain(device: &ID3D11Device, hwnd: HWND) -> Result<IDXGISwapChain1> {
    let desc = DXGI_SWAP_CHAIN_DESC1 {
        // Width/Height 0: take the window's client size.
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        ..Default::default()
    };
    unsafe {
        let factory: IDXGIFactory2 = device.cast::<IDXGIDevice>()?.GetAdapter()?.GetParent()?;
        factory.CreateSwapChainForHwnd(device, hwnd, &desc, None, None)
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
