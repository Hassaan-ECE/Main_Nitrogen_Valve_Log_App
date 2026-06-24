use std::fs;

use tauri::Manager;

const MIGRATIONS_DIR_NAME: &str = "migrations";
const LOGS_DIR_NAME: &str = "logs";
const CLEAR_LOCAL_LOGS_V014_MARKER: &str = "clear_local_logs_v0.1.4.done";

pub(crate) fn run_startup_migrations(app: &tauri::AppHandle) {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        return;
    };

    if let Err(error) = clear_local_logs_v014(&app_data_dir) {
        eprintln!("Valve Log migration failed: {error}");
    }
}

fn clear_local_logs_v014(app_data_dir: &std::path::Path) -> Result<(), String> {
    if let Err(error) = fs::create_dir_all(app_data_dir) {
        return Err(error.to_string());
    }

    let migrations_dir = app_data_dir.join(MIGRATIONS_DIR_NAME);
    let marker = migrations_dir.join(CLEAR_LOCAL_LOGS_V014_MARKER);

    if marker.exists() {
        return Ok(());
    }

    let logs_dir = app_data_dir.join(LOGS_DIR_NAME);
    if logs_dir.exists() {
        fs::remove_dir_all(&logs_dir).map_err(|error| error.to_string())?;
    }

    fs::create_dir_all(&migrations_dir).map_err(|error| error.to_string())?;
    fs::write(&marker, b"ok").map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_app_data_dir() -> PathBuf {
        std::env::temp_dir().join(format!("valve-log-migration-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn clear_local_logs_v014_removes_logs_once() {
        let app_data_dir = temp_app_data_dir();
        let logs_dir = app_data_dir.join(LOGS_DIR_NAME);
        fs::create_dir_all(&logs_dir).expect("logs dir");
        fs::write(logs_dir.join("events.jsonl"), b"{}\n").expect("jsonl");

        clear_local_logs_v014(&app_data_dir).expect("first migration");
        assert!(!logs_dir.exists());

        fs::create_dir_all(&logs_dir).expect("recreate logs");
        fs::write(logs_dir.join("events.jsonl"), b"{}\n").expect("jsonl again");

        clear_local_logs_v014(&app_data_dir).expect("second migration");
        assert!(logs_dir.exists());

        let _ = fs::remove_dir_all(&app_data_dir);
    }
}
