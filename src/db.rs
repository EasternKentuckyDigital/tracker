use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::{MergeSummary, Peer, Task, TimeEntry};

pub struct Database {
    connection: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("could not open {}", path.display()))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn device_id(&self) -> Result<String> {
        if let Some(value) = self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'device_id'",
                [],
                |row| row.get(0),
            )
            .optional()?
        {
            return Ok(value);
        }
        let id = Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES ('device_id', ?1)",
            [&id],
        )?;
        Ok(id)
    }

    pub fn add_task(&mut self, name: &str, project: Option<&str>, tags: &[String]) -> Result<Task> {
        let name = cleaned_required("task name", name)?;
        let project = cleaned_optional(project);
        let tags = normalized_tags(tags);
        let now = Utc::now();
        let device_id = self.device_id()?;

        if let Some(mut existing) = self.task_by_name(&name)? {
            existing.project = project;
            existing.tags = tags;
            existing.updated_at = now;
            existing.source_device = device_id;
            existing.deleted = false;
            self.upsert_task(&existing)?;
            return Ok(existing);
        }

        let task = Task {
            id: task_id(&name),
            name,
            project,
            tags,
            created_at: now,
            updated_at: now,
            source_device: device_id,
            deleted: false,
        };
        self.upsert_task(&task)?;
        Ok(task)
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, project, tags, created_at, updated_at, source_device, deleted
             FROM tasks WHERE deleted = 0 ORDER BY name COLLATE NOCASE",
        )?;
        statement
            .query_map([], task_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read tasks")
    }

    pub fn add_peer(&self, name: &str, url: &str) -> Result<Peer> {
        let name = cleaned_required("peer name", name)?;
        let url = url.trim().trim_end_matches('/').to_owned();
        let parsed = reqwest::Url::parse(&url).context("peer URL is invalid")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("peer URL must use http or https");
        }
        if parsed.host().is_none() {
            bail!("peer URL must include a host");
        }
        self.connection.execute(
            "INSERT INTO peers (name, url) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET url=excluded.url",
            params![name, url],
        )?;
        Ok(Peer { name, url })
    }

    pub fn remove_peer(&self, name: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM peers WHERE name = ?1 COLLATE NOCASE",
            [name.trim()],
        )? > 0)
    }

    pub fn list_peers(&self) -> Result<Vec<Peer>> {
        let mut statement = self
            .connection
            .prepare("SELECT name, url FROM peers ORDER BY name COLLATE NOCASE")?;
        statement
            .query_map([], |row| {
                Ok(Peer {
                    name: row.get(0)?,
                    url: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read peers")
    }

    pub fn start(
        &mut self,
        task_name: &str,
        project: Option<&str>,
        tags: &[String],
    ) -> Result<TimeEntry> {
        if let Some(active) = self.active_entry()? {
            bail!(
                "already tracking “{}” (started {})",
                active.task_name,
                active.started_at.to_rfc3339()
            );
        }

        let known = self.task_by_name(task_name)?;
        let task = match known {
            Some(task) if project.is_none() && tags.is_empty() => task,
            _ => self.add_task(task_name, project, tags)?,
        };
        let now = Utc::now();
        let entry = TimeEntry {
            id: Uuid::new_v4().to_string(),
            task_id: task.id,
            task_name: task.name,
            project: task.project,
            tags: task.tags,
            started_at: now,
            stopped_at: None,
            updated_at: now,
            source_device: self.device_id()?,
            deleted: false,
        };
        self.upsert_entry(&entry)?;
        Ok(entry)
    }

    pub fn stop(&mut self) -> Result<Vec<TimeEntry>> {
        let mut entries = self.active_entries()?;
        if entries.is_empty() {
            bail!("no timer is currently running");
        }
        let now = Utc::now();
        let device_id = self.device_id()?;
        for entry in &mut entries {
            entry.stopped_at = Some(now);
            entry.updated_at = now;
            entry.source_device.clone_from(&device_id);
            self.upsert_entry(entry)?;
        }
        Ok(entries)
    }

    pub fn active_entry(&self) -> Result<Option<TimeEntry>> {
        Ok(self.active_entries()?.into_iter().next())
    }

    fn active_entries(&self) -> Result<Vec<TimeEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, task_name, project, tags, started_at, stopped_at,
                    updated_at, source_device, deleted
             FROM time_entries
             WHERE stopped_at IS NULL AND deleted = 0
             ORDER BY started_at DESC",
        )?;
        statement
            .query_map([], entry_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read active timers")
        /*
         * More than one active row is possible only after independent devices
         * started timers while offline. Keeping all rows allows sync to converge;
         * stop() closes the whole conflict set together.
         */
    }

    pub fn entries_since(&self, since: DateTime<Utc>) -> Result<Vec<TimeEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, task_name, project, tags, started_at, stopped_at,
                    updated_at, source_device, deleted
             FROM time_entries WHERE started_at >= ?1 ORDER BY started_at DESC",
        )?;
        statement
            .query_map([format_time(since)], entry_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read time entries")
    }

    pub fn all_tasks(&self) -> Result<Vec<Task>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, project, tags, created_at, updated_at, source_device, deleted
             FROM tasks ORDER BY id",
        )?;
        statement
            .query_map([], task_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read tasks")
    }

    pub fn all_entries(&self) -> Result<Vec<TimeEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, task_name, project, tags, started_at, stopped_at,
                    updated_at, source_device, deleted
             FROM time_entries ORDER BY id",
        )?;
        statement
            .query_map([], entry_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("could not read time entries")
    }

    pub fn merge(&mut self, tasks: &[Task], entries: &[TimeEntry]) -> Result<MergeSummary> {
        let tx = self.connection.transaction()?;
        let mut summary = MergeSummary::default();
        for task in tasks {
            if should_apply_task(&tx, task)? {
                upsert_task_tx(&tx, task)?;
                summary.tasks_applied += 1;
            }
        }
        for entry in entries {
            if should_apply_entry(&tx, entry)? {
                upsert_entry_tx(&tx, entry)?;
                summary.entries_applied += 1;
            }
        }
        tx.commit()?;
        Ok(summary)
    }

    fn task_by_name(&self, name: &str) -> Result<Option<Task>> {
        self.connection
            .query_row(
                "SELECT id, name, project, tags, created_at, updated_at, source_device, deleted
                 FROM tasks WHERE name = ?1 COLLATE NOCASE AND deleted = 0 LIMIT 1",
                [name.trim()],
                task_from_row,
            )
            .optional()
            .context("could not find task")
    }

    fn upsert_task(&self, task: &Task) -> Result<()> {
        upsert_task_tx(&self.connection, task)
    }

    fn upsert_entry(&self, entry: &TimeEntry) -> Result<()> {
        upsert_entry_tx(&self.connection, entry)
    }
}

