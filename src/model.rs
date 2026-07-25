use std::collections::HashSet;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_SYNC_TASKS: usize = 10_000;
pub const MAX_SYNC_ENTRIES: usize = 50_000;
pub const MAX_TASK_NAME_BYTES: usize = 512;
pub const MAX_PROJECT_BYTES: usize = 512;
pub const MAX_TAG_BYTES: usize = 128;
pub const MAX_TAGS_PER_RECORD: usize = 32;

const MAX_IDENTIFIER_BYTES: usize = 64;
const MAX_FUTURE_CLOCK_SKEW_MINUTES: i64 = 10;

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

pub fn task_id_for_name(name: &str) -> String {
    let normalized = name.trim().to_lowercase();
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("https://github.com/EasternKentuckyDigital/tracker/task/{normalized}").as_bytes(),
    )
    .to_string()
}

pub fn validate_sync_payload(
    peer_device_id: &str,
    tasks: &[Task],
    entries: &[TimeEntry],
) -> Result<()> {
    validate_uuid("peer device ID", peer_device_id)?;
    validate_sync_records(tasks, entries)
}

pub fn validate_sync_records(tasks: &[Task], entries: &[TimeEntry]) -> Result<()> {
    if tasks.len() > MAX_SYNC_TASKS {
        bail!("sync contains too many tasks (maximum {MAX_SYNC_TASKS})");
    }
    if entries.len() > MAX_SYNC_ENTRIES {
        bail!("sync contains too many time entries (maximum {MAX_SYNC_ENTRIES})");
    }

    let now_limit = Utc::now() + chrono::Duration::minutes(MAX_FUTURE_CLOCK_SKEW_MINUTES);
    let mut task_ids = HashSet::with_capacity(tasks.len());
    for task in tasks {
        validate_uuid("task ID", &task.id)?;
        if !task_ids.insert(task.id.as_str()) {
            bail!("sync contains duplicate task ID {}", task.id);
        }
        validate_required_text("task name", &task.name, MAX_TASK_NAME_BYTES)?;
        if task.id != task_id_for_name(&task.name) {
            bail!("task ID does not match its normalized name");
        }
        validate_optional_text("project", task.project.as_deref(), MAX_PROJECT_BYTES)?;
        validate_tags(&task.tags)?;
        validate_uuid("task source device", &task.source_device)?;
        if task.created_at > task.updated_at {
            bail!("task creation time is later than its update time");
        }
        if task.updated_at > now_limit {
            bail!("task update time is too far in the future");
        }
    }

    let mut entry_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        validate_uuid("time-entry ID", &entry.id)?;
        if !entry_ids.insert(entry.id.as_str()) {
            bail!("sync contains duplicate time-entry ID {}", entry.id);
        }
        validate_uuid("time-entry task ID", &entry.task_id)?;
        if !task_ids.contains(entry.task_id.as_str()) {
            bail!("time entry {} references an unknown task", entry.id);
        }
        validate_required_text(
            "time-entry task name",
            &entry.task_name,
            MAX_TASK_NAME_BYTES,
        )?;
        validate_optional_text(
            "time-entry project",
            entry.project.as_deref(),
            MAX_PROJECT_BYTES,
        )?;
        validate_tags(&entry.tags)?;
        validate_uuid("time-entry source device", &entry.source_device)?;
        if entry.started_at > now_limit
            || entry
                .stopped_at
                .is_some_and(|stopped_at| stopped_at > now_limit)
            || entry.updated_at > now_limit
        {
            bail!("time-entry timestamp is too far in the future");
        }
        if entry
            .stopped_at
            .is_some_and(|stopped_at| stopped_at < entry.started_at)
        {
            bail!("time entry stops before it starts");
        }
        if entry.updated_at < entry.started_at
            || entry
                .stopped_at
                .is_some_and(|stopped_at| entry.updated_at < stopped_at)
        {
            bail!("time-entry update time precedes its recorded activity");
        }
    }

    Ok(())
}

pub fn validate_task_input(name: &str, project: Option<&str>, tags: &[String]) -> Result<()> {
    validate_required_text("task name", name.trim(), MAX_TASK_NAME_BYTES)?;
    validate_optional_text(
        "project",
        project.map(str::trim).filter(|value| !value.is_empty()),
        MAX_PROJECT_BYTES,
    )?;
    validate_tags(tags)
}

fn validate_tags(tags: &[String]) -> Result<()> {
    if tags.len() > MAX_TAGS_PER_RECORD {
        bail!("a record can contain at most {MAX_TAGS_PER_RECORD} tags");
    }
    for tag in tags {
        validate_required_text("tag", tag, MAX_TAG_BYTES)?;
    }
    Ok(())
}

fn validate_required_text(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    if value.len() > max_bytes {
        bail!("{label} is too long (maximum {max_bytes} UTF-8 bytes)");
    }
    if value.chars().any(char::is_control) {
        bail!("{label} cannot contain control characters");
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: Option<&str>, max_bytes: usize) -> Result<()> {
    if let Some(value) = value {
        validate_required_text(label, value, max_bytes)?;
    }
    Ok(())
}

fn validate_uuid(label: &str, value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES || Uuid::parse_str(value).is_err() {
        bail!("{label} is not a valid UUID");
    }
    Ok(())
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackerSnapshot {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub active_entry: Option<TimeEntry>,
    pub tasks: Vec<Task>,
    pub entries: Vec<TimeEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeSummary {
    pub tasks_applied: usize,
    pub entries_applied: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_payload() -> (String, Vec<Task>, Vec<TimeEntry>) {
        let device_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let name = "Security review".to_owned();
        let task_id = task_id_for_name(&name);
        let task = Task {
            id: task_id.clone(),
            name: name.clone(),
            project: Some("Tracker".to_owned()),
            tags: vec!["security".to_owned()],
            created_at: now,
            updated_at: now,
            source_device: device_id.clone(),
            deleted: false,
        };
        let entry = TimeEntry {
            id: Uuid::new_v4().to_string(),
            task_id,
            task_name: name,
            project: Some("Tracker".to_owned()),
            tags: vec!["security".to_owned()],
            started_at: now,
            stopped_at: None,
            updated_at: now,
            source_device: device_id.clone(),
            deleted: false,
        };
        (device_id, vec![task], vec![entry])
    }

    #[test]
    fn accepts_well_formed_sync_records() {
        let (device_id, tasks, entries) = valid_payload();
        validate_sync_payload(&device_id, &tasks, &entries).unwrap();
    }

    #[test]
    fn rejects_future_timestamp_poisoning() {
        let (device_id, tasks, mut entries) = valid_payload();
        entries[0].updated_at = Utc::now() + chrono::Duration::days(365);
        assert!(validate_sync_payload(&device_id, &tasks, &entries).is_err());
    }

    #[test]
    fn rejects_dangling_task_references() {
        let (device_id, tasks, mut entries) = valid_payload();
        entries[0].task_id = Uuid::new_v4().to_string();
        assert!(validate_sync_payload(&device_id, &tasks, &entries).is_err());
    }

    #[test]
    fn rejects_control_characters() {
        assert!(validate_task_input("spoofed\nmessage", None, &[]).is_err());
    }
}
