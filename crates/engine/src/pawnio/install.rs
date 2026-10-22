//! Installs the official PawnIO package through winget, without a console window.

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

pub fn winget_args() -> [&'static str; 8] {
    [
        "install",
        "--id",
        "namazso.PawnIO",
        "-e",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
}

/// Runs winget and waits for it (blocking: call it off the UI thread). Success is judged by
/// `is_installed()` afterwards, whatever winget's exit code ("already installed" and "restart
/// required" are non-zero codes with PawnIO in place).
pub fn run_install() -> Result<(), String> {
    let status = Command::new("winget")
        .args(winget_args())
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not run winget: {e}"))?;
    if super::is_installed() {
        Ok(())
    } else {
        Err(format!("winget finished ({status}) but PawnIO is not installed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winget_command_is_silent_and_exact() {
        assert_eq!(winget_args(), ["install", "--id", "namazso.PawnIO", "-e", "--silent",
            "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"]);
    }
}