fn upsert_task_tx(connection: &Connection, task: &Task) -> Result<()> {
    connection.execute(
        "INSERT INTO tasks
         (id, name, project, tags, created_at, updated_at, source_device, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, project=excluded.project, tags=excluded.tags,
           created_at=excluded.created_at, updated_at=excluded.updated_at,
           source_device=excluded.source_device, deleted=excluded.deleted",
        params![
            task.id,
            task.name,
            task.project,
            encode_tags(&task.tags)?,
            format_time(task.created_at),
            format_time(task.updated_at),
            task.source_device,
            task.deleted
        ],
    )?;
    Ok(())
}

fn upsert_entry_tx(connection: &Connection, entry: &TimeEntry) -> Result<()> {
    connection.execute(
        "INSERT INTO time_entries
         (id, task_id, task_name, project, tags, started_at, stopped_at, updated_at,
          source_device, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
           task_id=excluded.task_id, task_name=excluded.task_name,
           project=excluded.project, tags=excluded.tags, started_at=excluded.started_at,
           stopped_at=excluded.stopped_at, updated_at=excluded.updated_at,
           source_device=excluded.source_device, deleted=excluded.deleted",
        params![
            entry.id,
            entry.task_id,
            entry.task_name,
            entry.project,
            encode_tags(&entry.tags)?,
            format_time(entry.started_at),
            entry.stopped_at.map(format_time),
            format_time(entry.updated_at),
            entry.source_device,
            entry.deleted
        ],
    )?;
    Ok(())
}

fn should_apply_task(connection: &Connection, incoming: &Task) -> Result<bool> {
    let local: Option<(String, String)> = connection
        .query_row(
            "SELECT updated_at, source_device FROM tasks WHERE id = ?1",
            [&incoming.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(is_newer(
        local,
        incoming.updated_at,
        &incoming.source_device,
    ))
}

fn should_apply_entry(connection: &Connection, incoming: &TimeEntry) -> Result<bool> {
    let local: Option<(String, String)> = connection
        .query_row(
            "SELECT updated_at, source_device FROM time_entries WHERE id = ?1",
            [&incoming.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(is_newer(
        local,
        incoming.updated_at,
        &incoming.source_device,
    ))
}

fn is_newer(
    local: Option<(String, String)>,
    incoming_time: DateTime<Utc>,
    incoming_device: &str,
) -> bool {
    match local {
        None => true,
        Some((local_time, local_device)) => {
            let incoming_time = format_time(incoming_time);
            (incoming_time.as_str(), incoming_device) > (local_time.as_str(), local_device.as_str())
        }
    }
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        name: row.get(1)?,
        project: row.get(2)?,
        tags: decode_tags(row.get::<_, String>(3)?),
        created_at: parse_time(row.get::<_, String>(4)?)?,
        updated_at: parse_time(row.get::<_, String>(5)?)?,
        source_device: row.get(6)?,
        deleted: row.get(7)?,
    })
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimeEntry> {
    let stopped_at: Option<String> = row.get(6)?;
    Ok(TimeEntry {
        id: row.get(0)?,
        task_id: row.get(1)?,
        task_name: row.get(2)?,
        project: row.get(3)?,
        tags: decode_tags(row.get::<_, String>(4)?),
        started_at: parse_time(row.get::<_, String>(5)?)?,
        stopped_at: stopped_at.map(parse_time).transpose()?,
        updated_at: parse_time(row.get::<_, String>(7)?)?,
        source_device: row.get(8)?,
        deleted: row.get(9)?,
    })
}

fn parse_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn format_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

fn encode_tags(tags: &[String]) -> Result<String> {
    Ok(serde_json::to_string(tags)?)
}

fn decode_tags(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn cleaned_required(label: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value.to_owned())
}

fn cleaned_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalized_tags(tags: &[String]) -> Vec<String> {
    let mut tags: Vec<_> = tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn task_id(name: &str) -> String {
    let normalized = name.trim().to_lowercase();
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("https://github.com/EasternKentuckyDigital/tracker/task/{normalized}").as_bytes(),
    )
    .to_string()
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    project TEXT,
    tags TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    source_device TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS tasks_name_active
    ON tasks(name COLLATE NOCASE) WHERE deleted = 0;

CREATE TABLE IF NOT EXISTS peers (
    name TEXT PRIMARY KEY COLLATE NOCASE,
    url TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS time_entries (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    task_name TEXT NOT NULL,
    project TEXT,
    tags TEXT NOT NULL,
    started_at TEXT NOT NULL,
    stopped_at TEXT,
    updated_at TEXT NOT NULL,
    source_device TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS entries_started_at ON time_entries(started_at);
DROP INDEX IF EXISTS one_active_timer;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Database {
        Database::open(tempfile::NamedTempFile::new().unwrap().path()).unwrap()
    }

    #[test]
    fn starts_and_stops_an_entry() {
        let mut db = database();
        let entry = db
            .start("Write tests", Some("tracker"), &["rust".into()])
            .unwrap();
        assert_eq!(entry.task_name, "Write tests");
        assert!(entry.stopped_at.is_none());

        let stopped = db.stop().unwrap();
        assert_eq!(stopped.len(), 1);
        assert!(stopped[0].stopped_at.is_some());
        assert!(db.active_entry().unwrap().is_none());
    }

    #[test]
    fn prevents_two_active_entries() {
        let mut db = database();
        db.start("First", None, &[]).unwrap();
        assert!(db.start("Second", None, &[]).is_err());
    }

    #[test]
    fn merge_is_idempotent() {
        let mut source = database();
        source.start("Shared task", Some("work"), &[]).unwrap();
        let tasks = source.all_tasks().unwrap();
        let entries = source.all_entries().unwrap();

        let mut target = database();
        let first = target.merge(&tasks, &entries).unwrap();
        let second = target.merge(&tasks, &entries).unwrap();
        assert_eq!(first.tasks_applied, 1);
        assert_eq!(first.entries_applied, 1);
        assert_eq!(second, MergeSummary::default());
    }

    #[test]
    fn manages_local_sync_peers() {
        let db = database();
        db.add_peer("laptop", "https://laptop.example.ts.net/")
            .unwrap();
        let peers = db.list_peers().unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].url, "https://laptop.example.ts.net");
        assert!(db.remove_peer("LAPTOP").unwrap());
        assert!(db.list_peers().unwrap().is_empty());
    }
}
