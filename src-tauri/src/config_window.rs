use std::sync::Arc;

use overlay_engine::controller::Controller;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "config";

/// Shows the settings window, creating it when it does not exist.
pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Peek")
        .inner_size(480.0, 680.0)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .shadow(true)
        .center()
        .build();
    match built {
        Ok(window) => {
            let ctl = app.state::<Arc<Controller>>().inner().clone();
            ctl.set_config_open(true);
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    ctl.set_config_open(false);
                }
            });
        }
        Err(_e) => overlay_engine::trace!("config window failed: {_e}"),
    }
}
