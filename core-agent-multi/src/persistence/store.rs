use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, AgentMember, Collaboration, CollaborationState, MemberState, Organization,
    Role, Team, TeamState,
};
use crate::error::{MultiAgentError, MultiAgentResult};
use crate::infrastructure::{CollaborationCommit, MultiAgentStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteMultiAgentStore {
    connection: Mutex<Connection>,
}

impl SqliteMultiAgentStore {
    pub fn new(path: impl AsRef<Path>) -> MultiAgentResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> MultiAgentResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> MultiAgentResult<Self> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = OFF;")?;
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> MultiAgentResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| MultiAgentError::Internal("SQLite connection lock poisoned".into()))
    }
}

#[async_trait]
impl MultiAgentStore for SqliteMultiAgentStore {
    async fn save_organization(
        &self,
        value: &Organization,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_organization(&transaction, value, expected_version, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_organization(&self, id: Uuid) -> MultiAgentResult<Option<Organization>> {
        let connection = self.lock()?;
        read_organization(&connection, id)
    }

    async fn find_organization_by_key(&self, key: &str) -> MultiAgentResult<Option<Organization>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT id FROM organization WHERE organization_key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        id.map(|value| read_organization(&connection, parse_uuid("organization id", &value)?))
            .transpose()
            .map(Option::flatten)
    }

    async fn list_organizations(&self) -> MultiAgentResult<Vec<Organization>> {
        let connection = self.lock()?;
        query_ids(
            &connection,
            "SELECT id FROM organization ORDER BY organization_key, id",
            [],
        )?
        .into_iter()
        .map(|id| read_organization(&connection, id)?.ok_or_else(|| MultiAgentError::not_found(id)))
        .collect()
    }

    async fn save_role(
        &self,
        value: &Role,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_role(&transaction, value, expected_version, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_role(&self, id: Uuid) -> MultiAgentResult<Option<Role>> {
        let connection = self.lock()?;
        let value = read_role(&connection, id)?;
        if let Some(value) = &value {
            require_organization(&connection, value.organization_id)?;
        }
        Ok(value)
    }

    async fn list_roles(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Role>> {
        let connection = self.lock()?;
        require_organization(&connection, organization_id)?;
        query_ids(
            &connection,
            "SELECT id FROM role WHERE organization_id = ?1 ORDER BY role_key, id",
            [organization_id.to_string()],
        )?
        .into_iter()
        .map(|id| read_role(&connection, id)?.ok_or_else(|| MultiAgentError::not_found(id)))
        .collect()
    }

    async fn save_team(
        &self,
        value: &Team,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_team(&transaction, value, expected_version, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_team(&self, id: Uuid) -> MultiAgentResult<Option<Team>> {
        let connection = self.lock()?;
        let value = read_team(&connection, id)?;
        if let Some(value) = &value {
            require_organization(&connection, value.organization_id)?;
        }
        Ok(value)
    }

    async fn list_teams(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Team>> {
        let connection = self.lock()?;
        require_organization(&connection, organization_id)?;
        query_ids(
            &connection,
            "SELECT id FROM team WHERE organization_id = ?1 ORDER BY team_key, id",
            [organization_id.to_string()],
        )?
        .into_iter()
        .map(|id| read_team(&connection, id)?.ok_or_else(|| MultiAgentError::not_found(id)))
        .collect()
    }

    async fn save_member(
        &self,
        value: &AgentMember,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_member(&transaction, value, expected_version, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_member(&self, id: Uuid) -> MultiAgentResult<Option<AgentMember>> {
        let connection = self.lock()?;
        let value = read_member(&connection, id)?;
        if let Some(value) = &value {
            validate_member_owner(&connection, value)?;
        }
        Ok(value)
    }

    async fn list_members(&self, team_id: Uuid) -> MultiAgentResult<Vec<AgentMember>> {
        let connection = self.lock()?;
        require_team(&connection, team_id)?;
        query_ids(
            &connection,
            "SELECT id FROM agent_member WHERE team_id = ?1 ORDER BY role_id, id",
            [team_id.to_string()],
        )?
        .into_iter()
        .map(|id| {
            let value =
                read_member(&connection, id)?.ok_or_else(|| MultiAgentError::not_found(id))?;
            validate_member_owner(&connection, &value)?;
            Ok(value)
        })
        .collect()
    }

    async fn commit_collaboration(
        &self,
        commit: &CollaborationCommit,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        validate_commit_relations(&transaction, commit)?;
        write_team(
            &transaction,
            &commit.team.value,
            commit.team.expected_version,
            actor,
        )?;
        write_collaboration(
            &transaction,
            &commit.collaboration.value,
            commit.collaboration.expected_version,
            actor,
        )?;
        for member in &commit.members {
            write_member(&transaction, &member.value, member.expected_version, actor)?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn find_collaboration(&self, id: Uuid) -> MultiAgentResult<Option<Collaboration>> {
        let connection = self.lock()?;
        let value = read_collaboration(&connection, id)?;
        if let Some(value) = &value {
            validate_collaboration_owner(&connection, value)?;
        }
        Ok(value)
    }

    async fn list_collaborations(&self, team_id: Uuid) -> MultiAgentResult<Vec<Collaboration>> {
        let connection = self.lock()?;
        require_team(&connection, team_id)?;
        query_ids(
            &connection,
            "SELECT id FROM collaboration WHERE team_id = ?1 ORDER BY created_at DESC, id",
            [team_id.to_string()],
        )?
        .into_iter()
        .map(|id| {
            let value = read_collaboration(&connection, id)?
                .ok_or_else(|| MultiAgentError::not_found(id))?;
            validate_collaboration_owner(&connection, &value)?;
            Ok(value)
        })
        .collect()
    }
}

fn write_organization(
    transaction: &Transaction<'_>,
    value: &Organization,
    expected: Option<u64>,
    actor: &str,
) -> MultiAgentResult<()> {
    let current = read_organization(transaction, value.id)?;
    validate_version(
        current.as_ref().map(|item| item.version),
        expected,
        value.version,
    )?;
    if let Some(current) = &current {
        if current.key != value.key || current.created_at != value.created_at {
            return Err(MultiAgentError::Conflict(
                "organization immutable identity changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match expected {
        None => {
            transaction.execute(
                "INSERT INTO organization (id, organization_key, name, version, content,
                    created_at, updated_at, create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                params![
                    value.id.to_string(),
                    value.key,
                    value.name,
                    u64_i64(value.version)?,
                    content,
                    value.created_at.to_rfc3339(),
                    value.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            update_row(
                transaction,
                "UPDATE organization SET name=?1, version=?2, content=?3, updated_at=?4,
                update_time=?5, update_user=?6 WHERE id=?7 AND version=?8",
                params![
                    value.name,
                    u64_i64(value.version)?,
                    content,
                    value.updated_at.to_rfc3339(),
                    now,
                    actor,
                    value.id.to_string(),
                    u64_i64(expected)?,
                ],
            )?;
        }
    }
    Ok(())
}

fn write_role(
    transaction: &Transaction<'_>,
    value: &Role,
    expected: Option<u64>,
    actor: &str,
) -> MultiAgentResult<()> {
    require_organization(transaction, value.organization_id)?;
    let current = read_role(transaction, value.id)?;
    validate_version(
        current.as_ref().map(|item| item.version),
        expected,
        value.version,
    )?;
    if let Some(current) = &current {
        if current.organization_id != value.organization_id
            || current.key != value.key
            || current.created_at != value.created_at
        {
            return Err(MultiAgentError::Conflict(
                "role immutable identity changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match expected {
        None => transaction.execute(
            "INSERT INTO role (id, organization_id, role_key, name, version, content,
                created_at, updated_at, create_time, update_time, create_user, update_user)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
            params![
                value.id.to_string(),
                value.organization_id.to_string(),
                value.key,
                value.name,
                u64_i64(value.version)?,
                content,
                value.created_at.to_rfc3339(),
                value.updated_at.to_rfc3339(),
                now,
                actor
            ],
        )?,
        Some(expected) => update_row(
            transaction,
            "UPDATE role SET name=?1, version=?2, content=?3, updated_at=?4,
                update_time=?5, update_user=?6 WHERE id=?7 AND version=?8",
            params![
                value.name,
                u64_i64(value.version)?,
                content,
                value.updated_at.to_rfc3339(),
                now,
                actor,
                value.id.to_string(),
                u64_i64(expected)?
            ],
        )?,
    };
    Ok(())
}

fn write_team(
    transaction: &Transaction<'_>,
    value: &Team,
    expected: Option<u64>,
    actor: &str,
) -> MultiAgentResult<()> {
    require_organization(transaction, value.organization_id)?;
    let current = read_team(transaction, value.id)?;
    validate_version(
        current.as_ref().map(|item| item.version),
        expected,
        value.version,
    )?;
    if let Some(current) = &current {
        if current.organization_id != value.organization_id
            || current.key != value.key
            || current.created_at != value.created_at
            || current.workspace_id != value.workspace_id
            || current.memory_scope != value.memory_scope
        {
            return Err(MultiAgentError::Conflict(
                "team immutable identity or shared references changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match expected {
        None => transaction.execute(
            "INSERT INTO team (id,organization_id,team_key,state,workspace_id,version,content,
                created_at,updated_at,create_time,update_time,create_user,update_user)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?11)",
            params![
                value.id.to_string(),
                value.organization_id.to_string(),
                value.key,
                value.state.as_str(),
                value.workspace_id.map(|id| id.to_string()),
                u64_i64(value.version)?,
                content,
                value.created_at.to_rfc3339(),
                value.updated_at.to_rfc3339(),
                now,
                actor
            ],
        )?,
        Some(expected) => update_row(
            transaction,
            "UPDATE team SET state=?1, version=?2, content=?3, updated_at=?4,
                update_time=?5, update_user=?6 WHERE id=?7 AND version=?8",
            params![
                value.state.as_str(),
                u64_i64(value.version)?,
                content,
                value.updated_at.to_rfc3339(),
                now,
                actor,
                value.id.to_string(),
                u64_i64(expected)?
            ],
        )?,
    };
    Ok(())
}

fn write_member(
    transaction: &Transaction<'_>,
    value: &AgentMember,
    expected: Option<u64>,
    actor: &str,
) -> MultiAgentResult<()> {
    validate_member_owner(transaction, value)?;
    let current = read_member(transaction, value.id)?;
    validate_version(
        current.as_ref().map(|item| item.version),
        expected,
        value.version,
    )?;
    if let Some(current) = &current {
        if current.team_id != value.team_id
            || current.role_id != value.role_id
            || current.agent_id != value.agent_id
            || current.capabilities != value.capabilities
            || current.created_at != value.created_at
        {
            return Err(MultiAgentError::Conflict(
                "member immutable identity or capability snapshot changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match expected {
        None => transaction.execute(
            "INSERT INTO agent_member (id,team_id,role_id,agent_id,state,current_collaboration_id,
                version,content,created_at,updated_at,create_time,update_time,create_user,update_user)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,?12)",
            params![value.id.to_string(), value.team_id.to_string(), value.role_id.to_string(),
                value.agent_id.to_string(), value.state.as_str(),
                value.current_collaboration_id.map(|id| id.to_string()), u64_i64(value.version)?,
                content, value.created_at.to_rfc3339(), value.updated_at.to_rfc3339(), now, actor],
        )?,
        Some(expected) => update_row(transaction,
            "UPDATE agent_member SET state=?1,current_collaboration_id=?2,version=?3,content=?4,
                updated_at=?5,update_time=?6,update_user=?7 WHERE id=?8 AND version=?9",
            params![value.state.as_str(), value.current_collaboration_id.map(|id| id.to_string()),
                u64_i64(value.version)?, content, value.updated_at.to_rfc3339(), now, actor,
                value.id.to_string(), u64_i64(expected)?])?,
    };
    Ok(())
}

fn write_collaboration(
    transaction: &Transaction<'_>,
    value: &Collaboration,
    expected: Option<u64>,
    actor: &str,
) -> MultiAgentResult<()> {
    let current = read_collaboration(transaction, value.id)?;
    validate_version(
        current.as_ref().map(|item| item.version),
        expected,
        value.version,
    )?;
    if let Some(current) = &current {
        if current.team_id != value.team_id
            || current.created_at != value.created_at
            || current.goal != value.goal
            || current.required_capabilities != value.required_capabilities
            || current.source_member_id != value.source_member_id
        {
            return Err(MultiAgentError::Conflict(
                "collaboration immutable request changed".into(),
            ));
        }
    }
    let content = serde_json::to_string(value)?;
    let dispatch_id = value
        .binding
        .as_ref()
        .map(|binding| binding.dispatch_id.to_string());
    let now = Utc::now().to_rfc3339();
    match expected {
        None => transaction.execute(
            "INSERT INTO collaboration (id,team_id,role_id,source_member_id,target_member_id,
                dispatch_id,state,priority,version,content,created_at,updated_at,create_time,
                update_time,create_user,update_user)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13,?14,?14)",
            params![
                value.id.to_string(),
                value.team_id.to_string(),
                value.role_id.map(|id| id.to_string()),
                value.source_member_id.map(|id| id.to_string()),
                value.target_member_id.to_string(),
                dispatch_id,
                value.state.as_str(),
                value.priority.as_str(),
                u64_i64(value.version)?,
                content,
                value.created_at.to_rfc3339(),
                value.updated_at.to_rfc3339(),
                now,
                actor
            ],
        )?,
        Some(expected) => update_row(
            transaction,
            "UPDATE collaboration SET role_id=?1,target_member_id=?2,dispatch_id=?3,state=?4,
                priority=?5,version=?6,content=?7,updated_at=?8,update_time=?9,update_user=?10
             WHERE id=?11 AND version=?12",
            params![
                value.role_id.map(|id| id.to_string()),
                value.target_member_id.to_string(),
                dispatch_id,
                value.state.as_str(),
                value.priority.as_str(),
                u64_i64(value.version)?,
                content,
                value.updated_at.to_rfc3339(),
                now,
                actor,
                value.id.to_string(),
                u64_i64(expected)?
            ],
        )?,
    };
    Ok(())
}

fn read_organization(connection: &Connection, id: Uuid) -> MultiAgentResult<Option<Organization>> {
    let raw = connection
        .query_row(
            "SELECT organization_key,name,version,content,created_at,updated_at,update_user
             FROM organization WHERE id=?1",
            [id.to_string()],
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
    let Some(raw) = raw else { return Ok(None) };
    let value: Organization = serde_json::from_str(&raw.3)?;
    value.validate()?;
    if value.id != id
        || value.key != raw.0
        || value.name != raw.1
        || value.version != i64_u64("organization version", raw.2)?
        || value.created_at != parse_time("organization created_at", &raw.4)?
        || value.updated_at != parse_time("organization updated_at", &raw.5)?
        || value.actor != raw.6
    {
        return Err(MultiAgentError::Validation(
            "organization structured columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_role(connection: &Connection, id: Uuid) -> MultiAgentResult<Option<Role>> {
    let raw = connection
        .query_row(
            "SELECT organization_id,role_key,name,version,content,created_at,updated_at,update_user
         FROM role WHERE id=?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: Role = serde_json::from_str(&raw.4)?;
    value.validate()?;
    if value.id != id
        || value.organization_id != parse_uuid("role organization", &raw.0)?
        || value.key != raw.1
        || value.name != raw.2
        || value.version != i64_u64("role version", raw.3)?
        || value.created_at != parse_time("role created_at", &raw.5)?
        || value.updated_at != parse_time("role updated_at", &raw.6)?
        || value.actor != raw.7
    {
        return Err(MultiAgentError::Validation(
            "role structured columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_team(connection: &Connection, id: Uuid) -> MultiAgentResult<Option<Team>> {
    let raw=connection.query_row(
        "SELECT organization_id,team_key,state,workspace_id,version,content,created_at,updated_at,update_user
         FROM team WHERE id=?1",[id.to_string()],|row|Ok((row.get::<_,String>(0)?,
            row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,
            row.get::<_,i64>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?,
            row.get::<_,String>(7)?,row.get::<_,String>(8)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: Team = serde_json::from_str(&raw.5)?;
    value.validate()?;
    if value.id != id
        || value.organization_id != parse_uuid("team organization", &raw.0)?
        || value.key != raw.1
        || value.state.as_str() != raw.2
        || value.workspace_id != parse_optional_uuid("team workspace", raw.3.as_deref())?
        || value.version != i64_u64("team version", raw.4)?
        || value.created_at != parse_time("team created_at", &raw.6)?
        || value.updated_at != parse_time("team updated_at", &raw.7)?
        || value.actor != raw.8
    {
        return Err(MultiAgentError::Validation(
            "team structured columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_member(connection: &Connection, id: Uuid) -> MultiAgentResult<Option<AgentMember>> {
    let raw=connection.query_row(
        "SELECT team_id,role_id,agent_id,state,current_collaboration_id,version,content,created_at,updated_at,update_user
         FROM agent_member WHERE id=?1",[id.to_string()],|row|Ok((row.get::<_,String>(0)?,
            row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,
            row.get::<_,Option<String>>(4)?,row.get::<_,i64>(5)?,row.get::<_,String>(6)?,
            row.get::<_,String>(7)?,row.get::<_,String>(8)?,row.get::<_,String>(9)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: AgentMember = serde_json::from_str(&raw.6)?;
    value.validate()?;
    if value.id != id
        || value.team_id != parse_uuid("member team", &raw.0)?
        || value.role_id != parse_uuid("member role", &raw.1)?
        || value.agent_id != parse_uuid("member agent", &raw.2)?
        || value.state.as_str() != raw.3
        || value.current_collaboration_id
            != parse_optional_uuid("member collaboration", raw.4.as_deref())?
        || value.version != i64_u64("member version", raw.5)?
        || value.created_at != parse_time("member created_at", &raw.7)?
        || value.updated_at != parse_time("member updated_at", &raw.8)?
        || value.actor != raw.9
    {
        return Err(MultiAgentError::Validation(
            "member structured columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_collaboration(
    connection: &Connection,
    id: Uuid,
) -> MultiAgentResult<Option<Collaboration>> {
    let raw=connection.query_row(
        "SELECT team_id,role_id,source_member_id,target_member_id,dispatch_id,state,priority,version,
            content,created_at,updated_at,update_user FROM collaboration WHERE id=?1",
        [id.to_string()],|row|Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,
            row.get::<_,Option<String>>(2)?,row.get::<_,String>(3)?,row.get::<_,Option<String>>(4)?,
            row.get::<_,String>(5)?,row.get::<_,String>(6)?,row.get::<_,i64>(7)?,
            row.get::<_,String>(8)?,row.get::<_,String>(9)?,row.get::<_,String>(10)?,
            row.get::<_,String>(11)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: Collaboration = serde_json::from_str(&raw.8)?;
    value.validate()?;
    let content_dispatch = value.binding.as_ref().map(|binding| binding.dispatch_id);
    if value.id != id
        || value.team_id != parse_uuid("collaboration team", &raw.0)?
        || value.role_id != parse_optional_uuid("collaboration role", raw.1.as_deref())?
        || value.source_member_id != parse_optional_uuid("collaboration source", raw.2.as_deref())?
        || value.target_member_id != parse_uuid("collaboration target", &raw.3)?
        || content_dispatch != parse_optional_uuid("collaboration dispatch", raw.4.as_deref())?
        || value.state.as_str() != raw.5
        || value.priority.as_str() != raw.6
        || value.version != i64_u64("collaboration version", raw.7)?
        || value.created_at != parse_time("collaboration created_at", &raw.9)?
        || value.updated_at != parse_time("collaboration updated_at", &raw.10)?
        || value.actor != raw.11
    {
        return Err(MultiAgentError::Validation(
            "collaboration structured columns do not match content".into(),
        ));
    }
    Ok(Some(value))
}

fn validate_commit_relations(
    connection: &Connection,
    commit: &CollaborationCommit,
) -> MultiAgentResult<()> {
    let team = &commit.team.value;
    let collaboration = &commit.collaboration.value;
    require_organization(connection, team.organization_id)?;
    if collaboration.team_id != team.id {
        return Err(MultiAgentError::Validation(
            "Collaboration does not belong to Team".into(),
        ));
    }
    let target = commit
        .members
        .iter()
        .find(|member| member.value.id == collaboration.target_member_id)
        .map(|member| &member.value)
        .ok_or_else(|| {
            MultiAgentError::Validation("Collaboration commit lacks target Member".into())
        })?;
    validate_member_owner_with_team(connection, target, team)?;
    let expected_owner = if collaboration.state.is_terminal() {
        None
    } else {
        Some(collaboration.id)
    };
    if target.current_collaboration_id != expected_owner
        || collaboration.role_id.is_some_and(|id| id != target.role_id)
    {
        return Err(MultiAgentError::Validation(
            "Collaboration target ownership is invalid".into(),
        ));
    }
    if let Some(source) = collaboration.source_member_id {
        let source =
            read_member(connection, source)?.ok_or_else(|| MultiAgentError::not_found(source))?;
        if source.team_id != team.id {
            return Err(MultiAgentError::Validation(
                "Collaboration source belongs to another Team".into(),
            ));
        }
    }
    let expected_member = match collaboration.state {
        CollaborationState::Assigned => MemberState::Assigned,
        CollaborationState::Working | CollaborationState::OutcomeUnknown => MemberState::Working,
        CollaborationState::Waiting => MemberState::Waiting,
        CollaborationState::Completed => MemberState::Completed,
        CollaborationState::Failed | CollaborationState::Cancelled => MemberState::Available,
    };
    let expected_team = if collaboration.state.is_terminal() {
        TeamState::Ready
    } else {
        TeamState::Active
    };
    if target.state != expected_member || team.state != expected_team {
        return Err(MultiAgentError::Validation(
            "Team or Member state mismatches Collaboration".into(),
        ));
    }
    Ok(())
}

fn validate_collaboration_owner(
    connection: &Connection,
    value: &Collaboration,
) -> MultiAgentResult<()> {
    let team = require_team(connection, value.team_id)?;
    let target = read_member(connection, value.target_member_id)?
        .ok_or_else(|| MultiAgentError::not_found(value.target_member_id))?;
    validate_member_owner_with_team(connection, &target, &team)?;
    if value.role_id.is_some_and(|id| id != target.role_id) {
        return Err(MultiAgentError::Validation(
            "Collaboration Role does not match target".into(),
        ));
    }
    Ok(())
}

fn validate_member_owner(connection: &Connection, value: &AgentMember) -> MultiAgentResult<()> {
    let team = require_team(connection, value.team_id)?;
    validate_member_owner_with_team(connection, value, &team)
}

fn validate_member_owner_with_team(
    connection: &Connection,
    value: &AgentMember,
    team: &Team,
) -> MultiAgentResult<()> {
    let role = read_role(connection, value.role_id)?
        .ok_or_else(|| MultiAgentError::not_found(value.role_id))?;
    if role.organization_id != team.organization_id {
        return Err(MultiAgentError::Validation(
            "Member Role and Team ownership mismatch".into(),
        ));
    }
    Ok(())
}

fn require_organization(connection: &Connection, id: Uuid) -> MultiAgentResult<Organization> {
    read_organization(connection, id)?.ok_or_else(|| MultiAgentError::not_found(id))
}

fn require_team(connection: &Connection, id: Uuid) -> MultiAgentResult<Team> {
    let value = read_team(connection, id)?.ok_or_else(|| MultiAgentError::not_found(id))?;
    require_organization(connection, value.organization_id)?;
    Ok(value)
}

fn validate_version(
    current: Option<u64>,
    expected: Option<u64>,
    next: u64,
) -> MultiAgentResult<()> {
    match (current, expected) {
        (None, None) if next == 1 => Ok(()),
        (Some(current), Some(expected))
            if current == expected && next == expected.saturating_add(1) =>
        {
            Ok(())
        }
        _ => Err(MultiAgentError::Conflict(
            "multi-agent optimistic version conflict".into(),
        )),
    }
}

fn update_row(
    transaction: &Transaction<'_>,
    sql: &str,
    values: impl rusqlite::Params,
) -> MultiAgentResult<usize> {
    let changed = transaction.execute(sql, values)?;
    if changed != 1 {
        return Err(MultiAgentError::Conflict("stale SQLite writer".into()));
    }
    Ok(changed)
}

fn query_ids<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> MultiAgentResult<Vec<Uuid>> {
    let mut statement = connection.prepare(sql)?;
    let values = statement
        .query_map(params, |row| row.get::<_, String>(0))?
        .map(|value| parse_uuid("row id", &value?))
        .collect();
    values
}

fn parse_uuid(label: &str, value: &str) -> MultiAgentResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|error| MultiAgentError::Validation(format!("invalid {label}: {error}")))
}

fn parse_optional_uuid(label: &str, value: Option<&str>) -> MultiAgentResult<Option<Uuid>> {
    value.map(|value| parse_uuid(label, value)).transpose()
}

fn parse_time(label: &str, value: &str) -> MultiAgentResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| MultiAgentError::Validation(format!("invalid {label}: {error}")))
}

fn u64_i64(value: u64) -> MultiAgentResult<i64> {
    i64::try_from(value)
        .map_err(|_| MultiAgentError::Validation("integer exceeds SQLite range".into()))
}

fn i64_u64(label: &str, value: i64) -> MultiAgentResult<u64> {
    u64::try_from(value).map_err(|_| MultiAgentError::Validation(format!("invalid {label}")))
}
