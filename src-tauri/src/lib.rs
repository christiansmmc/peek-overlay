mod commands;
mod config_window;
mod sink;
mod tray;

use std::sync::Arc;

use overlay_engine::controller::Controller;
use overlay_engine::settings::{load, settings_path, LoadOutcome};
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            config_window::open(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::update_settings,
            commands::set_overlay_visible,
            commands::begin_hotkey_capture,
            commands::end_hotkey_capture,
            commands::try_set_hotkey,
            commands::install_driver,
        ])
        .setup(|app| {
            let path = settings_path();
            let (settings, outcome) = load(&path);
            let open_config = outcome == LoadOutcome::Missing || !settings.general.minimized;
            let sink = Arc::new(sink::TauriSink(app.handle().clone()));
            let controller = Controller::new(settings, path, sink);
            app.manage(controller.clone());
            controller.start();
            tray::build(app)?;
            if open_config {
                config_window::open(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
