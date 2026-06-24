mod commands;
mod migrations;
mod model;
mod shared_sync;
mod shared_watcher;
mod valve_log;

use shared_watcher::SharedStateWatcher;
use tauri::Manager;

pub use model::{AppStatus, ValveLogEntry, ValveLogErrorDto, ValveStateSnapshot};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            migrations::run_startup_migrations(app.handle());
            app.manage(SharedStateWatcher::new());
            let shared_paths = shared_sync::resolve_shared_paths();
            if let Some(watcher) = app.try_state::<SharedStateWatcher>() {
                let _ = watcher.ensure_watching(app.handle().clone(), &shared_paths.shared_dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::get_current_valve_state,
            commands::log_valve_closed,
            commands::log_valve_opened,
            commands::open_valve_log,
            commands::open_valve_log_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Valve Log app");
}
