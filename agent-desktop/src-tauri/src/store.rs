use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{DesktopError, DesktopResult, SavePreferenceRequest, UiPreference};

const SCHEMA: &str = r#"
-- Device-local visual preferences only; Runtime business data is never stored here.
CREATE TABLE IF NOT EXISTS ui_preference(
  id TEXT PRIMARY KEY NOT NULL,
  preference_key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  create_time TEXT NOT NULL,
  update_time TEXT NOT NULL,
  create_user TEXT NOT NULL,
  update_user TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ui_preference_kind ON ui_preference(kind,preference_key,id);
CREATE INDEX IF NOT EXISTS idx_ui_preference_updated ON ui_preference(updated_at DESC,id);
"#;

pub struct DesktopPreferenceStore {
    connection: Mutex<Connection>,
}

impl DesktopPreferenceStore {
    pub fn new(path: impl AsRef<Path>) -> DesktopResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> DesktopResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> DesktopResult<Self> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=OFF;")?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn save(&self, request: SavePreferenceRequest, actor: &str) -> DesktopResult<UiPreference> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let current = read_by_key(&transaction, &request.key)?;
        let value = if let Some(mut current) = current {
            if request.expected_version != Some(current.version) {
                return Err(DesktopError::Conflict(format!(
                    "preference {} has a newer version",
                    request.key
                )));
            }
            if current.kind != request.kind {
                return Err(DesktopError::Conflict(
                    "preference kind cannot change".into(),
                ));
            }
            current.value = request.value;
            current.version = current.version.saturating_add(1);
            current.actor = actor.into();
            current.updated_at = Utc::now().max(current.updated_at);
            current.validate()?;
            let changed = transaction.execute(
                "UPDATE ui_preference SET version=?1,content=?2,updated_at=?3,update_time=?4,update_user=?5 WHERE id=?6 AND version=?7",
                params![
                    to_i64(current.version)?,
                    serde_json::to_string(&current)?,
                    current.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    current.id.to_string(),
                    to_i64(request.expected_version.unwrap_or_default())?,
                ],
            )?;
            if changed != 1 {
                return Err(DesktopError::Conflict("stale preference writer".into()));
            }
            current
        } else {
            if request.expected_version.is_some() {
                return Err(DesktopError::Conflict(
                    "new preference cannot have an expected version".into(),
                ));
            }
            let value = UiPreference::new(request.key, request.kind, request.value, actor);
            value.validate()?;
            let now = Utc::now().to_rfc3339();
            transaction.execute(
                "INSERT INTO ui_preference(id,preference_key,kind,version,content,created_at,updated_at,create_time,update_time,create_user,update_user) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?9)",
                params![
                    value.id.to_string(),
                    value.key,
                    value.kind.as_str(),
                    to_i64(value.version)?,
                    serde_json::to_string(&value)?,
                    value.created_at.to_rfc3339(),
                    value.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
            value
        };
        transaction.commit()?;
        Ok(value)
    }

    pub fn find(&self, key: &str) -> DesktopResult<Option<UiPreference>> {
        let connection = self.lock()?;
        read_by_key(&connection, key)
    }

    pub fn list(&self) -> DesktopResult<Vec<UiPreference>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT preference_key FROM ui_preference ORDER BY kind,preference_key,id")?;
        let keys = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        keys.into_iter()
            .map(|key| {
                read_by_key(&connection, &key)?.ok_or_else(|| DesktopError::NotFound(key.clone()))
            })
            .collect()
    }

    fn lock(&self) -> DesktopResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| DesktopError::Internal("desktop SQLite lock poisoned".into()))
    }
}

fn read_by_key(connection: &Connection, key: &str) -> DesktopResult<Option<UiPreference>> {
    let row = connection
        .query_row(
            "SELECT id,kind,version,content,created_at,updated_at,update_user FROM ui_preference WHERE preference_key=?1",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let value: UiPreference = serde_json::from_str(&row.3)?;
    value.validate()?;
    if value.id != parse_uuid(&row.0)?
        || value.key != key
        || value.kind.as_str() != row.1
        || value.version != from_i64(row.2)?
        || value.created_at != parse_time(&row.4)?
        || value.updated_at != parse_time(&row.5)?
        || value.actor != row.6
    {
        return Err(DesktopError::Validation(
            "preference columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn parse_uuid(value: &str) -> DesktopResult<Uuid> {
    Uuid::parse_str(value).map_err(|error| DesktopError::Validation(error.to_string()))
}

fn parse_time(value: &str) -> DesktopResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| DesktopError::Validation(error.to_string()))
}

fn to_i64(value: u64) -> DesktopResult<i64> {
    i64::try_from(value)
        .map_err(|_| DesktopError::Validation("version exceeds SQLite range".into()))
}

fn from_i64(value: i64) -> DesktopResult<u64> {
    u64::try_from(value).map_err(|_| DesktopError::Validation("stored version is invalid".into()))
}
