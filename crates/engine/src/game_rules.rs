//! Game rules (spec 4.6): exe paths, game library folders and the classification of the foreground app.

use crate::process_info::is_ignored;
use crate::settings::GameRules;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{HSTRING, PWSTR};

const GOG_KEY: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

/// Folder fragments that only game launchers create (lowercase, backslash-delimited).
pub const LIBRARY_MARKERS: [&str; 6] = [
    r"\steamapps\common\",
    r"\epic games\",
    r"\gog galaxy\games\",
    r"\xboxgames\",
    r"\ea games\",
    r"\ubisoft game launcher\games\",
];

/// Trims, switches `/` to `\` and lowercases, so paths compare as Windows does.
pub fn normalize_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}

/// Install folders of games the launchers keep outside the default markers (Epic, GOG).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryRoots {
    roots: Vec<String>,
}

impl LibraryRoots {
    /// Normalizes the roots, dropping empty ones, bare drives (`c:`) and duplicates.
    pub fn new(roots: impl IntoIterator<Item = String>) -> LibraryRoots {
        let mut out: Vec<String> = Vec::new();
        for r in roots {
            let r = normalize_path(&r);
            let r = r.trim_end_matches('\\');
            if r.is_empty() || r.ends_with(':') || out.iter().any(|o| o == r) {
                continue;
            }
            out.push(r.to_string());
        }
        LibraryRoots { roots: out }
    }

    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// Reads the Epic manifests and the GOG registry; whatever is missing is skipped.
    pub fn load() -> LibraryRoots {
        let mut roots = Vec::new();
        if let Some(pd) = std::env::var_os("ProgramData") {
            let dir = Path::new(&pd).join(r"Epic\EpicGamesLauncher\Data\Manifests");
            roots.extend(epic_roots(&dir));
        }
        roots.extend(gog_roots());
        let roots = LibraryRoots::new(roots);
        crate::trace!("game_rules: {} library roots", roots.roots.len());
        roots
    }
}

/// `path` must already be normalized. True inside a launcher folder or under a known root.
pub fn is_library_path(path: &str, roots: &LibraryRoots) -> bool {
    LIBRARY_MARKERS.iter().any(|m| path.contains(m))
        || roots.roots.iter().any(|r| path.strip_prefix(r.as_str()).is_some_and(|rest| rest.starts_with('\\')))
}

/// What the foreground app is, before the sustained-activity heuristics weigh in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AppClass {
    pub ignored: bool,
    pub known_game: bool,
}

/// `exe_path` must already be normalized. Without a path only the name rules apply.
/// The built-in ignore list (and this app) wins over the user's "games" list.
pub fn classify(exe_name: &str, exe_path: Option<&str>, self_exe: &str, rules: &GameRules, roots: &LibraryRoots) -> AppClass {
    let listed = |list: &[String]| exe_path.is_some_and(|p| list.iter().any(|l| l == p));
    let ignored = is_ignored(exe_name, self_exe) || listed(&rules.not_games);
    let known_game = !ignored && (listed(&rules.games) || exe_path.is_some_and(|p| is_library_path(p, roots)));
    AppClass { ignored, known_game }
}

/// The non-empty `InstallLocation` of an Epic `.item` manifest.
pub fn epic_install_location(manifest: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let loc = v.get("InstallLocation")?.as_str()?;
    (!loc.is_empty()).then(|| loc.to_string())
}

fn epic_roots(manifests_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(manifests_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|m| epic_install_location(&m))
        .collect()
}

/// `path` value of every game GOG Galaxy registered under HKLM.
fn gog_roots() -> Vec<String> {
    let mut out = Vec::new();
    let mut key = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(GOG_KEY), None, KEY_READ, &mut key) };
    if opened != ERROR_SUCCESS {
        return out;
    }
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None)
        };
        if status != ERROR_SUCCESS {
            break;
        }
        let sub = String::from_utf16_lossy(&name[..len as usize]);
        if let Some(p) = reg_sz(key, &sub, "path") {
            out.push(p);
        }
    }
    let _ = unsafe { RegCloseKey(key) };
    out
}

