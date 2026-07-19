use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::domain::{
    Agent, AgentPolicyDefinition, AgentProfile, AgentSnapshot, AgentState, AgentStateRecord,
};
use crate::error::{AgentError, AgentResult};
use crate::infrastructure::{AgentCommit, AgentStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteAgentStore {
    connection: Mutex<Connection>,
}

impl SqliteAgentStore {
    pub fn new(path: impl AsRef<Path>) -> AgentResult<Self> {
        let connection = if path.as_ref() == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA journal_mode = WAL;")?;
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> AgentResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AgentError::Internal("agent database lock poisoned".into()))
    }
}

#[async_trait]
impl AgentStore for SqliteAgentStore {
    async fn commit(&self, commit: &AgentCommit, actor: &str) -> AgentResult<()> {
        commit.validate(actor)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_agent(&transaction, commit, actor)?;
        insert_state(&transaction, &commit.state, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_agent(&self, id: Uuid) -> AgentResult<Option<Agent>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT id,name,profile_id,profile_version,state,session_id,workspace_id,current_goal_id,current_plan_id,current_execution_id,version,content,created_at,updated_at FROM agent WHERE id=?1",
                [id.to_string()],
                parse_agent,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_agents(&self) -> AgentResult<Vec<Agent>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,name,profile_id,profile_version,state,session_id,workspace_id,current_goal_id,current_plan_id,current_execution_id,version,content,created_at,updated_at FROM agent ORDER BY created_at,id",
        )?;
        let rows = statement.query_map([], parse_agent)?;
        collect_rows(rows)
    }

    async fn list_states(&self, agent_id: Uuid) -> AgentResult<Vec<AgentStateRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,agent_id,sequence,from_state,to_state,goal_id,plan_id,execution_id,reason,actor,content,created_at FROM agent_state WHERE agent_id=?1 ORDER BY sequence,id",
        )?;
        let rows = statement.query_map([agent_id.to_string()], parse_state)?;
        collect_rows(rows)
    }

    async fn save_profile(&self, profile: &AgentProfile, actor: &str) -> AgentResult<()> {
        profile.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_profile(&transaction, profile, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_profile(&self, id: Uuid) -> AgentResult<Option<AgentProfile>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT id,profile_key,name,policy_id,version,content,created_at,updated_at FROM agent_profile WHERE id=?1",
                [id.to_string()],
                parse_profile,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_profiles(&self) -> AgentResult<Vec<AgentProfile>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,profile_key,name,policy_id,version,content,created_at,updated_at FROM agent_profile ORDER BY created_at,id",
        )?;
        let rows = statement.query_map([], parse_profile)?;
        collect_rows(rows)
    }

    async fn save_policy(&self, policy: &AgentPolicyDefinition, actor: &str) -> AgentResult<()> {
        policy.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_policy(&transaction, policy, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> AgentResult<Option<AgentPolicyDefinition>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT id,policy_key,name,version,content,created_at,updated_at FROM agent_policy WHERE id=?1",
                [id.to_string()],
                parse_policy,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_policies(&self) -> AgentResult<Vec<AgentPolicyDefinition>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,policy_key,name,version,content,created_at,updated_at FROM agent_policy ORDER BY created_at,id",
        )?;
        let rows = statement.query_map([], parse_policy)?;
        collect_rows(rows)
    }

    async fn save_snapshot(&self, snapshot: &AgentSnapshot, actor: &str) -> AgentResult<()> {
        snapshot.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let owner_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM agent WHERE id=?1)",
            [snapshot.agent_id.to_string()],
            |row| row.get(0),
        )?;
        if !owner_exists {
            return Err(AgentError::NotFound(snapshot.agent_id.to_string()));
        }
        let now = Utc::now().to_rfc3339();
        let inserted = transaction.execute(
            "INSERT INTO agent_snapshot (id,agent_id,agent_version,state,label,hash,content,created_at,create_time,update_time,create_user,update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
            params![snapshot.id.to_string(),snapshot.agent_id.to_string(),snapshot.agent_version as i64,snapshot.state.as_str(),snapshot.label,snapshot.hash,serde_json::to_string(snapshot)?,snapshot.created_at.to_rfc3339(),now,actor],
        );
        map_unique(inserted, "snapshot", snapshot.id)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> AgentResult<Option<AgentSnapshot>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT id,agent_id,agent_version,state,label,hash,content,created_at FROM agent_snapshot WHERE id=?1",
                [id.to_string()],
                parse_snapshot,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_snapshots(&self, agent_id: Uuid) -> AgentResult<Vec<AgentSnapshot>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id,agent_id,agent_version,state,label,hash,content,created_at FROM agent_snapshot WHERE agent_id=?1 ORDER BY agent_version,created_at,id",
        )?;
        let rows = statement.query_map([agent_id.to_string()], parse_snapshot)?;
        collect_rows(rows)
    }
}

