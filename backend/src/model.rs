use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ValveLogEntry {
    pub id: String,
    pub logged_at_local: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub valve: String,
    pub action: String,
    pub previous_state: String,
    pub new_state: String,
    pub operator_name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValveStateSnapshot {
    pub state: String,
    pub assumed: bool,
    pub last_entry: Option<ValveLogEntry>,
    #[serde(default)]
    pub shared_available: bool,
    #[serde(default)]
    pub saved_locally_only: bool,
    #[serde(default)]
    pub shared_sync_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_shared_update: Option<String>,
    #[serde(default)]
    pub sync_message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValveLogErrorDto {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub event_saved: bool,
    pub entry: Option<ValveLogEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppStatus {
    pub app_name: String,
    pub version: String,
}
