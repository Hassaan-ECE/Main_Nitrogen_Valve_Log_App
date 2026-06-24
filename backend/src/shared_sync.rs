use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::{
    env, fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime},
};
use uuid::Uuid;

use crate::model::{ValveLogEntry, ValveStateSnapshot};

pub(crate) const SHARED_ROOT_ENV: &str = "NITROGEN_VALVE_LOG_SHARED_ROOT";
pub(crate) const DEFAULT_SHARED_ROOT: &str =
    r"S:\Engineering\Public\Syed_Hassaan_Shah\Main_Nitrogen_Valve_Log_App";
const SHARED_DIR_NAME: &str = "shared";
const STATE_FILE_NAME: &str = "state.json";
const EVENTS_DIR_NAME: &str = "events";
const LOCK_DIR_NAME: &str = ".lock";
const CLIENT_ID_FILE_NAME: &str = "client_id.txt";
const VALVE_NAME: &str = "Main Nitrogen Valve";
const CLOSE_ACTION: &str = "Closed Valve";
const OPEN_ACTION: &str = "Opened Valve";
const OPEN_STATE: &str = "Open";
const CLOSED_STATE: &str = "Closed";
const MANUAL_SOURCE: &str = "Manual";
const LOCK_RETRY_INTERVAL_MS: u64 = 50;
const LOCK_MAX_WAIT_MS: u64 = 2_000;

