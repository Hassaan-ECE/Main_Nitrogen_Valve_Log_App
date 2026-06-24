use chrono::{Local, Utc};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{Emitter, Manager};
use uuid::Uuid;

use crate::model::{ValveLogEntry, ValveLogErrorDto, ValveStateSnapshot};
use crate::shared_sync::{self, SharedSyncPaths};

const WORKBOOK_FILE_NAME: &str = "Main Nitrogen Valve Log.xlsx";
const SOURCE_LOG_FILE_NAME: &str = "events.jsonl";
const VALVE_NAME: &str = "Main Nitrogen Valve";
const CLOSE_ACTION: &str = "Closed Valve";
const OPEN_ACTION: &str = "Opened Valve";
const LEGACY_CLOSE_ACTION: &str = "Close Valve";
const LEGACY_OPEN_ACTION: &str = "Open Valve";
const OPEN_STATE: &str = "Open";
const CLOSED_STATE: &str = "Closed";
const MANUAL_SOURCE: &str = "Manual";
const CLOSE_NOTE: &str = "Temporary manual close log";
const OPEN_NOTE: &str = "Temporary manual open log";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoggedValveState {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValveAction {
    Close,
    Open,
}

#[derive(Debug)]
pub struct ValveLogError {
    code: &'static str,
    message: &'static str,
    detail: Option<String>,
    event_saved: bool,
    entry: Option<ValveLogEntry>,
}

struct LocalLogPaths {
    log_dir: PathBuf,
    source_log_path: PathBuf,
    workbook_path: PathBuf,
    client_id: String,
}

impl LoggedValveState {
    fn from_new_state(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    #[cfg(test)]
    fn as_lower_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    fn as_display_str(self) -> &'static str {
        match self {
            Self::Open => OPEN_STATE,
            Self::Closed => CLOSED_STATE,
        }
    }
}

impl ValveAction {
    fn action_label(self) -> &'static str {
        match self {
            Self::Close => CLOSE_ACTION,
            Self::Open => OPEN_ACTION,
        }
    }

    fn previous_state(self) -> LoggedValveState {
        match self {
            Self::Close => LoggedValveState::Open,
            Self::Open => LoggedValveState::Closed,
        }
    }

    fn new_state(self) -> LoggedValveState {
        match self {
            Self::Close => LoggedValveState::Closed,
            Self::Open => LoggedValveState::Open,
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Close => CLOSE_NOTE,
            Self::Open => OPEN_NOTE,
        }
    }

    fn duplicate_error(self) -> ValveLogError {
        match self {
            Self::Close => ValveLogError::new(
                "invalid_transition",
                "The valve log is already marked closed. Refresh the app and try again.",
            ),
            Self::Open => ValveLogError::new(
                "invalid_transition",
                "The valve log is already marked open. Refresh the app and try again.",
            ),
        }
    }
}

impl ValveLogError {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            detail: None,
            event_saved: false,
            entry: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn with_saved_entry(mut self, entry: ValveLogEntry) -> Self {
        self.event_saved = true;
        self.entry = Some(entry);
        self
    }
}

impl From<ValveLogError> for ValveLogErrorDto {
    fn from(error: ValveLogError) -> Self {
        Self {
            code: error.code.to_string(),
            message: error.message.to_string(),
            detail: error.detail,
            event_saved: error.event_saved,
            entry: error.entry,
        }
    }
}

pub fn get_current_valve_state(
    app: &tauri::AppHandle,
) -> Result<ValveStateSnapshot, ValveLogError> {
    let shared_paths = shared_sync::resolve_shared_paths();
    if let Some(watcher) = app.try_state::<crate::shared_watcher::SharedStateWatcher>() {
        let _ = watcher.ensure_watching(app.clone(), &shared_paths.shared_dir);
    }

    let local_paths = local_log_paths(app)?;
    let local_entries = synchronized_local_entries(&local_paths, &shared_paths)?;

    shared_sync::load_fast_snapshot(&local_entries, &shared_paths).map_err(|error| {
        ValveLogError::new("source_log_read_failed", "The valve log could not be read.")
            .with_detail(error)
    })
}

