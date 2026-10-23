use serde::{Deserialize, Serialize};

use crate::settings::Settings;

pub const REASON_NEEDS_ADMIN: &str = "Execute o app como administrador.";
pub const REASON_NO_GPU_TEMP: &str = "Esta GPU não informa a temperatura.";
pub const REASON_NO_GPU_COUNTERS: &str = "Contadores de GPU indisponíveis.";
pub const REASON_NO_CPU_COUNTERS: &str = "Contadores de CPU indisponíveis.";
pub const REASON_CPU_UNSUPPORTED: &str = "Este processador não é suportado.";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetricsSample {
    pub fps: Option<u32>,
    pub frametime_ms: Option<f32>,
    pub graph: Vec<f32>,
    pub graph_max_ms: f32,
    pub gpu_use: Option<u8>,
    pub gpu_temp: Option<u8>,
    pub cpu_use: Option<u8>,
    pub cpu_temp: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Avail {
    Ok,
    NoSensor { reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub fps: Avail,
    pub gpu_use: Avail,
    pub gpu_temp: Avail,
    pub cpu_use: Avail,
    pub cpu_temp: Avail,
}

impl Availability {
    pub fn all_ok() -> Self {
        Self { fps: Avail::Ok, gpu_use: Avail::Ok, gpu_temp: Avail::Ok, cpu_use: Avail::Ok, cpu_temp: Avail::Ok }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverState {
    Missing,
    Installing,
    Failed,
    Installed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    /// The overlay is on (hotkey, chip, tray or auto-show); with `overlay_waiting` it is on but
    /// hidden until the detected game is in front.
    pub overlay_visible: bool,
    pub game_name: Option<String>,
    /// Latest foreground app outside the built-in ignore list, offered to "marcar como jogo".
    pub last_app_name: Option<String>,
    pub overlay_waiting: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub settings: Settings,
    pub status: Status,
    pub availability: Availability,
    pub driver: DriverState,
    pub hotkey_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyResult {
    pub ok: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_serializes_for_the_ui() {
        let st = AppState {
            settings: Settings::default(),
            status: Status {
                overlay_visible: true,
                game_name: Some("The Witcher 3".into()),
                last_app_name: None,
                overlay_waiting: false,
            },
            availability: Availability { gpu_temp: Avail::NoSensor { reason: "x".into() }, ..Availability::all_ok() },
            driver: DriverState::Missing,
            hotkey_error: None,
        };
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v["status"]["overlayVisible"], true);
        assert_eq!(v["status"]["gameName"], "The Witcher 3");
        assert_eq!(v["status"]["overlayWaiting"], false);
        assert!(v["status"]["lastAppName"].is_null());
        assert_eq!(v["availability"]["gpuTemp"], serde_json::json!({"state": "noSensor", "reason": "x"}));
        assert_eq!(v["availability"]["cpuUse"], serde_json::json!({"state": "ok"}));
        assert_eq!(v["driver"], "missing");
        assert!(v["hotkeyError"].is_null());
    }
    #[test]
    fn metrics_sample_serializes_camel_case() {
        let v = serde_json::to_value(MetricsSample { frametime_ms: Some(7.5), graph_max_ms: 8.0, ..Default::default() }).unwrap();
        assert_eq!(v["frametimeMs"], 7.5);
        assert_eq!(v["graphMaxMs"], 8.0);
        assert!(v["fps"].is_null());
    }
}
