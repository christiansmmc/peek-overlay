//! Prints CPU/GPU usage and temperatures every 500 ms. CPU temperature needs PawnIO and elevation.
//! Usage: sensors_probe [--seconds N]
use overlay_engine::sensors::{Sensors, default_dedicated_luid};
use overlay_engine::settings::Metrics;
use std::time::{Duration, Instant};

const REPORT_EVERY: Duration = Duration::from_millis(500);

fn show(v: Option<u8>) -> String {
    v.map_or("—".to_string(), |v| v.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10);
    let run_for = Duration::from_secs(seconds);

    let mut sensors = Sensors::new();
    println!("pawnio_installed={}", overlay_engine::pawnio::is_installed());
    println!("default_luid={:?}", default_dedicated_luid().map(|l| format!("{l:#x}")));
    println!("availability={:?}", sensors.availability());

    let wanted =
        Metrics { fps: false, graph: false, gpu_use: true, gpu_temp: true, cpu_use: true, cpu_temp: true };
    let start = Instant::now();
    let mut next = start + REPORT_EVERY;
    while start.elapsed() < run_for {
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        next += REPORT_EVERY;
        let s = sensors.sample(&wanted, None);
        println!(
            "cpu={} gpu={} gpu_temp={} cpu_temp={}",
            show(s.cpu_use),
            show(s.gpu_use),
            show(s.gpu_temp),
            show(s.cpu_temp)
        );
    }
}