pub fn log_valve_closed(
    app: &tauri::AppHandle,
    operator_name: String,
) -> Result<ValveLogEntry, ValveLogError> {
    let local_paths = local_log_paths(app)?;

    log_valve_action_at_paths(app, &local_paths, ValveAction::Close, operator_name)
}

pub fn log_valve_opened(
    app: &tauri::AppHandle,
    operator_name: String,
) -> Result<ValveLogEntry, ValveLogError> {
    let local_paths = local_log_paths(app)?;

    log_valve_action_at_paths(app, &local_paths, ValveAction::Open, operator_name)
}

pub fn open_valve_log(app: &tauri::AppHandle) -> Result<String, ValveLogError> {
    let local_paths = prepare_workbook(app)?;

    open_path_with_default_app(&local_paths.workbook_path).map_err(|error| {
        ValveLogError::new("open_log_failed", "The Excel log could not be opened.")
            .with_detail(error)
    })?;

    Ok(local_paths.workbook_path.display().to_string())
}

pub fn open_valve_log_folder(app: &tauri::AppHandle) -> Result<String, ValveLogError> {
    let local_paths = prepare_workbook(app)?;

    open_folder_with_default_app(&local_paths.log_dir).map_err(|error| {
        ValveLogError::new(
            "open_log_folder_failed",
            "The log folder could not be opened.",
        )
        .with_detail(error)
    })?;

    Ok(local_paths.log_dir.display().to_string())
}

fn prepare_workbook(app: &tauri::AppHandle) -> Result<LocalLogPaths, ValveLogError> {
    let local_paths = local_log_paths(app)?;
    let shared_paths = shared_sync::resolve_shared_paths();

    ensure_log_directory(&local_paths)?;

    let local_entries = synchronized_local_entries(&local_paths, &shared_paths)?;
    let merged_entries = shared_sync::merged_canonical_entries(&local_entries, &shared_paths)
        .map_err(|error| {
            ValveLogError::new("source_log_read_failed", "The valve log could not be read.")
                .with_detail(error)
        })?;

    refresh_workbook(&local_paths.workbook_path, &merged_entries).map_err(|error| {
        ValveLogError::new(
            "excel_refresh_failed",
            "The Excel log could not be refreshed. Close Excel and try opening the log again.",
        )
        .with_detail(error)
    })?;

    Ok(local_paths)
}

fn ensure_log_directory(paths: &LocalLogPaths) -> Result<(), ValveLogError> {
    fs::create_dir_all(&paths.log_dir).map_err(|error| {
        ValveLogError::new(
            "log_directory_failed",
            "The log folder could not be created.",
        )
        .with_detail(error.to_string())
    })?;

    Ok(())
}

