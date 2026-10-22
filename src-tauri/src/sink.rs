use overlay_engine::controller::UiSink;
use tauri::{AppHandle, Emitter};

/// Forwards controller events to the webviews. Called while the controller holds its
/// internal gate: it must stay non-blocking and never call back into the controller.
pub struct TauriSink(pub AppHandle);

impl UiSink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit(event, payload);
    }
}
