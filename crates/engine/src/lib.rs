/// Version of the engine crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Debug-only diagnostics: `eprintln!` in debug builds, expands to nothing in release.
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { eprintln!($($arg)*) };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{}};
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() { assert!(!super::version().is_empty()); }
}
