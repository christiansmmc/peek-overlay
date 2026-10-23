//! Start with Windows (spec 4.7): a Task Scheduler logon task that runs the app elevated, so
//! there is no UAC prompt at logon. Everything runs `schtasks` without a console window.

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
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

/// Applies startup changes off the caller's thread (`schtasks` takes ~100 ms, too long for the
/// UI thread), one run at a time. Requests made during a run collapse into the newest one,
/// so the task always ends up as last requested.
pub struct TaskUpdater {
    /// The newest request not applied yet.
    pending: Mutex<Option<bool>>,
    /// Held during a run, so runs never overlap.
    running: Mutex<()>,
    apply: Box<dyn Fn(bool) + Send + Sync>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TaskUpdater {
    /// `apply(on)` creates (`true`) or removes (`false`) the task, blocking.
    pub fn new(apply: impl Fn(bool) + Send + Sync + 'static) -> Arc<TaskUpdater> {
        Arc::new(TaskUpdater { pending: Mutex::new(None), running: Mutex::new(()), apply: Box::new(apply) })
    }

    /// Queues `on` and returns at once; a `startup-task` thread applies it.
    pub fn request(self: &Arc<Self>, on: bool) {
        *lock(&self.pending) = Some(on);
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new().name("startup-task".into()).spawn(move || me.run_pending());
        if spawned.is_err() {
            self.run_pending();
        }
    }

    /// Waits for a run in flight, then applies what is still pending. Blocking.
    pub fn flush(&self) {
        self.run_pending();
    }

    fn run_pending(&self) {
        let _running = lock(&self.running);
        let next = lock(&self.pending).take();
        if let Some(on) = next {
            (self.apply)(on);
        }
    }
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
    fn updater_returns_at_once_runs_one_at_a_time_and_applies_the_newest_last() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex, mpsc};
        use std::time::Duration;
        let applied = Arc::new(Mutex::new(Vec::new()));
        let (running, max_running) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (started_tx, release_rx) = (Mutex::new(started_tx), Mutex::new(release_rx));
        let updater = {
            let (applied, running, max_running) = (applied.clone(), running.clone(), max_running.clone());
            TaskUpdater::new(move |on| {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                max_running.fetch_max(now, Ordering::SeqCst);
                let _ = started_tx.lock().unwrap().send(());
                // Like schtasks, each run takes a while; the first one waits for the test.
                let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
                applied.lock().unwrap().push(on);
                running.fetch_sub(1, Ordering::SeqCst);
            })
        };
        updater.request(true);
        started_rx.recv_timeout(Duration::from_secs(5)).expect("the first request runs on its own thread");
        // Requested while the first run is in flight: each returns at once, and they collapse.
        updater.request(false);
        updater.request(true);
        updater.request(false);
        release_tx.send(()).unwrap();
        drop(release_tx);
        updater.flush();
        assert_eq!(*applied.lock().unwrap(), [true, false], "the first run, then only the newest request");
        assert_eq!(max_running.load(Ordering::SeqCst), 1, "runs never overlap");
    }

    #[test]
    fn xml_file_is_utf16le_with_bom() {
        assert_eq!(utf16le_bom("a\u{e7}"), [0xFF, 0xFE, b'a', 0, 0xE7, 0]);
    }
}