fn log_valve_action_at_paths(
    app: &tauri::AppHandle,
    local_paths: &LocalLogPaths,
    action: ValveAction,
    operator_name: String,
) -> Result<ValveLogEntry, ValveLogError> {
    let operator_name = normalize_operator_name(&operator_name)?;
    let shared_paths = shared_sync::resolve_shared_paths();
    ensure_log_directory(local_paths)?;

    let local_entries = synchronized_local_entries(local_paths, &shared_paths)?;

    if !shared_sync::shared_root_available(&shared_paths) {
        let snapshot = shared_sync::compute_merged_snapshot(&local_entries, &shared_paths)
            .map_err(|error| {
                ValveLogError::new("source_log_read_failed", "The valve log could not be read.")
                    .with_detail(error)
            })?;
        validate_transition(action, &snapshot)?;

        let entry = entry_for_action(action, operator_name);
        append_source_entry(&local_paths.source_log_path, &entry).map_err(|error| {
            ValveLogError::new(
                "source_log_write_failed",
                "The valve event could not be saved.",
            )
            .with_detail(error)
        })?;

        return Err(ValveLogError::new(
            "shared_sync_unavailable",
            "Shared sync unavailable — event saved locally only.",
        )
        .with_saved_entry(entry));
    }

    let source_log_path = local_paths.source_log_path.clone();
    let (entry, merged_entries) =
        shared_sync::commit_shared_valve_event(&shared_paths, &local_paths.client_id, || {
            let local_entries = read_valid_source_entries(&source_log_path).map_err(|error| {
                shared_sync::SharedSyncError::Message(error.message.to_string())
            })?;
            let snapshot = shared_sync::compute_merged_snapshot(&local_entries, &shared_paths)
                .map_err(|error| shared_sync::SharedSyncError::Message(error))?;
            validate_transition(action, &snapshot).map_err(|error| {
                shared_sync::SharedSyncError::Message(error.message.to_string())
            })?;

            let entry = entry_for_action(action, operator_name.clone());
            Ok((entry, local_entries))
        })
        .map_err(map_shared_sync_error)?;

    append_source_entry(&local_paths.source_log_path, &entry).map_err(|error| {
        ValveLogError::new(
            "local_log_write_failed",
            "The event was saved to the shared valve log, but this PC's local log could not be updated.",
        )
        .with_detail(error)
        .with_saved_entry(entry.clone())
    })?;

    if let Some(watcher) = app.try_state::<crate::shared_watcher::SharedStateWatcher>() {
        let _ = watcher.ensure_watching(app.clone(), &shared_paths.shared_dir);
    }
    let _ = app.emit(crate::shared_watcher::VALVE_LOG_CHANGED_EVENT, ());

    refresh_workbook(&local_paths.workbook_path, &merged_entries)
        .map(|()| entry.clone())
        .map_err(|error| {
            ValveLogError::new(
                "excel_refresh_failed",
                "The event was saved, but the Excel log could not be refreshed. Close Excel and try opening the log again.",
            )
            .with_detail(error)
            .with_saved_entry(entry)
        })
}

fn map_shared_sync_error(error: shared_sync::SharedSyncError) -> ValveLogError {
    match error {
        shared_sync::SharedSyncError::Busy => ValveLogError::new(
            "shared_log_busy",
            "Another operator is logging a valve event. Try again.",
        ),
        shared_sync::SharedSyncError::Message(message) => ValveLogError::new(
            "shared_sync_failed",
            "The event could not be saved to the shared valve log.",
        )
        .with_detail(message),
    }
}

fn map_shared_restore_error(error: shared_sync::SharedSyncError) -> ValveLogError {
    match error {
        shared_sync::SharedSyncError::Busy => ValveLogError::new(
            "shared_log_busy",
            "Another operator is logging a valve event. Try again.",
        ),
        shared_sync::SharedSyncError::Message(message) => ValveLogError::new(
            "shared_restore_failed",
            "The shared valve log could not be restored from this PC.",
        )
        .with_detail(message),
    }
}

fn validate_transition(
    action: ValveAction,
    snapshot: &ValveStateSnapshot,
) -> Result<(), ValveLogError> {
    let current_state = match snapshot.state.as_str() {
        "open" => LoggedValveState::Open,
        "closed" => LoggedValveState::Closed,
        _ => LoggedValveState::Open,
    };

    if current_state != action.previous_state() {
        return Err(action.duplicate_error());
    }

    Ok(())
}

fn local_log_paths(app: &tauri::AppHandle) -> Result<LocalLogPaths, ValveLogError> {
    let app_data_dir = app.path().app_data_dir().map_err(|error| {
        ValveLogError::new(
            "log_directory_failed",
            "The log folder could not be created.",
        )
        .with_detail(error.to_string())
    })?;
    let log_dir = app_data_dir.join("logs");
    let client_id = shared_sync::client_id(&app_data_dir).map_err(|error| {
        ValveLogError::new(
            "log_directory_failed",
            "The log folder could not be created.",
        )
        .with_detail(error)
    })?;

    Ok(LocalLogPaths {
        source_log_path: log_dir.join(SOURCE_LOG_FILE_NAME),
        workbook_path: log_dir.join(WORKBOOK_FILE_NAME),
        log_dir,
        client_id,
    })
}

