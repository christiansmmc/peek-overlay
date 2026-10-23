pub mod autostart;
pub mod controller;
pub mod effects;
pub mod frames;
pub mod game;
pub mod game_rules;
pub mod hotkey;
pub mod model;
pub mod native;
pub mod overlay;
pub mod pawnio;
pub mod process_info;
pub mod sensors;
pub mod settings;

/// Version of the engine crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Debug-only diagnostics: `eprintln!` in debug builds. In release the arguments are still
/// type-checked (so a variable used only here is not "unused") but never evaluated or printed.
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { eprintln!($($arg)*) };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if false {
            let _ = format_args!($($arg)*);
        }
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() { assert!(!super::version().is_empty()); }
}
