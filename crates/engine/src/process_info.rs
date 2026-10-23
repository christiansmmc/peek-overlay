//! Process helpers: ignore list, display-name fallback chain and two small Win32 lookups.

use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{PCWSTR, PWSTR};

const IGNORED: &[&str] = &[
    "explorer.exe",
    "dwm.exe",
    "shellexperiencehost.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "applicationframehost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "msedge.exe",
    "chrome.exe",
    "firefox.exe",
    "opera.exe",
    "brave.exe",
    "vivaldi.exe",
    "msedgewebview2.exe",
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "discord.exe",
    "slack.exe",
    "ms-teams.exe",
    "teams.exe",
    "code.exe",
    "cursor.exe",
    "zed.exe",
    "spotify.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "epicgameslauncher.exe",
    // Lives under \epic games\, so the library marker would call it a game.
    "epicwebhelper.exe",
    "eadesktop.exe",
    "battle.net.exe",
    "galaxyclient.exe",
    "ubisoftconnect.exe",
    // The Ubisoft Connect window, not a game.
    "upc.exe",
    "obs64.exe",
    "nvidia app.exe",
    "nvidia overlay.exe",
    "overwolf.exe",
    "claude.exe",
    "taskmgr.exe",
    "systemsettings.exe",
    // Wallpaper Engine lives under \steamapps\common\, which the library marker would call a game.
    "wallpaper32.exe",
    "wallpaper64.exe",
];

pub fn is_ignored(exe_name: &str, self_exe: &str) -> bool {
    IGNORED.iter().any(|n| exe_name.eq_ignore_ascii_case(n)) || exe_name.eq_ignore_ascii_case(self_exe)
}

pub fn display_name(file_description: Option<&str>, window_title: Option<&str>, exe_name: Option<&str>) -> String {
    let exe_stem = exe_name.map(|e| {
        let e = e.trim();
        match e.len().checked_sub(4) {
            Some(i) if e.is_char_boundary(i) && e[i..].eq_ignore_ascii_case(".exe") => &e[..i],
            _ => e,
        }
    });
    [file_description, window_title, exe_stem]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("Jogo")
        .to_string()
}

/// Full image path of a process; `None` when it cannot be opened (e.g. protected processes).
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 32768];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
    }
}

/// `FileDescription` of the executable, from the first `\VarFileInfo\Translation` entry.
pub fn file_description(path: &Path) -> Option<String> {
    let wpath: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wpath.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wpath.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;

        let mut p: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut n = 0u32;
        let key: Vec<u16> = "\\VarFileInfo\\Translation".encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(key.as_ptr()), &mut p, &mut n).as_bool() || n < 4 {
            return None;
        }
        let pair = std::slice::from_raw_parts(p as *const u16, 2);
        let sub: Vec<u16> = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", pair[0], pair[1])
            .encode_utf16()
            .chain(Some(0))
            .collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut p, &mut n).as_bool() || n == 0 || p.is_null() {
            return None;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, n as usize));
        let s = s.trim_end_matches('\0').trim();
        (!s.is_empty()).then(|| s.to_string())
    }
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn ignores_shell_browsers_and_self() {
    for exe in ["explorer.exe", "DWM.EXE", "msedge.exe", "chrome.exe", "firefox.exe", "brave.exe",
                "opera.exe", "vivaldi.exe", "msedgewebview2.exe", "Perf-Overlay.exe", "WindowsTerminal.exe",
                "OpenConsole.exe", "Discord.exe", "Code.exe", "steamwebhelper.exe", "EpicWebHelper.exe",
                "NVIDIA Overlay.exe", "wallpaper64.exe"] {
        assert!(is_ignored(exe, "perf-overlay.exe"), "{exe}");
    }
    assert!(!is_ignored("witcher3.exe", "perf-overlay.exe"));
}
#[test]
fn display_name_fallback_chain() {
    assert_eq!(display_name(Some("The Witcher 3"), Some("The Witcher 3: Wild Hunt"), Some("witcher3.exe")), "The Witcher 3");
    assert_eq!(display_name(Some("  "), Some("Fake Game"), Some("fake_game.exe")), "Fake Game");
    assert_eq!(display_name(None, None, Some("game.exe")), "game");
    assert_eq!(display_name(None, Some(""), None), "Jogo");
}

}

#[cfg(test)]
mod win32_tests {
    use super::*;

    #[test]
    fn exe_path_of_current_process_and_system_file_description() {
        let me = exe_path(std::process::id()).expect("own exe path");
        assert!(me.is_file());
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        let d = file_description(notepad).expect("notepad has a file description");
        assert!(!d.is_empty());
        assert!(file_description(Path::new(r"C:\does\not\exist.exe")).is_none());
        assert!(exe_path(0).is_none());
    }
}
