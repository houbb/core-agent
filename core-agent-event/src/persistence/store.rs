use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, EventDeadLetter, EventEnvelope, EventPolicyDefinition, EventReplayRecord,
    EventSubscription,
};
use crate::error::{EventError, EventResult};
use crate::infrastructure::{DeadLetterQueue, EventCommit, EventStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteEventStore {
    connection: Mutex<Connection>,
}

impl SqliteEventStore {
    pub fn new(path: impl AsRef<Path>) -> EventResult<Self> {
        let connection = if path.as_ref() == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> EventResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| EventError::Internal("event database lock poisoned".into()))
    }
}

#[async_trait]
impl DeadLetterQueue for SqliteEventStore {
    async fn save_dead_letter(&self, value: &EventDeadLetter, actor: &str) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let connection = self.lock()?;
        write_dead_letter(&connection, value, actor)
    }

    async fn find_dead_letter(&self, id: Uuid) -> EventResult<Option<EventDeadLetter>> {
        let connection = self.lock()?;
        read_dead_letter(&connection, id)
    }

    async fn list_dead_letters(&self, event_id: Uuid) -> EventResult<Vec<EventDeadLetter>> {
        let connection = self.lock()?;
        let ids = query_ids(
            &connection,
            "SELECT id FROM event_dead_letter WHERE event_id = ?1 ORDER BY created_at, id",
            event_id.to_string(),
        )?;
        ids.into_iter()
            .map(|id| {
                read_dead_letter(&connection, parse_uuid("dead-letter id", &id)?)?.ok_or_else(
                    || EventError::Internal("listed event dead letter disappeared".into()),
                )
            })
            .collect()
    }
}