fn write_agent(
    transaction: &Transaction<'_>,
    commit: &AgentCommit,
    actor: &str,
) -> AgentResult<()> {
    let value = &commit.agent;
    if let Some(expected) = commit.expected_version {
        let current: Agent = current_document(transaction, "agent", value.id)?
            .ok_or_else(|| AgentError::Conflict(format!("agent {} is missing", value.id)))?;
        current.validate()?;
        if current.version != expected {
            return Err(AgentError::Conflict(format!(
                "agent {} expected version {expected}",
                value.id
            )));
        }
        if current.profile != value.profile
            || current.policy != value.policy
            || current.session_id != value.session_id
            || current.workspace_id != value.workspace_id
            || current.created_at != value.created_at
        {
            return Err(AgentError::Validation(
                "Agent identity, Profile/Policy snapshots, or bindings changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match commit.expected_version {
        None => map_unique(
            transaction.execute(
                "INSERT INTO agent (id,name,profile_id,profile_version,state,session_id,workspace_id,current_goal_id,current_plan_id,current_execution_id,version,content,created_at,updated_at,create_time,update_time,create_user,update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15,?16,?16)",
                params![value.id.to_string(),value.name,value.profile.id.to_string(),value.profile.version as i64,value.state.as_str(),uuid_text(value.session_id),uuid_text(value.workspace_id),uuid_text(value.current_goal_id),uuid_text(value.current_plan_id),uuid_text(value.current_execution_id),value.version as i64,content,value.created_at.to_rfc3339(),value.updated_at.to_rfc3339(),now,actor],
            ),
            "agent",
            value.id,
        ),
        Some(expected) => {
            let changed = transaction.execute(
                "UPDATE agent SET name=?2,profile_id=?3,profile_version=?4,state=?5,session_id=?6,workspace_id=?7,current_goal_id=?8,current_plan_id=?9,current_execution_id=?10,version=?11,content=?12,updated_at=?13,update_time=?14,update_user=?15 WHERE id=?1 AND version=?16",
                params![value.id.to_string(),value.name,value.profile.id.to_string(),value.profile.version as i64,value.state.as_str(),uuid_text(value.session_id),uuid_text(value.workspace_id),uuid_text(value.current_goal_id),uuid_text(value.current_plan_id),uuid_text(value.current_execution_id),value.version as i64,content,value.updated_at.to_rfc3339(),now,actor,expected as i64],
            )?;
            if changed != 1 {
                return Err(AgentError::Conflict(format!(
                    "agent {} expected version {expected}", value.id
                )));
            }
            Ok(())
        }
    }
}

fn insert_state(
    transaction: &Transaction<'_>,
    value: &AgentStateRecord,
    actor: &str,
) -> AgentResult<()> {
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO agent_state (id,agent_id,sequence,from_state,to_state,goal_id,plan_id,execution_id,reason,actor,content,created_at,create_time,update_time,create_user,update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13,?14,?14)",
        params![value.id.to_string(),value.agent_id.to_string(),value.sequence as i64,value.from_state.map(AgentState::as_str),value.to_state.as_str(),uuid_text(value.goal_id),uuid_text(value.plan_id),uuid_text(value.execution_id),value.reason,value.actor,serde_json::to_string(value)?,value.created_at.to_rfc3339(),now,actor],
    )?;
    Ok(())
}

fn write_profile(
    transaction: &Transaction<'_>,
    value: &AgentProfile,
    actor: &str,
) -> AgentResult<()> {
    let current_value: Option<AgentProfile> =
        current_document(transaction, "agent_profile", value.id)?;
    if let Some(current) = &current_value {
        current.validate()?;
        if current.key != value.key
            || current.created_at != value.created_at
            || value.updated_at < current.updated_at
        {
            return Err(AgentError::Validation(
                "profile key, creation time, or update order is invalid".into(),
            ));
        }
    }
    let current = current_value.as_ref().map(|current| current.version);
    require_next_version(current, value.version, "profile", value.id)?;
    let now = Utc::now().to_rfc3339();
    let content = serde_json::to_string(value)?;
    if let Some(expected) = current {
        let changed = transaction.execute(
            "UPDATE agent_profile SET profile_key=?2,name=?3,policy_id=?4,version=?5,content=?6,updated_at=?7,update_time=?8,update_user=?9 WHERE id=?1 AND version=?10",
            params![value.id.to_string(),value.key,value.name,uuid_text(value.policy_id),value.version as i64,content,value.updated_at.to_rfc3339(),now,actor,expected as i64],
        )?;
        if changed != 1 {
            return Err(AgentError::Conflict(format!(
                "profile {} changed",
                value.id
            )));
        }
        Ok(())
    } else {
        map_unique(
            transaction.execute(
                "INSERT INTO agent_profile (id,profile_key,name,policy_id,version,content,created_at,updated_at,create_time,update_time,create_user,update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
                params![value.id.to_string(),value.key,value.name,uuid_text(value.policy_id),value.version as i64,content,value.created_at.to_rfc3339(),value.updated_at.to_rfc3339(),now,actor],
            ),
            "profile",
            value.id,
        )
    }
}

fn write_policy(
    transaction: &Transaction<'_>,
    value: &AgentPolicyDefinition,
    actor: &str,
) -> AgentResult<()> {
    let current_value: Option<AgentPolicyDefinition> =
        current_document(transaction, "agent_policy", value.id)?;
    if let Some(current) = &current_value {
        current.validate()?;
        if current.key != value.key
            || current.created_at != value.created_at
            || value.updated_at < current.updated_at
        {
            return Err(AgentError::Validation(
                "policy key, creation time, or update order is invalid".into(),
            ));
        }
    }
    let current = current_value.as_ref().map(|current| current.version);
    require_next_version(current, value.version, "policy", value.id)?;
    let now = Utc::now().to_rfc3339();
    let content = serde_json::to_string(value)?;
    if let Some(expected) = current {
        let changed = transaction.execute(
            "UPDATE agent_policy SET policy_key=?2,name=?3,version=?4,content=?5,updated_at=?6,update_time=?7,update_user=?8 WHERE id=?1 AND version=?9",
            params![value.id.to_string(),value.key,value.name,value.version as i64,content,value.updated_at.to_rfc3339(),now,actor,expected as i64],
        )?;
        if changed != 1 {
            return Err(AgentError::Conflict(format!("policy {} changed", value.id)));
        }
        Ok(())
    } else {
        map_unique(
            transaction.execute(
                "INSERT INTO agent_policy (id,policy_key,name,version,content,created_at,updated_at,create_time,update_time,create_user,update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?9)",
                params![value.id.to_string(),value.key,value.name,value.version as i64,content,value.created_at.to_rfc3339(),value.updated_at.to_rfc3339(),now,actor],
            ),
            "policy",
            value.id,
        )
    }
}

fn parse_agent(row: &Row<'_>) -> rusqlite::Result<AgentResult<Agent>> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let profile_id: String = row.get(2)?;
    let profile_version: i64 = row.get(3)?;
    let state: String = row.get(4)?;
    let session_id: Option<String> = row.get(5)?;
    let workspace_id: Option<String> = row.get(6)?;
    let goal_id: Option<String> = row.get(7)?;
    let plan_id: Option<String> = row.get(8)?;
    let execution_id: Option<String> = row.get(9)?;
    let version: i64 = row.get(10)?;
    let content: String = row.get(11)?;
    let created_at: String = row.get(12)?;
    let updated_at: String = row.get(13)?;
    Ok((|| {
        let value: Agent = serde_json::from_str(&content)?;
        value.validate()?;
        if id != value.id.to_string()
            || name != value.name
            || profile_id != value.profile.id.to_string()
            || as_u64(profile_version, "profile version")? != value.profile.version
            || parse_state_value(&state)? != value.state
            || parse_optional_uuid(session_id)? != value.session_id
            || parse_optional_uuid(workspace_id)? != value.workspace_id
            || parse_optional_uuid(goal_id)? != value.current_goal_id
            || parse_optional_uuid(plan_id)? != value.current_plan_id
            || parse_optional_uuid(execution_id)? != value.current_execution_id
            || as_u64(version, "agent version")? != value.version
            || parse_time(&created_at)? != value.created_at
            || parse_time(&updated_at)? != value.updated_at
        {
            return Err(corrupt("agent"));
        }
        Ok(value)
    })())
}

fn parse_profile(row: &Row<'_>) -> rusqlite::Result<AgentResult<AgentProfile>> {
    let id: String = row.get(0)?;
    let key: String = row.get(1)?;
    let name: String = row.get(2)?;
    let policy_id: Option<String> = row.get(3)?;
    let version: i64 = row.get(4)?;
    let content: String = row.get(5)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    Ok((|| {
        let value: AgentProfile = serde_json::from_str(&content)?;
        value.validate()?;
        if id != value.id.to_string()
            || key != value.key
            || name != value.name
            || parse_optional_uuid(policy_id)? != value.policy_id
            || as_u64(version, "profile version")? != value.version
            || parse_time(&created_at)? != value.created_at
            || parse_time(&updated_at)? != value.updated_at
        {
            return Err(corrupt("agent_profile"));
        }
        Ok(value)
    })())
}

fn parse_policy(row: &Row<'_>) -> rusqlite::Result<AgentResult<AgentPolicyDefinition>> {
    let id: String = row.get(0)?;
    let key: String = row.get(1)?;
    let name: String = row.get(2)?;
    let version: i64 = row.get(3)?;
    let content: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    Ok((|| {
        let value: AgentPolicyDefinition = serde_json::from_str(&content)?;
        value.validate()?;
        if id != value.id.to_string()
            || key != value.key
            || name != value.name
            || as_u64(version, "policy version")? != value.version
            || parse_time(&created_at)? != value.created_at
            || parse_time(&updated_at)? != value.updated_at
        {
            return Err(corrupt("agent_policy"));
        }
        Ok(value)
    })())
}

fn parse_snapshot(row: &Row<'_>) -> rusqlite::Result<AgentResult<AgentSnapshot>> {
    let id: String = row.get(0)?;
    let agent_id: String = row.get(1)?;
    let version: i64 = row.get(2)?;
    let state: String = row.get(3)?;
    let label: String = row.get(4)?;
    let hash: String = row.get(5)?;
    let content: String = row.get(6)?;
    let created_at: String = row.get(7)?;
    Ok((|| {
        let value: AgentSnapshot = serde_json::from_str(&content)?;
        value.validate()?;
        if id != value.id.to_string()
            || agent_id != value.agent_id.to_string()
            || as_u64(version, "snapshot version")? != value.agent_version
            || parse_state_value(&state)? != value.state
            || label != value.label
            || hash != value.hash
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("agent_snapshot"));
        }
        Ok(value)
    })())
}

