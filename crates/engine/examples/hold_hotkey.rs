//! Holds a global hotkey for N seconds, to simulate "hotkey in use by another app".
//! Usage: hold_hotkey Ctrl+Alt+Shift+F9 --seconds 2
use overlay_engine::hotkey;
use overlay_engine::native::NativeHandle;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let combo_arg = args.first().expect("usage: hold_hotkey <Ctrl+Alt+Shift+F9> [--seconds N]");
    let seconds = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5);
    let combo: Vec<String> = combo_arg.split('+').map(|s| s.to_string()).collect();
    let hk = hotkey::parse(&combo).expect("invalid hotkey combo");
    let handle = NativeHandle::spawn(Box::new(|_| {}));
    handle.register_hotkey(hk).expect("hotkey already in use");
    println!("held");
    std::thread::sleep(Duration::from_secs(seconds));
    handle.quit();
}
