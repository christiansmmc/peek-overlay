use std::sync::Arc;

use overlay_engine::controller::Controller;
use overlay_engine::model::{AppState, HotkeyResult};
use overlay_engine::settings::Settings;
use tauri::State;

type Ctl<'a> = State<'a, Arc<Controller>>;

#[tauri::command]
pub fn get_state(ctl: Ctl) -> AppState {
    ctl.state()
}

#[tauri::command]
pub fn update_settings(ctl: Ctl, patch: serde_json::Value) -> Result<Settings, String> {
    ctl.update_settings(patch)
}

#[tauri::command]
pub fn set_overlay_visible(ctl: Ctl, visible: bool) {
    ctl.set_overlay_visible(visible);
}

#[tauri::command]
pub fn begin_hotkey_capture(ctl: Ctl) {
    ctl.begin_hotkey_capture();
}

#[tauri::command]
pub fn end_hotkey_capture(ctl: Ctl) {
    ctl.end_hotkey_capture();
}

#[tauri::command]
pub fn try_set_hotkey(ctl: Ctl, combo: Vec<String>) -> HotkeyResult {
    ctl.try_set_hotkey(combo)
}

#[tauri::command]
pub fn install_driver(ctl: Ctl) {
    ctl.install_driver();
}
