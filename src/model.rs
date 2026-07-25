use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_device: String,
    pub deleted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeEntry {
    pub id: String,
    pub task_id: String,
    pub task_name: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub source_device: String,
    pub deleted: bool,
}

impl TimeEntry {
    pub fn elapsed_seconds_at(&self, now: DateTime<Utc>) -> i64 {
        self.stopped_at
            .unwrap_or(now)
            .signed_duration_since(self.started_at)
            .num_seconds()
            .max(0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncRequest {
    pub device_id: String,
    pub tasks: Vec<Task>,
    pub entries: Vec<TimeEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub device_id: String,
    pub tasks: Vec<Task>,
    pub entries: Vec<TimeEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeSummary {
    pub tasks_applied: usize,
    pub entries_applied: usize,
}