#[derive(Clone, Debug)]
pub(crate) struct SharedSyncPaths {
    pub shared_root: PathBuf,
    pub shared_dir: PathBuf,
    pub state_path: PathBuf,
    pub events_dir: PathBuf,
    pub lock_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SharedSyncError {
    Busy,
    Message(String),
}

impl fmt::Display for SharedSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => write!(
                formatter,
                "Another operator is logging a valve event. Try again."
            ),
            Self::Message(message) => write!(formatter, "{message}"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CompactState {
    s: String,
    op: String,
    at: String,
    id: String,
    a: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    u: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tz: Option<String>,
    #[serde(default)]
    assumed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CompactEvent {
    id: String,
    at: String,
    a: String,
    op: String,
    ps: String,
    ns: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    u: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tz: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoggedValveState {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValveActionKind {
    Close,
    Open,
}

struct SharedWriteLock {
    lock_path: PathBuf,
}

impl Drop for SharedWriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.lock_path);
    }
}

pub(crate) fn resolve_shared_paths() -> SharedSyncPaths {
    let shared_root = resolve_shared_root();
    let shared_dir = shared_root.join(SHARED_DIR_NAME);

    SharedSyncPaths {
        state_path: shared_dir.join(STATE_FILE_NAME),
        events_dir: shared_dir.join(EVENTS_DIR_NAME),
        lock_path: shared_dir.join(LOCK_DIR_NAME),
        shared_root,
        shared_dir,
    }
}

pub(crate) fn shared_root_available(paths: &SharedSyncPaths) -> bool {
    paths.shared_root.exists()
}

pub(crate) fn client_id(app_data_dir: &Path) -> Result<String, String> {
    fs::create_dir_all(app_data_dir).map_err(|error| error.to_string())?;
    let client_id_path = app_data_dir.join(CLIENT_ID_FILE_NAME);

    if let Ok(existing) = fs::read_to_string(&client_id_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let generated = Uuid::new_v4().to_string();
    write_atomic(&client_id_path, generated.as_bytes())?;
    Ok(generated)
}

pub(crate) fn merged_canonical_entries(
    local_entries: &[ValveLogEntry],
    paths: &SharedSyncPaths,
) -> Result<Vec<ValveLogEntry>, String> {
    let shared_entries = if shared_root_available(paths) {
        load_shared_events(&paths.events_dir)?
    } else {
        Vec::new()
    };

    Ok(merge_canonical_entries(collect_entries(
        local_entries,
        &shared_entries,
    )))
}

pub(crate) fn compute_merged_snapshot(
    local_entries: &[ValveLogEntry],
    paths: &SharedSyncPaths,
) -> Result<ValveStateSnapshot, String> {
    let canonical = merged_canonical_entries(local_entries, paths)?;
    Ok(enrich_snapshot(
        snapshot_from_canonical(&canonical),
        paths,
        None,
    ))
}

pub(crate) fn load_merged_snapshot(
    local_entries: &[ValveLogEntry],
    paths: &SharedSyncPaths,
) -> Result<ValveStateSnapshot, String> {
    let snapshot = compute_merged_snapshot(local_entries, paths)?;

    if shared_root_available(paths) {
        let _ = write_shared_state(paths, &snapshot);
    }

    Ok(snapshot)
}

pub(crate) fn load_fast_snapshot(
    local_entries: &[ValveLogEntry],
    paths: &SharedSyncPaths,
) -> Result<ValveStateSnapshot, String> {
    let shared_available = shared_root_available(paths);

    if shared_available {
        if let Ok(snapshot) = read_shared_state(paths) {
            let last_shared_update = snapshot.last_shared_update.clone();
            return Ok(enrich_snapshot(snapshot, paths, last_shared_update));
        }
    }

    load_merged_snapshot(local_entries, paths)
}

pub(crate) fn commit_shared_valve_event<F>(
    paths: &SharedSyncPaths,
    client_id: &str,
    prepare: F,
) -> Result<(ValveLogEntry, Vec<ValveLogEntry>), SharedSyncError>
where
    F: FnOnce() -> Result<(ValveLogEntry, Vec<ValveLogEntry>), SharedSyncError>,
{
    if !shared_root_available(paths) {
        return Err(SharedSyncError::Message(
            "Shared sync root is unavailable.".to_string(),
        ));
    }

    fs::create_dir_all(&paths.shared_dir)
        .map_err(|error| SharedSyncError::Message(error.to_string()))?;
    fs::create_dir_all(&paths.events_dir)
        .map_err(|error| SharedSyncError::Message(error.to_string()))?;

    let _lock = acquire_shared_write_lock(&paths.lock_path)?;
    let (entry, local_entries) = prepare()?;

    let event_path = unique_event_path(&paths.events_dir, client_id);
    let compact = compact_from_entry(&entry);
    let payload = serde_json::to_vec(&compact)
        .map_err(|error| SharedSyncError::Message(error.to_string()))?;
    write_atomic(&event_path, &payload).map_err(SharedSyncError::Message)?;

    let snapshot =
        compute_merged_snapshot(&local_entries, paths).map_err(SharedSyncError::Message)?;
    write_shared_state(paths, &snapshot).map_err(SharedSyncError::Message)?;

    let _ = enrich_snapshot(
        snapshot,
        paths,
        Some(entry.logged_at_local.clone()),
    );

    Ok((entry, local_entries))
}

fn acquire_shared_write_lock(lock_path: &Path) -> Result<SharedWriteLock, SharedSyncError> {
    let started = Instant::now();

    loop {
        match fs::create_dir(lock_path) {
            Ok(()) => {
                return Ok(SharedWriteLock {
                    lock_path: lock_path.to_path_buf(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if started.elapsed() >= Duration::from_millis(LOCK_MAX_WAIT_MS) {
                    return Err(SharedSyncError::Busy);
                }
                thread::sleep(Duration::from_millis(LOCK_RETRY_INTERVAL_MS));
            }
            Err(error) => return Err(SharedSyncError::Message(error.to_string())),
        }
    }
}

fn enrich_snapshot(
    mut snapshot: ValveStateSnapshot,
    paths: &SharedSyncPaths,
    last_shared_update: Option<String>,
) -> ValveStateSnapshot {
    let shared_available = shared_root_available(paths);
    snapshot.shared_available = shared_available;
    snapshot.saved_locally_only = !shared_available || snapshot.saved_locally_only;
    snapshot.last_shared_update = last_shared_update.or_else(|| {
        snapshot
            .last_entry
            .as_ref()
            .map(|entry| entry.logged_at_local.clone())
    });

    if !shared_available {
        snapshot.shared_sync_status = "unavailable".to_string();
        snapshot.sync_message = "Shared sync unavailable — event saved locally only.".to_string();
        return snapshot;
    }

    if snapshot.saved_locally_only {
        snapshot.shared_sync_status = "local_only".to_string();
        snapshot.sync_message = "Shared sync unavailable — event saved locally only.".to_string();
        return snapshot;
    }

    snapshot.shared_sync_status = "connected".to_string();
    snapshot.sync_message = match &snapshot.last_shared_update {
        Some(timestamp) => format!("Shared sync: Connected. Last shared update: {timestamp}"),
        None => "Shared sync: Connected.".to_string(),
    };
    snapshot
}

fn resolve_shared_root() -> PathBuf {
    env::var_os(SHARED_ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SHARED_ROOT))
}

fn collect_entries(
    local_entries: &[ValveLogEntry],
    shared_entries: &[ValveLogEntry],
) -> Vec<ValveLogEntry> {
    let mut entries = Vec::with_capacity(local_entries.len() + shared_entries.len());
    entries.extend_from_slice(local_entries);
    entries.extend_from_slice(shared_entries);
    entries
}

fn load_shared_events(events_dir: &Path) -> Result<Vec<ValveLogEntry>, String> {
    if !events_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    collect_event_files(events_dir, &mut entries)?;
    entries.sort_by(compare_logged_at);
    Ok(entries)
}

fn collect_event_files(dir: &Path, entries: &mut Vec<ValveLogEntry>) -> Result<(), String> {
    let read_dir = fs::read_dir(dir).map_err(|error| error.to_string())?;

    for item in read_dir {
        let item = item.map_err(|error| error.to_string())?;
        let path = item.path();

        if path.is_dir() {
            collect_event_files(&path, entries)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        let compact: CompactEvent = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        entries.push(entry_from_compact(compact));
    }

    Ok(())
}

fn merge_canonical_entries(entries: Vec<ValveLogEntry>) -> Vec<ValveLogEntry> {
    let mut sorted = entries
        .into_iter()
        .filter(|entry| LoggedValveState::from_new_state(&entry.new_state).is_some())
        .collect::<Vec<_>>();
    sorted.sort_by(compare_logged_at);

    let mut canonical = Vec::new();
    let mut current_state = LoggedValveState::Open;

    for entry in sorted {
        let Some(new_state) = LoggedValveState::from_new_state(&entry.new_state) else {
            continue;
        };
        let action = action_kind_from_entry(&entry);

        match (current_state, new_state, action) {
            (LoggedValveState::Open, LoggedValveState::Closed, Some(ValveActionKind::Close)) => {
                current_state = LoggedValveState::Closed;
                canonical.push(entry);
            }
            (LoggedValveState::Closed, LoggedValveState::Open, Some(ValveActionKind::Open)) => {
                current_state = LoggedValveState::Open;
                canonical.push(entry);
            }
            (LoggedValveState::Closed, LoggedValveState::Closed, Some(ValveActionKind::Close)) => {
                replace_duplicate_close(&mut canonical, entry);
            }
            (LoggedValveState::Open, LoggedValveState::Open, Some(ValveActionKind::Open)) => {
                replace_duplicate_open(&mut canonical, entry);
            }
            _ => {}
        }
    }

    canonical
}

fn replace_duplicate_close(canonical: &mut Vec<ValveLogEntry>, entry: ValveLogEntry) {
    let Some(index) = canonical
        .iter()
        .rposition(|existing| action_kind_from_entry(existing) == Some(ValveActionKind::Close))
    else {
        canonical.push(entry);
        return;
    };

    let existing = &canonical[index];
    if compare_logged_at(&entry, existing) == std::cmp::Ordering::Greater {
        canonical[index] = entry;
    }
}

fn replace_duplicate_open(canonical: &mut Vec<ValveLogEntry>, entry: ValveLogEntry) {
    let Some(index) = canonical
        .iter()
        .rposition(|existing| action_kind_from_entry(existing) == Some(ValveActionKind::Open))
    else {
        canonical.push(entry);
        return;
    };

    let existing = &canonical[index];
    if compare_logged_at(&entry, existing) == std::cmp::Ordering::Less {
        canonical[index] = entry;
    }
}

fn snapshot_from_canonical(canonical: &[ValveLogEntry]) -> ValveStateSnapshot {
    if let Some(entry) = canonical.last() {
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

fn read_shared_state(paths: &SharedSyncPaths) -> Result<ValveStateSnapshot, String> {
    let bytes = fs::read(&paths.state_path).map_err(|error| error.to_string())?;
    let compact: CompactState =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(snapshot_from_compact(compact))
}

fn write_shared_state(
    paths: &SharedSyncPaths,
    snapshot: &ValveStateSnapshot,
) -> Result<(), String> {
    fs::create_dir_all(&paths.shared_dir).map_err(|error| error.to_string())?;
    let compact = compact_from_snapshot(snapshot);
    let payload = serde_json::to_vec(&compact).map_err(|error| error.to_string())?;
    write_atomic(&paths.state_path, &payload)
}

fn snapshot_from_compact(compact: CompactState) -> ValveStateSnapshot {
    let state = compact.s.trim().to_ascii_lowercase();
    let last_shared_update = if compact.assumed {
        None
    } else {
        Some(compact.at.clone())
    };
    let last_entry = if compact.assumed {
        None
    } else {
        let action = compact.a;
        Some(ValveLogEntry {
            id: compact.id,
            logged_at_local: compact.at,
            logged_at_utc: compact.u,
            timezone: compact.tz,
            valve: VALVE_NAME.to_string(),
            previous_state: previous_state_for_action_label(&action),
            new_state: display_state_from_lower(&state),
            operator_name: compact.op,
            action,
            source: MANUAL_SOURCE.to_string(),
            notes: None,
        })
    };

    ValveStateSnapshot {
        state,
        assumed: compact.assumed,
        last_entry,
        shared_available: true,
        saved_locally_only: false,
        shared_sync_status: String::new(),
        last_shared_update,
        sync_message: String::new(),
    }
}

fn compact_from_snapshot(snapshot: &ValveStateSnapshot) -> CompactState {
    if let Some(entry) = &snapshot.last_entry {
        return CompactState {
            s: snapshot.state.clone(),
            op: entry.operator_name.clone(),
            at: entry.logged_at_local.clone(),
            id: entry.id.clone(),
            a: entry.action.clone(),
            u: entry.logged_at_utc.clone(),
            tz: entry.timezone.clone(),
            assumed: snapshot.assumed,
        };
    }

    CompactState {
        s: snapshot.state.clone(),
        op: String::new(),
        at: String::new(),
        id: String::new(),
        a: String::new(),
        u: None,
        tz: None,
        assumed: snapshot.assumed,
    }
}

fn compact_from_entry(entry: &ValveLogEntry) -> CompactEvent {
    CompactEvent {
        id: entry.id.clone(),
        at: entry.logged_at_local.clone(),
        a: compact_action_code(&entry.action),
        op: entry.operator_name.clone(),
        ps: entry.previous_state.clone(),
        ns: entry.new_state.clone(),
        u: entry.logged_at_utc.clone(),
        tz: entry.timezone.clone(),
    }
}

fn entry_from_compact(compact: CompactEvent) -> ValveLogEntry {
    ValveLogEntry {
        id: compact.id,
        logged_at_local: compact.at,
        logged_at_utc: compact.u,
        timezone: compact.tz,
        valve: VALVE_NAME.to_string(),
        action: action_label_from_code(&compact.a),
        previous_state: compact.ps,
        new_state: compact.ns,
        operator_name: compact.op,
        source: MANUAL_SOURCE.to_string(),
        notes: None,
    }
}

fn compact_action_code(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "closed valve" | "close valve" | "c" => "c".to_string(),
        "opened valve" | "open valve" | "o" => "o".to_string(),
        other => other.to_string(),
    }
}

fn action_label_from_code(code: &str) -> String {
    match code.trim().to_ascii_lowercase().as_str() {
        "c" | "close" | "closed valve" | "close valve" => CLOSE_ACTION.to_string(),
        "o" | "open" | "opened valve" | "open valve" => OPEN_ACTION.to_string(),
        other => other.to_string(),
    }
}

fn action_kind_from_entry(entry: &ValveLogEntry) -> Option<ValveActionKind> {
    match entry.action.trim().to_ascii_lowercase().as_str() {
        "closed valve" | "close valve" | "c" => Some(ValveActionKind::Close),
        "opened valve" | "open valve" | "o" => Some(ValveActionKind::Open),
        _ => LoggedValveState::from_new_state(&entry.new_state).map(|state| match state {
            LoggedValveState::Closed => ValveActionKind::Close,
            LoggedValveState::Open => ValveActionKind::Open,
        }),
    }
}

fn previous_state_for_action_label(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "closed valve" | "close valve" | "c" => OPEN_STATE.to_string(),
        _ => CLOSED_STATE.to_string(),
    }
}

fn display_state_from_lower(state: &str) -> String {
    match state {
        "closed" => CLOSED_STATE.to_string(),
        _ => OPEN_STATE.to_string(),
    }
}

fn compare_logged_at(left: &ValveLogEntry, right: &ValveLogEntry) -> std::cmp::Ordering {
    parse_sortable_timestamp(left)
        .cmp(&parse_sortable_timestamp(right))
        .then_with(|| left.logged_at_local.cmp(&right.logged_at_local))
        .then_with(|| left.id.cmp(&right.id))
}

fn parse_sortable_timestamp(entry: &ValveLogEntry) -> NaiveDateTime {
    if let Some(utc) = entry.logged_at_utc.as_deref() {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(utc.trim(), "%Y-%m-%dT%H:%M:%SZ") {
            return parsed;
        }
    }

    parse_logged_at(&entry.logged_at_local)
}

fn parse_logged_at(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S").unwrap_or_default()
}

fn unique_event_path(events_dir: &Path, client_id: &str) -> PathBuf {
    let client_dir = events_dir.join(client_id);
    let millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    client_dir.join(format!("{millis}.json"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let temp_path = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&temp_path).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
    }

    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }

    fs::rename(&temp_path, path).map_err(|error| error.to_string())
}

impl LoggedValveState {
    fn from_new_state(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    fn as_lower_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(action: &str, new_state: &str, at: &str, operator: &str) -> ValveLogEntry {
        ValveLogEntry {
            id: Uuid::new_v4().to_string(),
            logged_at_local: at.to_string(),
            logged_at_utc: None,
            timezone: None,
            valve: VALVE_NAME.to_string(),
            action: action.to_string(),
            previous_state: if new_state == CLOSED_STATE {
                OPEN_STATE.to_string()
            } else {
                CLOSED_STATE.to_string()
            },
            new_state: new_state.to_string(),
            operator_name: operator.to_string(),
            source: MANUAL_SOURCE.to_string(),
            notes: None,
        }
    }

    fn test_paths(root: PathBuf) -> SharedSyncPaths {
        let shared_dir = root.join(SHARED_DIR_NAME);
        SharedSyncPaths {
            shared_root: root.clone(),
            shared_dir: shared_dir.clone(),
            state_path: shared_dir.join(STATE_FILE_NAME),
            events_dir: shared_dir.join(EVENTS_DIR_NAME),
            lock_path: shared_dir.join(LOCK_DIR_NAME),
        }
    }

    #[test]
    fn merge_replays_close_then_open_in_order() {
        let canonical = merge_canonical_entries(vec![
            entry(CLOSE_ACTION, CLOSED_STATE, "2026-06-24 17:00:00", "Sean"),
            entry(OPEN_ACTION, OPEN_STATE, "2026-06-25 08:00:00", "Long"),
        ]);

        assert_eq!(canonical.len(), 2);
        assert_eq!(snapshot_from_canonical(&canonical).state, "open");
    }

    #[test]
    fn duplicate_close_keeps_later_entry() {
        let canonical = merge_canonical_entries(vec![
            entry(CLOSE_ACTION, CLOSED_STATE, "2026-06-24 17:00:00", "Sean"),
            entry(CLOSE_ACTION, CLOSED_STATE, "2026-06-24 17:05:00", "Long"),
        ]);

        assert_eq!(canonical.len(), 1);
        assert_eq!(canonical[0].operator_name, "Long");
        assert_eq!(snapshot_from_canonical(&canonical).state, "closed");
    }

    #[test]
    fn duplicate_open_keeps_earlier_entry() {
        let canonical = merge_canonical_entries(vec![
            entry(OPEN_ACTION, OPEN_STATE, "2026-06-25 08:05:00", "Long"),
            entry(OPEN_ACTION, OPEN_STATE, "2026-06-25 08:00:00", "Sean"),
        ]);

        assert_eq!(canonical.len(), 1);
        assert_eq!(canonical[0].operator_name, "Sean");
        assert_eq!(snapshot_from_canonical(&canonical).state, "open");
    }

    #[test]
    fn compact_event_round_trips() {
        let mut original = entry(CLOSE_ACTION, CLOSED_STATE, "2026-06-24 17:00:00", "Sean");
        original.logged_at_utc = Some("2026-06-24T22:00:00Z".to_string());
        original.timezone = Some("America/Chicago".to_string());
        let compact = compact_from_entry(&original);
        let restored = entry_from_compact(compact);

        assert_eq!(restored.action, CLOSE_ACTION);
        assert_eq!(restored.operator_name, "Sean");
        assert_eq!(restored.logged_at_utc, original.logged_at_utc);
    }

    #[test]
    fn write_and_read_shared_state_round_trip() {
        let temp_root = std::env::temp_dir().join(format!("valve-shared-test-{}", Uuid::new_v4()));
        let paths = test_paths(temp_root.clone());

        let snapshot = ValveStateSnapshot {
            state: "closed".to_string(),
            assumed: false,
            last_entry: Some(entry(
                CLOSE_ACTION,
                CLOSED_STATE,
                "2026-06-24 17:00:00",
                "Sean",
            )),
            shared_available: true,
            saved_locally_only: false,
            shared_sync_status: String::new(),
            last_shared_update: None,
            sync_message: String::new(),
        };

        write_shared_state(&paths, &snapshot).expect("write state");
        let restored = read_shared_state(&paths).expect("read state");

        assert_eq!(restored.state, "closed");
        assert_eq!(restored.last_entry.expect("entry").operator_name, "Sean");

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn shared_write_lock_can_be_reacquired_after_release() {
        let temp_root = std::env::temp_dir().join(format!("valve-lock-test-{}", Uuid::new_v4()));
        let paths = test_paths(temp_root.clone());
        fs::create_dir_all(&paths.shared_dir).expect("shared dir");

        {
            let _first = acquire_shared_write_lock(&paths.lock_path).expect("first lock");
        }
        let _second = acquire_shared_write_lock(&paths.lock_path).expect("second lock");

        let _ = fs::remove_dir_all(temp_root);
    }
}