fn reg_sz(key: HKEY, subkey: &str, value: &str) -> Option<String> {
    let get = |data: Option<&mut [u16]>, size: &mut u32| -> WIN32_ERROR {
        let data = data.map(|d| d.as_mut_ptr().cast());
        unsafe {
            RegGetValueW(key, &HSTRING::from(subkey), &HSTRING::from(value), RRF_RT_REG_SZ, None, data, Some(size))
        }
    };
    let mut size = 0u32;
    if get(None, &mut size) != ERROR_SUCCESS {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let mut size = (buf.len() * 2) as u32;
    if get(Some(&mut buf), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    (len > 0).then(|| String::from_utf16_lossy(&buf[..len]))
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn normalize_path_lowercases_and_uses_backslashes() {
    assert_eq!(normalize_path("  D:/SteamLibrary/steamapps/common/Hades/Hades.exe "),
               r"d:\steamlibrary\steamapps\common\hades\hades.exe");
}
#[test]
fn library_markers_cover_every_store() {
    let none = LibraryRoots::default();
    for p in [
        r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe",
        r"C:\Program Files (x86)\Steam\steamapps\common\Celeste\Celeste.exe",
        r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe",
        r"C:\Program Files (x86)\GOG Galaxy\Games\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe",
        r"C:\XboxGames\Starfield\Content\Starfield.exe",
        r"C:\Program Files\EA Games\Battlefield 2042\BF2042.exe",
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Far Cry 6\bin\FarCry6.exe",
    ] {
        assert!(is_library_path(&normalize_path(p), &none), "{p}");
    }
    for p in [
        r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.21\WindowsTerminal.exe",
        r"C:\Users\me\AppData\Local\Discord\app-1.0\Discord.exe",
        r"C:\Tools\steamapps\notes.exe",
    ] {
        assert!(!is_library_path(&normalize_path(p), &none), "{p}");
    }
}
#[test]
fn library_roots_match_whole_folders_only() {
    let roots = LibraryRoots::new([r"D:\Jogos\Foo\".to_string(), r"C:\".to_string(), String::new()]);
    assert_eq!(roots.roots(), [r"d:\jogos\foo".to_string()]);
    assert!(is_library_path(r"d:\jogos\foo\bin\foo.exe", &roots));
    assert!(!is_library_path(r"d:\jogos\foobar\foobar.exe", &roots));
    assert!(!is_library_path(r"c:\tools\x.exe", &roots));
}
#[test]
fn epic_manifest_gives_its_install_location() {
    let m = r#"{"FormatVersion":0,"DisplayName":"Hades","InstallLocation":"D:\\Jogos\\Hades","LaunchExecutable":"Hades.exe"}"#;
    assert_eq!(epic_install_location(m).as_deref(), Some(r"D:\Jogos\Hades"));
    assert_eq!(epic_install_location("{not json"), None);
    assert_eq!(epic_install_location(r#"{"InstallLocation":""}"#), None);
    assert_eq!(epic_install_location(r#"{"DisplayName":"x"}"#), None);
}
#[test]
fn epic_roots_skip_broken_manifests() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.item"), r#"{"InstallLocation":"D:\\Jogos\\Hades"}"#).unwrap();
    std::fs::write(dir.path().join("b.item"), "{not json").unwrap();
    std::fs::write(dir.path().join("c.txt"), r#"{"InstallLocation":"D:\\Outro"}"#).unwrap();
    assert_eq!(epic_roots(dir.path()), vec![r"D:\Jogos\Hades".to_string()]);
    assert!(epic_roots(&dir.path().join("missing")).is_empty());
}
fn rules() -> GameRules {
    GameRules {
        games: vec![r"c:\tools\emu\emu.exe".into()],
        not_games: vec![r"d:\steamlibrary\steamapps\common\tool\tool.exe".into()],
    }
}
#[test]
fn classify_by_name_lists_and_library() {
    let roots = LibraryRoots::default();
    let c = |name: &str, path: &str| classify(name, Some(path), "peek.exe", &rules(), &roots);
    assert_eq!(c("hades.exe", r"d:\steamlibrary\steamapps\common\hades\hades.exe"), AppClass { ignored: false, known_game: true });
    assert_eq!(c("emu.exe", r"c:\tools\emu\emu.exe"), AppClass { ignored: false, known_game: true });
    assert_eq!(c("tool.exe", r"d:\steamlibrary\steamapps\common\tool\tool.exe"), AppClass { ignored: true, known_game: false });
    assert_eq!(c("notepad.exe", r"c:\windows\notepad.exe"), AppClass::default());
    assert_eq!(c("Peek.exe", r"c:\program files\peek\peek.exe"), AppClass { ignored: true, known_game: false });
}
#[test]
fn builtin_ignore_wins_over_the_games_list() {
    let discord = r"c:\users\me\appdata\local\discord\discord.exe";
    let r = GameRules { games: vec![discord.into()], ..GameRules::default() };
    assert_eq!(classify("Discord.exe", Some(discord), "peek.exe", &r, &LibraryRoots::default()),
               AppClass { ignored: true, known_game: false });
}
#[test]
fn classify_without_a_path_uses_only_the_name() {
    let roots = LibraryRoots::new([r"d:\jogos".to_string()]);
    assert_eq!(classify("witcher3.exe", None, "peek.exe", &rules(), &roots), AppClass::default());
    assert!(classify("cmd.exe", None, "peek.exe", &rules(), &roots).ignored);
}
}

#[cfg(test)]
mod win32_tests {
    use super::*;

    #[test]
    fn load_reads_this_machine_without_failing() {
        let roots = LibraryRoots::load();
        assert!(roots.roots().iter().all(|r| *r == normalize_path(r) && !r.ends_with('\\')), "{roots:?}");
    }
}
