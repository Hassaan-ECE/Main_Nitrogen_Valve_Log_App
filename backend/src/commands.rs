use crate::model::{AppStatus, ValveLogEntry, ValveLogErrorDto, ValveStateSnapshot};
use crate::valve_log;

#[tauri::command]
pub fn get_app_status() -> AppStatus {
    AppStatus {
        app_name: "Valve Log".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn get_current_valve_state(
    app: tauri::AppHandle,
) -> Result<ValveStateSnapshot, ValveLogErrorDto> {
    valve_log::get_current_valve_state(&app).map_err(Into::into)
}

#[tauri::command]
pub fn log_valve_closed(
    app: tauri::AppHandle,
    operator_name: String,
) -> Result<ValveLogEntry, ValveLogErrorDto> {
    valve_log::log_valve_closed(&app, operator_name).map_err(Into::into)
}

#[tauri::command]
pub fn log_valve_opened(
    app: tauri::AppHandle,
    operator_name: String,
) -> Result<ValveLogEntry, ValveLogErrorDto> {
    valve_log::log_valve_opened(&app, operator_name).map_err(Into::into)
}

#[tauri::command]
pub fn open_valve_log(app: tauri::AppHandle) -> Result<String, ValveLogErrorDto> {
    valve_log::open_valve_log(&app).map_err(Into::into)
}

#[tauri::command]
pub fn open_valve_log_folder(app: tauri::AppHandle) -> Result<String, ValveLogErrorDto> {
    valve_log::open_valve_log_folder(&app).map_err(Into::into)
}