#[async_trait]
impl EventStore for SqliteEventStore {
    async fn commit_event(
        &self,
        commit: &EventCommit,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_event(&transaction, commit, actor)?;
        for value in dead_letters {
            value.validate()?;
            if value.event_id != commit.event.id
                || value.payload_hash != commit.event.payload_hash()?
            {
                return Err(EventError::Validation(
                    "dead letter does not belong to committed event content".into(),
                ));
            }
            write_dead_letter(&transaction, value, actor)?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn find_event(&self, id: Uuid) -> EventResult<Option<EventEnvelope>> {
        let connection = self.lock()?;
        read_event(&connection, id)
    }

    async fn list_events(&self, namespace: &str) -> EventResult<Vec<EventEnvelope>> {
        let connection = self.lock()?;
        let ids = query_ids(
            &connection,
            "SELECT id FROM event WHERE namespace = ?1 ORDER BY occurred_at DESC, id",
            namespace,
        )?;
        ids.into_iter()
            .map(|id| {
                read_event(&connection, parse_uuid("event id", &id)?)?
                    .ok_or_else(|| EventError::Internal("listed event disappeared".into()))
            })
            .collect()
    }

    async fn save_subscription(
        &self,
        value: &EventSubscription,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let connection = self.lock()?;
        match expected_version {
            None => {
                let duplicate = connection
                    .query_row(
                        "SELECT id FROM event_subscription WHERE id = ?1 OR subscription_key = ?2 LIMIT 1",
                        params![value.id.to_string(), value.key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if duplicate.is_some() {
                    return Err(EventError::Conflict(
                        "event subscription identity already exists".into(),
                    ));
                }
                let now = Utc::now().to_rfc3339();
                connection.execute(
                    "INSERT INTO event_subscription (
                        id, subscription_key, namespace, priority, enabled, version,
                        content, created_at, updated_at, create_time, update_time,
                        create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?11)",
                    params![
                        value.id.to_string(),
                        value.key,
                        value.namespace,
                        value.priority,
                        bool_i64(value.enabled),
                        u64_i64(value.version)?,
                        serde_json::to_string(value)?,
                        value.created_at.to_rfc3339(),
                        value.updated_at.to_rfc3339(),
                        now,
                        actor,
                    ],
                )?;
            }
            Some(expected) => {
                let current = read_subscription(&connection, value.id)?
                    .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
                validate_subscription_update(&current, value, expected)?;
                let changed = connection.execute(
                    "UPDATE event_subscription SET priority = ?1, enabled = ?2, version = ?3,
                        content = ?4, updated_at = ?5, update_time = ?6, update_user = ?7
                     WHERE id = ?8 AND version = ?9",
                    params![
                        value.priority,
                        bool_i64(value.enabled),
                        u64_i64(value.version)?,
                        serde_json::to_string(value)?,
                        value.updated_at.to_rfc3339(),
                        Utc::now().to_rfc3339(),
                        actor,
                        value.id.to_string(),
                        u64_i64(expected)?,
                    ],
                )?;
                if changed != 1 {
                    return Err(EventError::Conflict(
                        "event subscription changed concurrently".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    async fn find_subscription(&self, id: Uuid) -> EventResult<Option<EventSubscription>> {
        let connection = self.lock()?;
        read_subscription(&connection, id)
    }

    async fn list_subscriptions(&self, namespace: &str) -> EventResult<Vec<EventSubscription>> {
        let connection = self.lock()?;
        let ids = query_ids(
            &connection,
            "SELECT id FROM event_subscription WHERE namespace = ?1 ORDER BY priority DESC, subscription_key, id",
            namespace,
        )?;
        ids.into_iter()
            .map(|id| {
                read_subscription(&connection, parse_uuid("subscription id", &id)?)?.ok_or_else(
                    || EventError::Internal("listed event subscription disappeared".into()),
                )
            })
            .collect()
    }

    async fn save_replay(
        &self,
        value: &EventReplayRecord,
        expected_version: Option<u64>,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_replay(&transaction, value, expected_version, dead_letters, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_replay(&self, id: Uuid) -> EventResult<Option<EventReplayRecord>> {
        let connection = self.lock()?;
        read_replay(&connection, id)
    }

    async fn list_replays(&self, event_id: Uuid) -> EventResult<Vec<EventReplayRecord>> {
        let connection = self.lock()?;
        let ids = query_ids(
            &connection,
            "SELECT id FROM event_replay WHERE event_id = ?1 ORDER BY created_at, id",
            event_id.to_string(),
        )?;
        ids.into_iter()
            .map(|id| {
                read_replay(&connection, parse_uuid("replay id", &id)?)?
                    .ok_or_else(|| EventError::Internal("listed event replay disappeared".into()))
            })
            .collect()
    }

    async fn save_policy(
        &self,
        value: &EventPolicyDefinition,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let connection = self.lock()?;
        match expected_version {
            None => {
                let duplicate = connection
                    .query_row(
                        "SELECT id FROM event_policy WHERE id = ?1 OR policy_key = ?2 LIMIT 1",
                        params![value.id.to_string(), value.key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if duplicate.is_some() {
                    return Err(EventError::Conflict(
                        "event policy identity already exists".into(),
                    ));
                }
                let now = Utc::now().to_rfc3339();
                connection.execute(
                    "INSERT INTO event_policy (
                        id, policy_key, name, version, content, created_at, updated_at,
                        create_time, update_time, create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                    params![
                        value.id.to_string(),
                        value.key,
                        value.name,
                        u64_i64(value.version)?,
                        serde_json::to_string(value)?,
                        value.created_at.to_rfc3339(),
                        value.updated_at.to_rfc3339(),
                        now,
                        actor,
                    ],
                )?;
            }
            Some(expected) => {
                let current = read_policy(&connection, value.id)?
                    .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
                validate_policy_update(&current, value, expected)?;
                let changed = connection.execute(
                    "UPDATE event_policy SET name = ?1, version = ?2, content = ?3,
                        updated_at = ?4, update_time = ?5, update_user = ?6
                     WHERE id = ?7 AND version = ?8",
                    params![
                        value.name,
                        u64_i64(value.version)?,
                        serde_json::to_string(value)?,
                        value.updated_at.to_rfc3339(),
                        Utc::now().to_rfc3339(),
                        actor,
                        value.id.to_string(),
                        u64_i64(expected)?,
                    ],
                )?;
                if changed != 1 {
                    return Err(EventError::Conflict(
                        "event policy changed concurrently".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> EventResult<Option<EventPolicyDefinition>> {
        let connection = self.lock()?;
        read_policy(&connection, id)
    }

    async fn list_policies(&self) -> EventResult<Vec<EventPolicyDefinition>> {
        let connection = self.lock()?;
        let mut statement =
            connection.prepare("SELECT id FROM event_policy ORDER BY policy_key, id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_policy(&connection, parse_uuid("policy id", &id)?)?
                    .ok_or_else(|| EventError::Internal("listed event policy disappeared".into()))
            })
            .collect()
    }
}

fn write_event(
    transaction: &Transaction<'_>,
    commit: &EventCommit,
    actor: &str,
) -> EventResult<()> {
    let value = &commit.event;
    match commit.expected_version {
        None => {
            if read_event(transaction, value.id)?.is_some() {
                return Err(EventError::Conflict(format!(
                    "event {} already exists",
                    value.id
                )));
            }
            let now = Utc::now().to_rfc3339();
            transaction.execute(
                "INSERT INTO event (
                    id, event_type, category, namespace, source_kind, target, state,
                    priority, visibility, sensitive, schema_version, policy_id, version,
                    content, occurred_at, created_at, updated_at, create_time, update_time,
                    create_user, update_user
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16, ?17, ?18, ?18, ?19, ?19
                 )",
                params![
                    value.id.to_string(),
                    value.event_type,
                    value.category.as_str(),
                    value.namespace,
                    value.source.kind.as_str(),
                    value.target,
                    value.state.as_str(),
                    value.priority.as_str(),
                    value.visibility.as_str(),
                    bool_i64(value.sensitive),
                    i64::from(value.schema_version),
                    value.policy_id.map(|id| id.to_string()),
                    u64_i64(value.version)?,
                    serde_json::to_string(value)?,
                    value.occurred_at.to_rfc3339(),
                    value.created_at.to_rfc3339(),
                    value.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            let current = read_event(transaction, value.id)?
                .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
            validate_event_update(&current, value, expected)?;
            let changed = transaction.execute(
                "UPDATE event SET state = ?1, version = ?2, content = ?3, updated_at = ?4,
                    update_time = ?5, update_user = ?6 WHERE id = ?7 AND version = ?8",
                params![
                    value.state.as_str(),
                    u64_i64(value.version)?,
                    serde_json::to_string(value)?,
                    value.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    value.id.to_string(),
                    u64_i64(expected)?,
                ],
            )?;
            if changed != 1 {
                return Err(EventError::Conflict("event changed concurrently".into()));
            }
        }
    }
    Ok(())
}

fn write_replay(
    connection: &Connection,
    value: &EventReplayRecord,
    expected_version: Option<u64>,
    dead_letters: &[EventDeadLetter],
    actor: &str,
) -> EventResult<()> {
    match expected_version {
        None => {
            if read_replay(connection, value.id)?.is_some() {
                return Err(EventError::Conflict(format!(
                    "event replay {} already exists",
                    value.id
                )));
            }
            let now = Utc::now().to_rfc3339();
            connection.execute(
                "INSERT INTO event_replay (
                    id, event_id, state, version, content, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                params![
                    value.id.to_string(),
                    value.event_id.to_string(),
                    value.state.as_str(),
                    u64_i64(value.version)?,
                    serde_json::to_string(value)?,
                    value.created_at.to_rfc3339(),
                    value.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            let current = read_replay(connection, value.id)?
                .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
            validate_replay_update(&current, value, expected)?;
            let changed = connection.execute(
                "UPDATE event_replay SET state = ?1, version = ?2, content = ?3,
                    updated_at = ?4, update_time = ?5, update_user = ?6
                 WHERE id = ?7 AND version = ?8",
                params![
                    value.state.as_str(),
                    u64_i64(value.version)?,
                    serde_json::to_string(value)?,
                    value.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    value.id.to_string(),
                    u64_i64(expected)?,
                ],
            )?;
            if changed != 1 {
                return Err(EventError::Conflict(
                    "event replay changed concurrently".into(),
                ));
            }
        }
    }
    for dead_letter in dead_letters {
        dead_letter.validate()?;
        if dead_letter.replay_id != Some(value.id) {
            return Err(EventError::Validation(
                "event replay dead letter has a different replay owner".into(),
            ));
        }
        validate_dead_letter_ownership(connection, dead_letter)?;
        write_dead_letter(connection, dead_letter, actor)?;
    }
    Ok(())
}

fn read_event(connection: &Connection, id: Uuid) -> EventResult<Option<EventEnvelope>> {
    let raw = connection
        .query_row(
            "SELECT event_type, category, namespace, source_kind, target, state, priority,
                    visibility, sensitive, schema_version, policy_id, version, content,
                    occurred_at, created_at, updated_at FROM event WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: EventEnvelope = serde_json::from_str(&raw.12)?;
    value.validate()?;
    if value.id != id
        || raw.0 != value.event_type
        || raw.1 != value.category.as_str()
        || raw.2 != value.namespace
        || raw.3 != value.source.kind.as_str()
        || raw.4 != value.target
        || raw.5 != value.state.as_str()
        || raw.6 != value.priority.as_str()
        || raw.7 != value.visibility.as_str()
        || raw.8 != bool_i64(value.sensitive)
        || raw.9 != i64::from(value.schema_version)
        || raw.10 != value.policy_id.map(|id| id.to_string())
        || raw.11 != u64_i64(value.version)?
        || raw.13 != value.occurred_at.to_rfc3339()
        || raw.14 != value.created_at.to_rfc3339()
        || raw.15 != value.updated_at.to_rfc3339()
    {
        return Err(EventError::Validation(
            "event structured columns do not match serialized content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_subscription(connection: &Connection, id: Uuid) -> EventResult<Option<EventSubscription>> {
    let raw = connection
        .query_row(
            "SELECT subscription_key, namespace, priority, enabled, version, content,
                    created_at, updated_at FROM event_subscription WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: EventSubscription = serde_json::from_str(&raw.5)?;
    value.validate()?;
    if value.id != id
        || raw.0 != value.key
        || raw.1 != value.namespace
        || raw.2 != value.priority
        || raw.3 != bool_i64(value.enabled)
        || raw.4 != u64_i64(value.version)?
        || raw.6 != value.created_at.to_rfc3339()
        || raw.7 != value.updated_at.to_rfc3339()
    {
        return Err(EventError::Validation(
            "event subscription columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_replay(connection: &Connection, id: Uuid) -> EventResult<Option<EventReplayRecord>> {
    let raw = connection
        .query_row(
            "SELECT event_id, state, version, content, created_at, updated_at
             FROM event_replay WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: EventReplayRecord = serde_json::from_str(&raw.3)?;
    value.validate()?;
    if value.id != id
        || raw.0 != value.event_id.to_string()
        || raw.1 != value.state.as_str()
        || raw.2 != u64_i64(value.version)?
        || raw.4 != value.created_at.to_rfc3339()
        || raw.5 != value.updated_at.to_rfc3339()
    {
        return Err(EventError::Validation(
            "event replay columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_policy(connection: &Connection, id: Uuid) -> EventResult<Option<EventPolicyDefinition>> {
    let raw = connection
        .query_row(
            "SELECT policy_key, name, version, content, created_at, updated_at
             FROM event_policy WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: EventPolicyDefinition = serde_json::from_str(&raw.3)?;
    value.validate()?;
    if value.id != id
        || raw.0 != value.key
        || raw.1 != value.name
        || raw.2 != u64_i64(value.version)?
        || raw.4 != value.created_at.to_rfc3339()
        || raw.5 != value.updated_at.to_rfc3339()
    {
        return Err(EventError::Validation(
            "event policy columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn write_dead_letter(
    connection: &Connection,
    value: &EventDeadLetter,
    actor: &str,
) -> EventResult<()> {
    validate_dead_letter_ownership(connection, value)?;
    if read_dead_letter(connection, value.id)?.is_some() {
        return Err(EventError::Conflict(format!(
            "event dead letter {} already exists",
            value.id
        )));
    }
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO event_dead_letter (
            id, event_id, subscription_id, replay_id, resolved, attempts, error,
            version, content, created_at, updated_at, create_time, update_time,
            create_user, update_user
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?13)",
        params![
            value.id.to_string(),
            value.event_id.to_string(),
            value.subscription_id.to_string(),
            value.replay_id.map(|id| id.to_string()),
            bool_i64(value.resolved),
            i64::from(value.attempts),
            value.error,
            u64_i64(value.version)?,
            serde_json::to_string(value)?,
            value.created_at.to_rfc3339(),
            value.updated_at.to_rfc3339(),
            now,
            actor,
        ],
    )?;
    Ok(())
}

fn validate_dead_letter_ownership(
    connection: &Connection,
    value: &EventDeadLetter,
) -> EventResult<()> {
    let event = read_event(connection, value.event_id)?
        .ok_or_else(|| EventError::NotFound(value.event_id.to_string()))?;
    let replay_is_invalid = if let Some(id) = value.replay_id {
        read_replay(connection, id)?.is_none_or(|replay| replay.event_id != value.event_id)
    } else {
        false
    };
    if event.payload_hash()? != value.payload_hash
        || read_subscription(connection, value.subscription_id)?.is_none()
        || replay_is_invalid
    {
        return Err(EventError::Validation(
            "event dead letter has an invalid event, subscription or replay owner".into(),
        ));
    }
    Ok(())
}

fn read_dead_letter(connection: &Connection, id: Uuid) -> EventResult<Option<EventDeadLetter>> {
    let raw = connection
        .query_row(
            "SELECT event_id, subscription_id, replay_id, resolved, attempts, error,
                    version, content, created_at, updated_at
             FROM event_dead_letter WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: EventDeadLetter = serde_json::from_str(&raw.7)?;
    value.validate()?;
    if value.id != id
        || raw.0 != value.event_id.to_string()
        || raw.1 != value.subscription_id.to_string()
        || raw.2 != value.replay_id.map(|id| id.to_string())
        || raw.3 != bool_i64(value.resolved)
        || raw.4 != i64::from(value.attempts)
        || raw.5 != value.error
        || raw.6 != u64_i64(value.version)?
        || raw.8 != value.created_at.to_rfc3339()
        || raw.9 != value.updated_at.to_rfc3339()
    {
        return Err(EventError::Validation(
            "event dead-letter columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn validate_event_update(
    current: &EventEnvelope,
    next: &EventEnvelope,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.event_type != next.event_type
        || current.category != next.category
        || current.namespace != next.namespace
        || current.source != next.source
        || current.target != next.target
        || current.payload != next.payload
        || current.payload_type != next.payload_type
        || current.metadata != next.metadata
        || current.priority != next.priority
        || current.visibility != next.visibility
        || current.sensitive != next.sensitive
        || current.schema_version != next.schema_version
        || current.policy_id != next.policy_id
        || current.occurred_at != next.occurred_at
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(EventError::Conflict(
            "event identity, payload or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_subscription_update(
    current: &EventSubscription,
    next: &EventSubscription,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.namespace != next.namespace
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(EventError::Conflict(
            "event subscription identity or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_replay_update(
    current: &EventReplayRecord,
    next: &EventReplayRecord,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.event_id != next.event_id
        || current.subscription_ids != next.subscription_ids
        || current.reason != next.reason
        || current.actor != next.actor
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(EventError::Conflict(
            "event replay identity or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_policy_update(
    current: &EventPolicyDefinition,
    next: &EventPolicyDefinition,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.updated_at <= current.updated_at
    {
        return Err(EventError::Conflict(
            "event policy identity or version conflict".into(),
        ));
    }
    Ok(())
}

fn query_ids(
    connection: &Connection,
    sql: &str,
    value: impl rusqlite::ToSql,
) -> EventResult<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let values = statement
        .query_map(params![value], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

fn parse_uuid(label: &str, value: &str) -> EventResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|_| EventError::Validation(format!("{label} is not a valid UUID")))
}

fn u64_i64(value: u64) -> EventResult<i64> {
    i64::try_from(value)
        .map_err(|_| EventError::Validation("event version exceeds SQLite range".into()))
}

fn bool_i64(value: bool) -> i64 {
    i64::from(value)
}