fn parse_state(row: &Row<'_>) -> rusqlite::Result<AgentResult<AgentStateRecord>> {
    let id: String = row.get(0)?;
    let agent_id: String = row.get(1)?;
    let sequence: i64 = row.get(2)?;
    let from_state: Option<String> = row.get(3)?;
    let to_state: String = row.get(4)?;
    let goal_id: Option<String> = row.get(5)?;
    let plan_id: Option<String> = row.get(6)?;
    let execution_id: Option<String> = row.get(7)?;
    let reason: String = row.get(8)?;
    let actor: String = row.get(9)?;
    let content: String = row.get(10)?;
    let created_at: String = row.get(11)?;
    Ok((|| {
        let value: AgentStateRecord = serde_json::from_str(&content)?;
        crate::domain::validate_actor(&value.actor)?;
        crate::domain::validate_text("agent state reason", &value.reason, 1024)?;
        if id != value.id.to_string()
            || agent_id != value.agent_id.to_string()
            || as_u64(sequence, "state sequence")? != value.sequence
            || parse_optional_state(from_state)? != value.from_state
            || parse_state_value(&to_state)? != value.to_state
            || parse_optional_uuid(goal_id)? != value.goal_id
            || parse_optional_uuid(plan_id)? != value.plan_id
            || parse_optional_uuid(execution_id)? != value.execution_id
            || reason != value.reason
            || actor != value.actor
            || value.sequence == 0
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("agent_state"));
        }
        Ok(value)
    })())
}

