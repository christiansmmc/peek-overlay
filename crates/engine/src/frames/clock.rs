use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;

/// Current time in nanoseconds on the same clock as ETW timestamps
/// (FILETIME, 100 ns ticks since 1601-01-01), scaled by 100.
pub fn now_ns() -> u64 {
    // SAFETY: the call has no preconditions and returns a plain value.
    let ft = unsafe { GetSystemTimePreciseAsFileTime() };
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    ticks * 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic_enough_and_plausible() {
        let a = now_ns();
        let b = now_ns();
        assert!(b >= a);
        // FILETIME counts from 1601: any current date is far above 1e19 ns.
        assert!(a > 10_000_000_000_000_000_000);
    }
}
