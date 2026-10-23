use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Metrics {
    pub fps: bool,
    pub graph: bool,
    pub gpu_use: bool,
    pub gpu_temp: bool,
    pub cpu_use: bool,
    pub cpu_temp: bool,
}

impl Default for Metrics {
    fn default() -> Self {
        Self { fps: true, graph: true, gpu_use: true, gpu_temp: true, cpu_use: true, cpu_temp: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Line,
    Card,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pos {
    Tl,
    Tc,
    Tr,
    Ml,
    Mr,
    Bl,
    Bc,
    Br,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct General {
    pub startup: bool,
    pub minimized: bool,
    pub auto_show: bool,
}

impl Default for General {
    fn default() -> Self {
        Self { startup: true, minimized: true, auto_show: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub metrics: Metrics,
    pub style: Style,
    pub pos: Pos,
    pub size: u32,
    pub opacity: u32,
    pub shortcut: Vec<String>,
    pub general: General,
}

fn default_shortcut() -> Vec<String> {
    ["Ctrl", "Shift", "O"].iter().map(|s| s.to_string()).collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            metrics: Metrics::default(),
            style: Style::Card,
            pos: Pos::Tr,
            size: 100,
            opacity: 80,
            shortcut: default_shortcut(),
            general: General::default(),
        }
    }
}

/// Rounds to the nearest multiple of 5, then clamps to `min..=max`.
fn snap(v: u32, min: u32, max: u32) -> u32 {
    (v.saturating_add(2) / 5 * 5).clamp(min, max)
}

impl Settings {
    pub fn normalized(mut self) -> Settings {
        self.size = snap(self.size, 75, 200);
        self.opacity = snap(self.opacity, 30, 100);
        if hotkey::parse(&self.shortcut).is_err() {
            self.shortcut = default_shortcut();
        }
        self
    }
}

#[derive(Debug, PartialEq)]
pub enum LoadOutcome {
    Loaded,
    Missing,
    Corrupt,
}

pub fn settings_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    base.join("Peek").join("settings.json")
}

pub fn load(path: &Path) -> (Settings, LoadOutcome) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), LoadOutcome::Missing);
        }
        Err(_) => return (Settings::default(), LoadOutcome::Corrupt),
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(s) => (s.normalized(), LoadOutcome::Loaded),
        Err(_) => (Settings::default(), LoadOutcome::Corrupt),
    }
}

/// `load` for a launch of the app. A missing file is the first run: the defaults are written
/// right away, so the next launch is no longer one even if nothing was changed (spec 4.1).
pub fn load_or_create(path: &Path) -> (Settings, LoadOutcome) {
    let (settings, outcome) = load(path);
    if outcome == LoadOutcome::Missing
        && let Err(e) = save(path, &settings)
    {
        crate::trace!("settings: writing the defaults failed: {e}");
    }
    (settings, outcome)
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

pub fn apply_patch(current: &Settings, patch: serde_json::Value) -> Result<Settings, String> {
    let mut merged = serde_json::to_value(current).map_err(|e| e.to_string())?;
    merge(&mut merged, patch);
    let s: Settings = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    Ok(s.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        assert_eq!(serde_json::to_value(Settings::default()).unwrap(), serde_json::json!({
            "version": 1,
            "metrics": {"fps": true, "graph": true, "gpuUse": true, "gpuTemp": true, "cpuUse": true, "cpuTemp": false},
            "style": "card", "pos": "tr", "size": 100, "opacity": 80,
            "shortcut": ["Ctrl", "Shift", "O"],
            "general": {"startup": true, "minimized": true, "autoShow": true}
        }));
    }
    #[test]
    fn normalize_clamps_and_snaps_to_step_5() {
        let s = Settings { size: 203, opacity: 12, ..Settings::default() }.normalized();
        assert_eq!((s.size, s.opacity), (200, 30));
        let s = Settings { size: 77, opacity: 83, ..Settings::default() }.normalized();
        assert_eq!((s.size, s.opacity), (75, 85));
        let s = Settings { size: 82, ..Settings::default() }.normalized();
        assert_eq!(s.size, 80);
    }
    #[test]
    fn invalid_shortcut_falls_back_to_default() {
        let s = Settings { shortcut: vec!["O".into()], ..Settings::default() }.normalized();
        assert_eq!(s.shortcut, vec!["Ctrl", "Shift", "O"]);
    }
    #[test]
    fn load_missing_corrupt_and_partial() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(load(&p), (Settings::default(), LoadOutcome::Missing));
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load(&p), (Settings::default(), LoadOutcome::Corrupt));
        std::fs::write(&p, r#"{"style":"line","size":999,"opacity":0}"#).unwrap();
        let (s, o) = load(&p);
        assert_eq!(o, LoadOutcome::Loaded);
        assert_eq!((s.style, s.size, s.opacity), (Style::Line, 200, 30));
        assert_eq!(s.metrics, Metrics::default());
    }
    #[test]
    fn first_launch_writes_the_defaults_so_only_it_counts_as_first() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Peek").join("settings.json");
        assert_eq!(load_or_create(&p), (Settings::default(), LoadOutcome::Missing));
        assert_eq!(load(&p), (Settings::default(), LoadOutcome::Loaded), "defaults saved on the first launch");
        assert_eq!(load_or_create(&p), (Settings::default(), LoadOutcome::Loaded));
        // A corrupt file falls back to the defaults but is left alone.
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load_or_create(&p), (Settings::default(), LoadOutcome::Corrupt));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{not json");
    }
    #[test]
    fn save_creates_dir_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Peek").join("settings.json");
        let s = Settings { pos: Pos::Bl, ..Settings::default() };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), (s, LoadOutcome::Loaded));
    }
    #[test]
    fn apply_patch_merges_nested_and_rejects_bad_enum() {
        let s = apply_patch(&Settings::default(), serde_json::json!({"metrics": {"cpuTemp": true}, "pos": "bl"})).unwrap();
        assert!(s.metrics.cpu_temp && s.metrics.fps);
        assert_eq!(s.pos, Pos::Bl);
        assert!(apply_patch(&Settings::default(), serde_json::json!({"pos": "center"})).is_err());
    }
}
