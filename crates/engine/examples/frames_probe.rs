//! Prints one process's frame rate as seen through the ETW capture (needs elevation).
//! Usage: frames_probe --pid N [--seconds N] [--crash-after N]
//!   --crash-after N  exits with code 1 after N seconds without stopping the session,
//!                    leaving a stale "PerfOverlay-Frames" session behind
use overlay_engine::frames::clock::now_ns;
use overlay_engine::frames::etw::{CaptureError, FrameCapture, FrameShared};
use std::time::{Duration, Instant};

const REPORT_EVERY: Duration = Duration::from_millis(500);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let pid = value("--pid").expect("usage: frames_probe --pid N [--seconds N] [--crash-after N]")
        as u32;
    let run_for = Duration::from_secs(value("--seconds").unwrap_or(10));
    let crash_after = value("--crash-after").map(Duration::from_secs);

    let shared = FrameShared::new();
    shared.set_target(Some(pid));
    let capture = match FrameCapture::start(shared.clone()) {
        Ok(capture) => capture,
        Err(CaptureError::AccessDenied) => {
            println!("error=AccessDenied");
            return;
        }
        Err(e) => {
            println!("error={e:?}");
            std::process::exit(1);
        }
    };

    let start = Instant::now();
    let mut next_report = start + REPORT_EVERY;
    let mut skew_printed = false;
    while start.elapsed() < run_for {
        std::thread::sleep(Duration::from_millis(10));
        if crash_after.is_some_and(|after| start.elapsed() >= after) {
            println!("crash-after: exiting without stopping the session");
            std::process::exit(1);
        }
        let now = now_ns();
        let report = Instant::now() >= next_report;
        let (last, fps, ft) = {
            let tracker = shared.tracker.lock().unwrap();
            (tracker.last_present_ns(), tracker.fps(now), tracker.avg_frametime_ms(now))
        };
        if !skew_printed && let Some(t) = last {
            println!("clock_skew_ms={:.1}", (now as i128 - t as i128) as f64 / 1e6);
            skew_printed = true;
        }
        if report {
            next_report += REPORT_EVERY;
            let fps = fps.map_or("-".to_string(), |v| v.to_string());
            let ft = ft.map_or("-".to_string(), |v| format!("{v:.2}"));
            println!("fps={fps} ft={ft}");
        }
    }
    capture.stop();
}