fn synchronized_local_entries(
    local_paths: &LocalLogPaths,
    shared_paths: &SharedSyncPaths,
) -> Result<Vec<ValveLogEntry>, ValveLogError> {
    let mut local_entries = read_valid_source_entries(&local_paths.source_log_path)?;

    if !shared_sync::shared_root_available(shared_paths) {
        return Ok(local_entries);
    }

    let shared_is_empty =
        shared_sync::shared_event_store_is_empty(shared_paths).map_err(|error| {
            ValveLogError::new(
                "shared_log_read_failed",
                "The shared valve log could not be read.",
            )
            .with_detail(error)
        })?;

    if shared_is_empty {
        if !local_entries.is_empty() {
            shared_sync::restore_shared_events_from_local_entries(
                shared_paths,
                &local_paths.client_id,
                &local_entries,
            )
            .map_err(map_shared_restore_error)?;
        }

        return Ok(local_entries);
    }

    let shared_entries = shared_sync::load_shared_event_entries(shared_paths).map_err(|error| {
        ValveLogError::new(
            "shared_log_read_failed",
            "The shared valve log could not be read.",
        )
        .with_detail(error)
    })?;

    if append_missing_source_entries(local_paths, &local_entries, &shared_entries)? {
        local_entries = read_valid_source_entries(&local_paths.source_log_path)?;
    }

    Ok(local_entries)
}

fn append_missing_source_entries(
    local_paths: &LocalLogPaths,
    local_entries: &[ValveLogEntry],
    shared_entries: &[ValveLogEntry],
) -> Result<bool, ValveLogError> {
    let mut local_ids = local_entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<HashSet<_>>();
    let mut appended = false;

    for entry in shared_entries {
        if local_ids.contains(&entry.id) {
            continue;
        }

        ensure_log_directory(local_paths)?;
        append_source_entry(&local_paths.source_log_path, entry).map_err(|error| {
            ValveLogError::new(
                "source_log_write_failed",
                "The shared valve event could not be mirrored locally.",
            )
            .with_detail(error)
        })?;
        local_ids.insert(entry.id.clone());
        appended = true;
    }

    Ok(appended)
}

#[cfg(test)]
fn local_snapshot_from_entries(entries: &[ValveLogEntry]) -> ValveStateSnapshot {
    if let Some(entry) = entries.last() {
        if let Some(state) = LoggedValveState::from_new_state(&entry.new_state) {
            return ValveStateSnapshot {
                state: state.as_lower_str().to_string(),
                assumed: false,
                last_entry: Some(entry.clone()),
                shared_available: false,
                saved_locally_only: false,
                shared_sync_status: String::new(),
                last_shared_update: None,
                sync_message: String::new(),
            };
        }
    }

    assumed_open_snapshot()
}

