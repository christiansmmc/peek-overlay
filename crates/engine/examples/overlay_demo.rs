//! Shows the overlay with sample values that change every 500 ms, like the prototype's tick().
//! Usage: overlay_demo [--style line|card] [--pos tl|tc|tr|ml|mr|bl|bc|br] [--size 100]
//!                     [--opacity 80] [--seconds N]
//! Ctrl+Shift+O toggles the overlay.
use overlay_engine::hotkey;
use overlay_engine::model::MetricsSample;
use overlay_engine::native::{NativeCmd, NativeEvent, NativeHandle};
use overlay_engine::overlay::window::{OverlayCmd, OverlayConfig};
use overlay_engine::settings::{Metrics, Pos, Style};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

const TICK: Duration = Duration::from_millis(500);
/// Columns the controller sends for the frametime graph.
const GRAPH_COLUMNS: usize = 126;
/// The prototype's graph Y axis goes from 0 to 14 ms.
const GRAPH_MAX_MS: f32 = 14.0;

/// xorshift64*: enough randomness for jittering sample values.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Sample values evolving like the prototype's `tick()`.
struct Sim {
    rng: Rng,
    fps: u32,
    ft: Vec<f32>,
    gpu_use: u8,
    gpu_temp: u8,
    cpu_use: u8,
    cpu_temp: u8,
}

impl Sim {
    fn new() -> Sim {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        let mut rng = Rng(seed);
        let ft = (0..GRAPH_COLUMNS).map(|_| 6.6 + rng.next() * 0.8).collect();
        Sim { rng, fps: 142, ft, gpu_use: 96, gpu_temp: 71, cpu_use: 38, cpu_temp: 64 }
    }

    fn jitter(&mut self, v: u8, d: f32, lo: u8, hi: u8) -> u8 {
        (f32::from(v) + (self.rng.next() - 0.5) * d).round().clamp(f32::from(lo), f32::from(hi)) as u8
    }

    fn tick(&mut self) {
        let spike = self.rng.next() < 0.05;
        let r = self.rng.next();
        self.fps = (if spike { 96.0 + r * 20.0 } else { 138.0 + r * 9.0 }).round() as u32;
        let ms = 1000.0 / self.fps as f32 + (self.rng.next() - 0.5) * 0.4;
        self.ft.remove(0);
        self.ft.push(ms);
        self.gpu_use = self.jitter(self.gpu_use, 4.0, 88, 99);
        self.gpu_temp = self.jitter(self.gpu_temp, 2.0, 68, 75);
        self.cpu_use = self.jitter(self.cpu_use, 6.0, 28, 52);
        self.cpu_temp = self.jitter(self.cpu_temp, 2.0, 60, 69);
    }

    fn sample(&self) -> MetricsSample {
        MetricsSample {
            fps: Some(self.fps),
            frametime_ms: self.ft.last().copied(),
            graph: self.ft.clone(),
            graph_max_ms: GRAPH_MAX_MS,
            gpu_use: Some(self.gpu_use),
            gpu_temp: Some(self.gpu_temp),
            cpu_use: Some(self.cpu_use),
            cpu_temp: Some(self.cpu_temp),
        }
    }
}

fn main() {
    // Examples have no manifest: opt into per-monitor DPI awareness here; failure is harmless.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let number = |flag: &str, default: u32| value(flag).and_then(|v| v.parse().ok()).unwrap_or(default);
    let style = match value("--style").as_deref() {
        Some("line") => Style::Line,
        _ => Style::Card,
    };
    let pos: Pos = value("--pos").and_then(|p| serde_json::from_value(serde_json::json!(p)).ok()).unwrap_or(Pos::Tr);
    let seconds = number("--seconds", 10);
    let config = OverlayConfig {
        style,
        pos,
        size: number("--size", 100),
        opacity: number("--opacity", 80),
        metrics: Metrics::default(),
    };

    let (tx, rx) = mpsc::channel();
    let native = NativeHandle::spawn(Box::new(move |e| match e {
        NativeEvent::Hotkey => {
            let _ = tx.send(());
        }
        // The overlay already placed itself again; just make the event visible.
        NativeEvent::DisplayChanged => println!("display changed"),
        NativeEvent::Timer(_) => {}
    }));
    let combo: Vec<String> = ["Ctrl", "Shift", "O"].iter().map(|s| s.to_string()).collect();
    if native.register_hotkey(hotkey::parse(&combo).expect("valid combo")).is_err() {
        eprintln!("Ctrl+Shift+O is in use by another app; toggling disabled");
    }
    println!("overlay_demo: {config:?} for {seconds} s");

    let mut sim = Sim::new();
    native.send(NativeCmd::Overlay(OverlayCmd::Configure(config)));
    native.send(NativeCmd::Overlay(OverlayCmd::Data(sim.sample())));
    native.send(NativeCmd::Overlay(OverlayCmd::Show(true)));
    let mut visible = true;

    let end = Instant::now() + Duration::from_secs(u64::from(seconds));
    let mut next_tick = Instant::now() + TICK;
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        match rx.recv_timeout(next_tick.min(end) - now) {
            Ok(()) => {
                visible = !visible;
                native.send(NativeCmd::Overlay(OverlayCmd::Show(visible)));
                println!("visible={visible}");
            }
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= next_tick {
                    next_tick += TICK;
                    sim.tick();
                    native.send(NativeCmd::Overlay(OverlayCmd::Data(sim.sample())));
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    native.quit();
}
