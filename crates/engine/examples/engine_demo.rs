//! Runs the whole engine (the `Controller`) without the Tauri shell and logs every UI event.
//! Usage: engine_demo [--seconds N] [--settings <path>] [--log <path>]
//!   --seconds N        how long to run before `shutdown()` (default 30)
//!   --settings <path>  settings file (default: %APPDATA%\Peek\settings.json)
//!   --log <path>       one JSON line {"event":..,"payload":..} per event (default: stdout)
//! The saved shortcut (Ctrl+Shift+O by default) toggles the overlay; FPS needs elevation.
use overlay_engine::controller::{Controller, UiSink};
use overlay_engine::settings;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

/// Writes each event as one JSON line and flushes it, so the log can be read while it runs.
struct LogSink(Mutex<Box<dyn Write + Send>>);

impl UiSink for LogSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let line = serde_json::json!({ "event": event, "payload": payload });
        let mut out = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

fn main() {
    // Examples have no manifest: opt into per-monitor DPI awareness here; failure is harmless.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let seconds = value("--seconds").and_then(|v| v.parse::<u64>().ok()).unwrap_or(30);
    let settings_path = value("--settings").map(PathBuf::from).unwrap_or_else(settings::settings_path);
    let out: Box<dyn Write + Send> = match value("--log") {
        Some(path) => Box::new(std::fs::File::create(&path).unwrap_or_else(|e| panic!("cannot create {path}: {e}"))),
        None => Box::new(std::io::stdout()),
    };

    let (mut loaded, outcome) = settings::load(&settings_path);
    // Never let the demo (possibly elevated) rewrite the "Peek" scheduled task to point at engine_demo.exe.
    loaded.general.startup = false;
    println!("engine_demo: settings {outcome:?} from {} for {seconds} s", settings_path.display());
    let controller = Controller::new(loaded, settings_path, Arc::new(LogSink(Mutex::new(out))));
    controller.start();
    println!("state={}", serde_json::to_string(&controller.state()).unwrap_or_default());

    std::thread::sleep(Duration::from_secs(seconds));
    controller.shutdown();
    println!("engine_demo: done");
}
