//! Start with Windows (spec 4.7): a Task Scheduler logon task that runs the app elevated, so
//! there is no UAC prompt at logon. Everything runs `schtasks` without a console window.

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

pub const TASK_NAME: &str = "Peek";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Task Scheduler 1.2 definition: elevated at logon of `user_id`, no limits, normal priority.
pub fn task_xml(exe: &Path, user_id: &str) -> String {
    let user = xml_escape(user_id);
    let command = xml_escape(&exe.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn schtasks(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .output()
}

/// UTF-16LE with a byte order mark, as `schtasks /XML` expects for a `UTF-16` declaration.
fn utf16le_bom(s: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

/// Writes the task definition to a new file in `%TEMP%` with a random name, created
/// exclusively: an elevated `schtasks` reads it, so its path must not be predictable (a fixed
/// name could be swapped before it is read). The file is deleted when the result is dropped.
fn write_task_file(xml: &str) -> std::io::Result<tempfile::TempPath> {
    let mut file = tempfile::Builder::new().prefix("peek-task-").suffix(".xml").tempfile()?;
    file.write_all(&utf16le_bom(xml))?;
    // Closed before schtasks opens it, whatever sharing mode schtasks asks for.
    Ok(file.into_temp_path())
}

/// Creates or replaces the task (`/F`). Needs an elevated process (`HighestAvailable`).
/// Blocking: it spawns `schtasks`.
pub fn enable(exe: &Path) -> Result<(), String> {
    let user = format!(
        r"{}\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").map_err(|e| format!("USERNAME: {e}"))?
    );
    let file = write_task_file(&task_xml(exe, &user)).map_err(|e| format!("writing the task file: {e}"))?;
    let file_arg = file.to_string_lossy().into_owned();
    let out = schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &file_arg, "/F"]);
    let _ = file.close();
    let out = out.map_err(|e| format!("could not run schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("schtasks /Create failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Deletes the task; a task that does not exist counts as done. Blocking.
pub fn disable() -> Result<(), String> {
    if !is_enabled() {
        return Ok(());
    }
    let out = schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]).map_err(|e| format!("could not run schtasks: {e}"))?;
    if out.status.success() || !is_enabled() {
        Ok(())
    } else {
        Err(format!("schtasks /Delete failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

pub fn is_enabled() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME]).is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_runs_elevated_at_logon_without_limits() {
        let x = task_xml(std::path::Path::new(r"C:\Program Files\A&B\perf-overlay.exe"), r"PC\csequ");
        for want in ["<LogonTrigger>", r"<UserId>PC\csequ</UserId>", "<RunLevel>HighestAvailable</RunLevel>",
                     "<LogonType>InteractiveToken</LogonType>", "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
                     "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
                     "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>", "<Priority>4</Priority>",
                     r"<Command>C:\Program Files\A&amp;B\perf-overlay.exe</Command>"] {
            assert!(x.contains(want), "faltou {want}");
        }
    }

    #[test]
    fn task_file_has_a_random_name_and_is_deleted_after_use() {
        let a = write_task_file("<Task/>").unwrap();
        let b = write_task_file("<Task/>").unwrap();
        assert_ne!(a.to_path_buf(), b.to_path_buf());
        assert_eq!(a.parent(), Some(std::env::temp_dir().as_path()));
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("peek-task-") && name.ends_with(".xml") && name.len() > "peek-task-.xml".len(), "{name}");
        assert_eq!(std::fs::read(&a).unwrap(), utf16le_bom("<Task/>"));
        let path = a.to_path_buf();
        a.close().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn xml_file_is_utf16le_with_bom() {
        assert_eq!(utf16le_bom("a\u{e7}"), [0xFF, 0xFE, b'a', 0, 0xE7, 0]);
    }
}