fn current_document<T: DeserializeOwned>(
    transaction: &Transaction<'_>,
    table: &str,
    id: Uuid,
) -> AgentResult<Option<T>> {
    let sql = format!("SELECT content FROM {table} WHERE id=?1");
    let value: Option<String> = transaction
        .query_row(&sql, [id.to_string()], |row| row.get(0))
        .optional()?;
    value
        .map(|value| serde_json::from_str(&value).map_err(AgentError::from))
        .transpose()
}

fn require_next_version(current: Option<u64>, next: u64, label: &str, id: Uuid) -> AgentResult<()> {
    let valid = match current {
        None => next == 1,
        Some(value) => value.checked_add(1) == Some(next),
    };
    if !valid {
        return Err(AgentError::Conflict(format!(
            "{label} {id} version is not the next version"
        )));
    }
    Ok(())
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<AgentResult<T>>>,
) -> AgentResult<Vec<T>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row??);
    }
    Ok(values)
}

fn map_unique(result: rusqlite::Result<usize>, label: &str, id: Uuid) -> AgentResult<()> {
    match result {
        Ok(1) => Ok(()),
        Ok(_) => Err(AgentError::Internal(format!(
            "{label} insert changed no row"
        ))),
        Err(error) if is_constraint(&error) => {
            Err(AgentError::Conflict(format!("{label} {id} already exists")))
        }
        Err(error) => Err(error.into()),
    }
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(value, _)
            if value.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn uuid_text(value: Option<Uuid>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn parse_optional_uuid(value: Option<String>) -> AgentResult<Option<Uuid>> {
    value
        .map(|value| {
            Uuid::parse_str(&value)
                .map_err(|_| AgentError::Validation("stored UUID is invalid".into()))
        })
        .transpose()
}

fn parse_state_value(value: &str) -> AgentResult<AgentState> {
    AgentState::parse(value)
        .ok_or_else(|| AgentError::Validation("stored Agent state is invalid".into()))
}

fn parse_optional_state(value: Option<String>) -> AgentResult<Option<AgentState>> {
    value.map(|value| parse_state_value(&value)).transpose()
}

fn as_u64(value: i64, label: &str) -> AgentResult<u64> {
    value
        .try_into()
        .map_err(|_| AgentError::Validation(format!("stored {label} is invalid")))
}

fn parse_time(value: &str) -> AgentResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AgentError::Validation("stored timestamp is invalid".into()))
}

fn corrupt(label: &str) -> AgentError {
    AgentError::Validation(format!("{label} columns do not match serialized content"))
}