fn normalize_operator_name(operator_name: &str) -> Result<String, ValveLogError> {
    let trimmed = operator_name.trim();

    if trimmed.is_empty() {
        return Err(ValveLogError::new(
            "blank_operator_name",
            "Operator name is required.",
        ));
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
fn current_state_from_source(path: &Path) -> Result<ValveStateSnapshot, ValveLogError> {
    let entries = read_valid_source_entries(path)?;
    Ok(local_snapshot_from_entries(&entries))
}

#[cfg(test)]
fn assumed_open_snapshot() -> ValveStateSnapshot {
    ValveStateSnapshot {
        state: "open".to_string(),
        assumed: true,
        last_entry: None,
        shared_available: false,
        saved_locally_only: false,
        shared_sync_status: String::new(),
        last_shared_update: None,
        sync_message: String::new(),
    }
}

fn entry_for_action(action: ValveAction, operator_name: String) -> ValveLogEntry {
    let local_now = Local::now();
    let utc_now = Utc::now();
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());

    ValveLogEntry {
        id: Uuid::new_v4().to_string(),
        logged_at_local: local_now.format("%Y-%m-%d %H:%M:%S").to_string(),
        logged_at_utc: Some(utc_now.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        timezone: Some(timezone),
        valve: VALVE_NAME.to_string(),
        action: action.action_label().to_string(),
        previous_state: action.previous_state().as_display_str().to_string(),
        new_state: action.new_state().as_display_str().to_string(),
        operator_name,
        source: MANUAL_SOURCE.to_string(),
        notes: Some(action.note().to_string()),
    }
}

fn excel_action_label(action: &str) -> String {
    match action {
        LEGACY_CLOSE_ACTION | CLOSE_ACTION => CLOSE_ACTION.to_string(),
        LEGACY_OPEN_ACTION | OPEN_ACTION => OPEN_ACTION.to_string(),
        other => other.to_string(),
    }
}

fn append_source_entry(path: &Path, entry: &ValveLogEntry) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|error| error.to_string())?;

    serde_json::to_writer(&mut file, entry).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;

    Ok(())
}

fn read_valid_source_entries(path: &Path) -> Result<Vec<ValveLogEntry>, ValveLogError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = OpenOptions::new().read(true).open(path).map_err(|error| {
        ValveLogError::new("source_log_read_failed", "The valve log could not be read.")
            .with_detail(error.to_string())
    })?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|error| {
            ValveLogError::new("source_log_read_failed", "The valve log could not be read.")
                .with_detail(error.to_string())
        })?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        let Ok(entry) = serde_json::from_str::<ValveLogEntry>(trimmed) else {
            continue;
        };

        if LoggedValveState::from_new_state(&entry.new_state).is_some() {
            entries.push(entry);
        }
    }

    Ok(entries)
}

#[cfg(test)]
fn read_source_entries(path: &Path) -> Result<Vec<ValveLogEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| error.to_string())?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        let entry = serde_json::from_str::<ValveLogEntry>(trimmed)
            .map_err(|error| format!("line {}: {error}", index + 1))?;
        entries.push(entry);
    }

    Ok(entries)
}

fn refresh_workbook(workbook_path: &Path, entries: &[ValveLogEntry]) -> Result<(), String> {
    write_workbook(workbook_path, entries)
}

fn write_workbook(path: &Path, entries: &[ValveLogEntry]) -> Result<(), String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    worksheet
        .set_name("Valve Log")
        .map_err(|error| error.to_string())?;

    let header_format = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x292928))
        .set_font_color(Color::White)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0x454542));

    let data_format = Format::new()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD8D2C8))
        .set_align(FormatAlign::VerticalCenter);

    let alt_data_format = Format::new()
        .set_background_color(Color::RGB(0xF7F6F4))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD8D2C8))
        .set_align(FormatAlign::VerticalCenter);

    let close_action_format = Format::new()
        .set_bold()
        .set_font_color(Color::RGB(0x1D7F47))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD8D2C8))
        .set_align(FormatAlign::VerticalCenter);

    let open_action_format = Format::new()
        .set_bold()
        .set_font_color(Color::RGB(0x1F74AE))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xD8D2C8))
        .set_align(FormatAlign::VerticalCenter);

    let headers = ["Timestamp", "Valve", "Action", "Operator"];

    worksheet
        .set_row_height(0, 22.0)
        .map_err(|error| error.to_string())?;

    for (column, header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *header, &header_format)
            .map_err(|error| error.to_string())?;
    }

    for (index, entry) in entries.iter().enumerate() {
        let row = (index + 1) as u32;
        let action_label = excel_action_label(&entry.action);
        let row_format = if index % 2 == 0 {
            &data_format
        } else {
            &alt_data_format
        };
        let action_format = match action_label.as_str() {
            CLOSE_ACTION => &close_action_format,
            OPEN_ACTION => &open_action_format,
            _ => row_format,
        };

        worksheet
            .write_string_with_format(row, 0, &entry.logged_at_local, row_format)
            .map_err(|error| error.to_string())?;
        worksheet
            .write_string_with_format(row, 1, &entry.valve, row_format)
            .map_err(|error| error.to_string())?;
        worksheet
            .write_string_with_format(row, 2, &action_label, action_format)
            .map_err(|error| error.to_string())?;
        worksheet
            .write_string_with_format(row, 3, &entry.operator_name, row_format)
            .map_err(|error| error.to_string())?;
    }

    worksheet
        .set_column_width(0, 21.0)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_column_width(1, 24.0)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_column_width(2, 16.0)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_column_width(3, 18.0)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|error| error.to_string())?;

    if !entries.is_empty() {
        worksheet
            .autofilter(0, 0, entries.len() as u32, 3)
            .map_err(|error| error.to_string())?;
    }

    workbook.save(path).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn open_folder_with_default_app(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("explorer")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn open_folder_with_default_app(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_folder_with_default_app(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, io::Read};
    use zip::ZipArchive;

    fn temp_log_dir() -> PathBuf {
        std::env::temp_dir().join(format!("nitrogen-valve-log-test-{}", Uuid::new_v4()))
    }

    fn test_paths() -> LocalLogPaths {
        let log_dir = temp_log_dir();

        LocalLogPaths {
            source_log_path: log_dir.join("events.jsonl"),
            workbook_path: log_dir.join("log.xlsx"),
            log_dir,
            client_id: "test-client".to_string(),
        }
    }

    fn log_valve_action_local_only(
        paths: &LocalLogPaths,
        action: ValveAction,
        operator_name: String,
    ) -> Result<ValveLogEntry, ValveLogError> {
        let operator_name = normalize_operator_name(&operator_name)?;
        let snapshot = current_state_from_source(&paths.source_log_path)?;

        validate_transition(action, &snapshot)?;
        ensure_log_directory(paths)?;

        let entry = entry_for_action(action, operator_name);
        append_source_entry(&paths.source_log_path, &entry).map_err(|error| {
            ValveLogError::new(
                "source_log_write_failed",
                "The valve event could not be saved.",
            )
            .with_detail(error)
        })?;

        Ok(entry)
    }

    fn append_entry(path: &Path, entry: &ValveLogEntry) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create test dir");
        append_source_entry(path, entry).expect("append entry");
    }

    fn workbook_shared_strings(path: &Path) -> String {
        let file = File::open(path).expect("open workbook");
        let mut archive = ZipArchive::new(file).expect("read workbook zip");
        let mut shared_strings = String::new();

        archive
            .by_name("xl/sharedStrings.xml")
            .expect("shared strings")
            .read_to_string(&mut shared_strings)
            .expect("read shared strings");

        shared_strings
    }

    #[test]
    fn blank_operator_name_is_rejected() {
        let error = normalize_operator_name("   ").expect_err("blank name should fail");
        let dto = ValveLogErrorDto::from(error);

        assert_eq!(dto.code, "blank_operator_name");
        assert!(!dto.event_saved);
    }

    #[test]
    fn no_jsonl_file_returns_assumed_open_state() {
        let paths = test_paths();
        let snapshot = current_state_from_source(&paths.source_log_path).expect("snapshot");

        assert_eq!(snapshot.state, "open");
        assert!(snapshot.assumed);
        assert_eq!(snapshot.last_entry, None);
    }

    #[test]
    fn jsonl_file_with_no_valid_entries_returns_assumed_open_state() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        fs::write(
            &paths.source_log_path,
            "not json\n{\"new_state\":\"Unknown\"}\n",
        )
        .expect("write source log");

        let snapshot = current_state_from_source(&paths.source_log_path).expect("snapshot");

        assert_eq!(snapshot.state, "open");
        assert!(snapshot.assumed);
        assert_eq!(snapshot.last_entry, None);
    }

    #[test]
    fn latest_close_entry_returns_closed_state() {
        let paths = test_paths();
        let entry = entry_for_action(ValveAction::Close, "Sean".to_string());
        append_entry(&paths.source_log_path, &entry);

        let snapshot = current_state_from_source(&paths.source_log_path).expect("snapshot");

        assert_eq!(snapshot.state, "closed");
        assert!(!snapshot.assumed);
        assert_eq!(snapshot.last_entry, Some(entry));
    }

    #[test]
    fn latest_open_entry_returns_open_state() {
        let paths = test_paths();
        append_entry(
            &paths.source_log_path,
            &entry_for_action(ValveAction::Close, "Sean".to_string()),
        );
        let open_entry = entry_for_action(ValveAction::Open, "Long".to_string());
        append_entry(&paths.source_log_path, &open_entry);

        let snapshot = current_state_from_source(&paths.source_log_path).expect("snapshot");

        assert_eq!(snapshot.state, "open");
        assert!(!snapshot.assumed);
        assert_eq!(snapshot.last_entry, Some(open_entry));
    }

    #[test]
    fn source_entries_append_and_read_back() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        let entry = entry_for_action(ValveAction::Close, "Sean".to_string());

        append_source_entry(&paths.source_log_path, &entry).expect("append entry");
        let entries = read_source_entries(&paths.source_log_path).expect("read entries");

        assert_eq!(entries, vec![entry]);
    }

    #[test]
    fn missing_shared_entries_are_mirrored_to_local_source_log() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        let close_entry = entry_for_action(ValveAction::Close, "Sean".to_string());
        append_source_entry(&paths.source_log_path, &close_entry).expect("append local entry");
        let open_entry = entry_for_action(ValveAction::Open, "Long".to_string());
        let local_entries = read_valid_source_entries(&paths.source_log_path).expect("read local");

        let appended = append_missing_source_entries(
            &paths,
            &local_entries,
            &[close_entry.clone(), open_entry.clone()],
        )
        .expect("mirror shared entries");
        let entries = read_source_entries(&paths.source_log_path).expect("read entries");

        assert!(appended);
        assert_eq!(entries, vec![close_entry, open_entry]);
    }

    #[test]
    fn existing_shared_entries_are_not_duplicated_in_local_source_log() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        let close_entry = entry_for_action(ValveAction::Close, "Sean".to_string());
        append_source_entry(&paths.source_log_path, &close_entry).expect("append local entry");
        let local_entries = read_valid_source_entries(&paths.source_log_path).expect("read local");

        let appended = append_missing_source_entries(
            &paths,
            &local_entries,
            std::slice::from_ref(&close_entry),
        )
        .expect("mirror shared entries");
        let entries = read_source_entries(&paths.source_log_path).expect("read entries");

        assert!(!appended);
        assert_eq!(entries, vec![close_entry]);
    }

    #[test]
    fn close_is_allowed_from_open() {
        let paths = test_paths();
        let entry = log_valve_action_local_only(&paths, ValveAction::Close, "Sean".to_string())
            .expect("log");

        assert_eq!(entry.previous_state, OPEN_STATE);
        assert_eq!(entry.new_state, CLOSED_STATE);
        assert_eq!(
            current_state_from_source(&paths.source_log_path)
                .expect("snapshot")
                .state,
            "closed"
        );
    }

    #[test]
    fn open_is_allowed_from_closed() {
        let paths = test_paths();
        log_valve_action_local_only(&paths, ValveAction::Close, "Sean".to_string())
            .expect("close log");
        let entry = log_valve_action_local_only(&paths, ValveAction::Open, "Long".to_string())
            .expect("open");

        assert_eq!(entry.previous_state, CLOSED_STATE);
        assert_eq!(entry.new_state, OPEN_STATE);
        assert_eq!(
            current_state_from_source(&paths.source_log_path)
                .expect("snapshot")
                .state,
            "open"
        );
    }

    #[test]
    fn duplicate_close_is_rejected_when_latest_state_is_closed() {
        let paths = test_paths();
        log_valve_action_local_only(&paths, ValveAction::Close, "Sean".to_string())
            .expect("close log");

        let error = log_valve_action_local_only(&paths, ValveAction::Close, "Long".to_string())
            .expect_err("duplicate close should fail");
        let dto = ValveLogErrorDto::from(error);

        assert_eq!(dto.code, "invalid_transition");
        assert_eq!(
            dto.message,
            "The valve log is already marked closed. Refresh the app and try again."
        );
        assert!(!dto.event_saved);
    }

    #[test]
    fn duplicate_open_is_rejected_when_latest_state_is_open() {
        let paths = test_paths();
        log_valve_action_local_only(&paths, ValveAction::Close, "Sean".to_string())
            .expect("close log");
        log_valve_action_local_only(&paths, ValveAction::Open, "Long".to_string())
            .expect("open log");

        let error = log_valve_action_local_only(&paths, ValveAction::Open, "Jose".to_string())
            .expect_err("duplicate open should fail");
        let dto = ValveLogErrorDto::from(error);

        assert_eq!(dto.code, "invalid_transition");
        assert_eq!(
            dto.message,
            "The valve log is already marked open. Refresh the app and try again."
        );
        assert!(!dto.event_saved);
    }

    #[test]
    fn workbook_is_created_with_headers() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        let entry = entry_for_action(ValveAction::Close, "Long".to_string());

        write_workbook(&paths.workbook_path, &[entry]).expect("write workbook");

        assert!(paths.workbook_path.exists());
        assert!(fs::metadata(paths.workbook_path).expect("metadata").len() > 0);
    }

    #[test]
    fn empty_workbook_is_created_with_headers_only() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");

        write_workbook(&paths.workbook_path, &[]).expect("write workbook");

        assert!(paths.workbook_path.exists());
        let shared_strings = workbook_shared_strings(&paths.workbook_path);
        assert!(shared_strings.contains("Timestamp"));
        assert!(shared_strings.contains("Operator"));
    }

    #[test]
    fn workbook_generation_includes_open_and_close_rows() {
        let paths = test_paths();
        fs::create_dir_all(&paths.log_dir).expect("create test dir");
        let close_entry = entry_for_action(ValveAction::Close, "Sean".to_string());
        let open_entry = entry_for_action(ValveAction::Open, "Long".to_string());

        write_workbook(&paths.workbook_path, &[close_entry, open_entry]).expect("write workbook");
        let shared_strings = workbook_shared_strings(&paths.workbook_path);

        assert!(shared_strings.contains(CLOSE_ACTION));
        assert!(shared_strings.contains(OPEN_ACTION));
        assert!(shared_strings.contains("Timestamp"));
        assert!(shared_strings.contains("Operator"));
        assert!(!shared_strings.contains("Previous State"));
        assert!(!shared_strings.contains("New State"));
        assert!(!shared_strings.contains("Source"));
        assert!(!shared_strings.contains(CLOSE_NOTE));
        assert!(!shared_strings.contains(OPEN_NOTE));
    }

    #[test]
    fn legacy_actions_are_normalized_for_excel_output() {
        let legacy_close = ValveLogEntry {
            id: "1".to_string(),
            logged_at_local: "2026-06-24 10:00:00".to_string(),
            logged_at_utc: None,
            timezone: None,
            valve: VALVE_NAME.to_string(),
            action: LEGACY_CLOSE_ACTION.to_string(),
            previous_state: OPEN_STATE.to_string(),
            new_state: CLOSED_STATE.to_string(),
            operator_name: "Sean".to_string(),
            source: MANUAL_SOURCE.to_string(),
            notes: Some(CLOSE_NOTE.to_string()),
        };

        assert_eq!(excel_action_label(&legacy_close.action), CLOSE_ACTION);
    }
}
