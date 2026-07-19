use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use chrono::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Goal, Plan, PlanReview, PlanSnapshot, PlanningGraph, PlanningMetadata,
    PlanningNodeKind, Step, Task,
};
use crate::error::{PlanError, PlanResult};
use crate::infrastructure::{GoalStore, PlanSnapshotStore, PlanStore};

use super::schema::SCHEMA_SQL;

pub struct SqlitePlanningStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqlitePlanningStore {
    pub fn new(path: impl AsRef<Path>) -> PlanResult<Self> {
        let manager = if path.as_ref() == Path::new(":memory:") {
            SqliteConnectionManager::memory()
        } else {
            SqliteConnectionManager::file(path)
        };
        let pool = Pool::builder().max_size(1).build(manager)?;
        let store = Self { pool };
        store.pool.get()?.execute_batch(SCHEMA_SQL)?;
        Ok(store)
    }

    fn insert_snapshot(
        transaction: &Transaction<'_>,
        snapshot: &PlanSnapshot,
        actor: &str,
        audit_time: &str,
    ) -> PlanResult<()> {
        snapshot.validate()?;
        transaction.execute(
            "INSERT INTO plan_snapshot (
                id, plan_id, plan_version, label, content, hash, created_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)
             ",
            params![
                snapshot.id.to_string(),
                snapshot.plan_id.to_string(),
                to_i64(snapshot.plan_version, "snapshot version")?,
                snapshot.label,
                serde_json::to_string(snapshot)?,
                snapshot.hash,
                snapshot.created_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        Ok(())
    }

    fn insert_tasks(
        transaction: &Transaction<'_>,
        plan: &Plan,
        actor: &str,
        audit_time: &str,
    ) -> PlanResult<()> {
        for task in plan.tasks.values() {
            transaction.execute(
                "INSERT INTO task (
                    id, plan_id, task_key, name, status, priority, dependencies, content,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10)",
                params![
                    task.id.to_string(),
                    plan.id.to_string(),
                    task.key,
                    task.name,
                    task.status.as_str(),
                    i64::from(task.priority),
                    serde_json::to_string(&task.dependencies)?,
                    serde_json::to_string(task)?,
                    audit_time,
                    actor,
                ],
            )?;
            for step in task.steps.values() {
                transaction.execute(
                    "INSERT INTO step (
                        id, plan_id, task_id, step_key, name, status, dependencies,
                        action_kind, tool_key, content, create_time, update_time,
                        create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12)",
                    params![
                        step.id.to_string(),
                        plan.id.to_string(),
                        task.id.to_string(),
                        step.key,
                        step.name,
                        step.status.as_str(),
                        serde_json::to_string(&step.dependencies)?,
                        step.action.kind.as_str(),
                        step.action.tool_key,
                        serde_json::to_string(step)?,
                        audit_time,
                        actor,
                    ],
                )?;
            }
        }
        Ok(())
    }

    fn load_goal(connection: &Connection, id: Uuid) -> PlanResult<Option<Goal>> {
        type GoalRow = (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            i64,
            Option<String>,
            Option<String>,
            i64,
            String,
            String,
            String,
        );
        let row: Option<GoalRow> = connection
            .query_row(
                "SELECT content, id, intent_id, intent, title, status, priority, session_id,
                        workspace_id, version, metadata, created_at, updated_at
                 FROM goal WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                    ))
                },
            )
            .optional()?;
        row.map(|row| decode_goal(row, id)).transpose()
    }

    fn load_plan(connection: &Connection, id: Uuid) -> PlanResult<Option<Plan>> {
        type PlanRow = (
            String,
            String,
            String,
            String,
            String,
            i64,
            Option<String>,
            String,
            String,
            String,
            String,
        );
        let row: Option<PlanRow> = connection
            .query_row(
                "SELECT content, id, goal_id, strategy_key, status, version, review, metadata,
                        graph, created_at, updated_at
                 FROM plan WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                    ))
                },
            )
            .optional()?;
        let Some(row) = row else { return Ok(None) };
        let plan = decode_plan_base(row, id)?;
        let goal_intent = connection
            .query_row(
                "SELECT intent_id FROM goal WHERE id = ?1",
                [plan.goal_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| PlanError::Validation("plan references a missing goal".into()))?;
        ensure_intent_matches(goal_intent.as_deref(), &plan)?;
        let tasks = Self::load_tasks(connection, id)?;
        let steps = Self::load_steps(connection, id)?;
        if plan.tasks != tasks {
            return Err(PlanError::Validation(
                "plan content does not match task rows".into(),
            ));
        }
        let expected_steps = plan
            .tasks
            .values()
            .flat_map(|task| task.steps.iter().map(|(id, step)| (*id, step.clone())))
            .collect::<BTreeMap<_, _>>();
        if steps != expected_steps {
            return Err(PlanError::Validation(
                "plan content does not match step rows".into(),
            ));
        }
        plan.validate()?;
        Ok(Some(plan))
    }

    fn load_tasks(connection: &Connection, plan_id: Uuid) -> PlanResult<BTreeMap<Uuid, Task>> {
        type TaskRow = (String, String, String, String, String, String, i64, String);
        let rows = {
            let mut statement = connection.prepare(
                "SELECT content, id, plan_id, task_key, name, status, priority, dependencies
                 FROM task WHERE plan_id = ?1 ORDER BY task_key, id",
            )?;
            let values = statement
                .query_map([plan_id.to_string()], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                })?
                .collect::<Result<Vec<TaskRow>, _>>()?;
            values
        };
        let mut tasks = BTreeMap::new();
        for (content, id, stored_plan_id, key, name, status, priority, dependencies) in rows {
            let task: Task = serde_json::from_str(&content)?;
            let stored_dependencies: Vec<Uuid> = serde_json::from_str(&dependencies)?;
            let priority = i32::try_from(priority)
                .map_err(|_| PlanError::Validation("task priority exceeds i32".into()))?;
            let matches = task.id.to_string() == id
                && task.plan_id == plan_id
                && stored_plan_id == plan_id.to_string()
                && task.key == key
                && task.name == name
                && task.status.as_str() == status
                && task.priority == priority
                && task.dependencies == stored_dependencies;
            if !matches || tasks.insert(task.id, task.clone()).is_some() {
                return Err(PlanError::Validation(
                    "task columns do not match serialized entity".into(),
                ));
            }
            task.validate()?;
        }
        Ok(tasks)
    }

    fn load_steps(connection: &Connection, plan_id: Uuid) -> PlanResult<BTreeMap<Uuid, Step>> {
        type StepRow = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
        );
        let rows = {
            let mut statement = connection.prepare(
                "SELECT content, id, plan_id, task_id, step_key, name, status, dependencies,
                        tool_key, action_kind
                 FROM step WHERE plan_id = ?1 ORDER BY step_key, id",
            )?;
            let values = statement
                .query_map([plan_id.to_string()], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                })?
                .collect::<Result<Vec<StepRow>, _>>()?;
            values
        };
        let mut steps = BTreeMap::new();
        for (
            content,
            id,
            stored_plan_id,
            task_id,
            key,
            name,
            status,
            dependencies,
            tool_key,
            action_kind,
        ) in rows
        {
            let step: Step = serde_json::from_str(&content)?;
            let stored_dependencies: Vec<Uuid> = serde_json::from_str(&dependencies)?;
            let matches = step.id.to_string() == id
                && step.plan_id == plan_id
                && stored_plan_id == plan_id.to_string()
                && step.task_id.to_string() == task_id
                && step.key == key
                && step.name == name
                && step.status.as_str() == status
                && step.dependencies == stored_dependencies
                && step.action.tool_key == tool_key
                && step.action.kind.as_str() == action_kind;
            if !matches || steps.insert(step.id, step.clone()).is_some() {
                return Err(PlanError::Validation(
                    "step columns do not match serialized entity".into(),
                ));
            }
            step.validate()?;
        }
        Ok(steps)
    }
}

#[async_trait]
impl GoalStore for SqlitePlanningStore {
    async fn save_goal(&self, goal: &Goal, actor: &str) -> PlanResult<()> {
        goal.validate()?;
        validate_actor(actor)?;
        let audit_time = Utc::now().to_rfc3339();
        let mut connection = self.pool.get()?;
        let transaction = connection.transaction()?;
        let current_version = transaction
            .query_row(
                "SELECT version FROM goal WHERE id = ?1",
                [goal.id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|version| to_u64(version, "goal version"))
            .transpose()?;
        let valid_version = match current_version {
            None => goal.version == 1,
            Some(current) => current.checked_add(1) == Some(goal.version),
        };
        if !valid_version {
            return Err(PlanError::Conflict(format!(
                "goal {} was concurrently modified",
                goal.id
            )));
        }
        transaction.execute(
            "INSERT INTO goal (
                id, intent_id, intent, title, status, priority, session_id, workspace_id,
                version, metadata, content, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?15)
             ON CONFLICT(id) DO UPDATE SET
                intent_id=excluded.intent_id, intent=excluded.intent, title=excluded.title,
                status=excluded.status, priority=excluded.priority, session_id=excluded.session_id,
                workspace_id=excluded.workspace_id, version=excluded.version,
                metadata=excluded.metadata, content=excluded.content,
                updated_at=excluded.updated_at, update_time=excluded.update_time,
                update_user=excluded.update_user",
            params![
                goal.id.to_string(),
                goal.intent.as_ref().map(|intent| intent.id.to_string()),
                goal.intent
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                goal.title,
                goal.status.as_str(),
                i64::from(goal.priority),
                goal.session_id.map(|id| id.to_string()),
                goal.workspace_id.map(|id| id.to_string()),
                to_i64(goal.version, "goal version")?,
                serde_json::to_string(&goal.metadata)?,
                serde_json::to_string(goal)?,
                goal.created_at.to_rfc3339(),
                goal.updated_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_goal(&self, id: Uuid) -> PlanResult<Option<Goal>> {
        let connection = self.pool.get()?;
        Self::load_goal(&connection, id)
    }

    async fn list_goals(&self) -> PlanResult<Vec<Goal>> {
        let connection = self.pool.get()?;
        let ids = {
            let mut statement = connection
                .prepare("SELECT id FROM goal ORDER BY priority DESC, updated_at DESC, id")?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                let id = parse_uuid(&id, "goal")?;
                Self::load_goal(&connection, id)?
                    .ok_or_else(|| PlanError::Internal("goal disappeared during list".into()))
            })
            .collect()
    }
}

#[async_trait]
impl PlanStore for SqlitePlanningStore {
    async fn save_plan(
        &self,
        plan: &Plan,
        previous: Option<&PlanSnapshot>,
        actor: &str,
    ) -> PlanResult<()> {
        plan.validate()?;
        validate_actor(actor)?;
        if let Some(snapshot) = previous {
            snapshot.validate()?;
            if snapshot.plan_id != plan.id {
                return Err(PlanError::Validation(
                    "atomic snapshot belongs to another plan".into(),
                ));
            }
            if plan.goal_id != snapshot.content.goal_id
                || plan.created_at != snapshot.content.created_at
            {
                return Err(PlanError::Validation(
                    "plan update changed immutable goal or creation time".into(),
                ));
            }
        }
        let audit_time = Utc::now().to_rfc3339();
        let mut connection = self.pool.get()?;
        let transaction = connection.transaction()?;
        let goal_intent = transaction
            .query_row(
                "SELECT intent_id FROM goal WHERE id = ?1",
                [plan.goal_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| PlanError::NotFound(plan.goal_id.to_string()))?;
        ensure_intent_matches(goal_intent.as_deref(), plan)?;
        let current = transaction
            .query_row(
                "SELECT content, version FROM plan WHERE id = ?1",
                [plan.id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let valid_version = match (current, previous) {
            (None, None) => plan.version == 1,
            (Some((content, version)), Some(snapshot)) => {
                let current: Plan = serde_json::from_str(&content)?;
                let version = to_u64(version, "plan version")?;
                current == snapshot.content
                    && version == snapshot.plan_version
                    && version.checked_add(1) == Some(plan.version)
            }
            _ => false,
        };
        if !valid_version {
            return Err(PlanError::Conflict(format!(
                "plan {} was concurrently modified",
                plan.id
            )));
        }
        transaction.execute(
            "INSERT INTO plan (
                id, goal_id, strategy_key, status, version, review, metadata, graph, content,
                created_at, updated_at, create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?13)
             ON CONFLICT(id) DO UPDATE SET
                goal_id=excluded.goal_id, strategy_key=excluded.strategy_key,
                status=excluded.status, version=excluded.version, review=excluded.review,
                metadata=excluded.metadata, graph=excluded.graph, content=excluded.content,
                updated_at=excluded.updated_at, update_time=excluded.update_time,
                update_user=excluded.update_user",
            params![
                plan.id.to_string(),
                plan.goal_id.to_string(),
                plan.strategy_key,
                plan.status.as_str(),
                to_i64(plan.version, "plan version")?,
                plan.review
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                serde_json::to_string(&plan.metadata)?,
                serde_json::to_string(&plan.graph)?,
                serde_json::to_string(plan)?,
                plan.created_at.to_rfc3339(),
                plan.updated_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        transaction.execute("DELETE FROM step WHERE plan_id = ?1", [plan.id.to_string()])?;
        transaction.execute("DELETE FROM task WHERE plan_id = ?1", [plan.id.to_string()])?;
        Self::insert_tasks(&transaction, plan, actor, &audit_time)?;
        if let Some(snapshot) = previous {
            Self::insert_snapshot(&transaction, snapshot, actor, &audit_time)?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn find_plan(&self, id: Uuid) -> PlanResult<Option<Plan>> {
        let connection = self.pool.get()?;
        Self::load_plan(&connection, id)
    }

    async fn list_plans(&self, goal_id: Uuid) -> PlanResult<Vec<Plan>> {
        let connection = self.pool.get()?;
        let ids = {
            let mut statement = connection
                .prepare("SELECT id FROM plan WHERE goal_id = ?1 ORDER BY updated_at DESC, id")?;
            let values = statement
                .query_map([goal_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                let id = parse_uuid(&id, "plan")?;
                Self::load_plan(&connection, id)?
                    .ok_or_else(|| PlanError::Internal("plan disappeared during list".into()))
            })
            .collect()
    }
}

#[async_trait]
impl PlanSnapshotStore for SqlitePlanningStore {
    async fn save_snapshot(&self, snapshot: &PlanSnapshot, actor: &str) -> PlanResult<()> {
        snapshot.validate()?;
        validate_actor(actor)?;
        let audit_time = Utc::now().to_rfc3339();
        let mut connection = self.pool.get()?;
        let transaction = connection.transaction()?;
        let plan_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM plan WHERE id = ?1)",
            [snapshot.plan_id.to_string()],
            |row| row.get(0),
        )?;
        if !plan_exists {
            return Err(PlanError::NotFound(snapshot.plan_id.to_string()));
        }
        let snapshot_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM plan_snapshot WHERE id = ?1)",
            [snapshot.id.to_string()],
            |row| row.get(0),
        )?;
        if snapshot_exists {
            return Err(PlanError::Conflict(format!(
                "snapshot {} already exists",
                snapshot.id
            )));
        }
        Self::insert_snapshot(&transaction, snapshot, actor, &audit_time)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> PlanResult<Option<PlanSnapshot>> {
        let connection = self.pool.get()?;
        load_snapshot(&connection, "WHERE id = ?1", id.to_string())?
            .into_iter()
            .next()
            .map(|snapshot| {
                if snapshot.id != id {
                    return Err(PlanError::Validation(
                        "snapshot query identity mismatch".into(),
                    ));
                }
                Ok(snapshot)
            })
            .transpose()
    }

    async fn list_snapshots(&self, plan_id: Uuid) -> PlanResult<Vec<PlanSnapshot>> {
        let connection = self.pool.get()?;
        let snapshots = load_snapshot(
            &connection,
            "WHERE plan_id = ?1 ORDER BY created_at DESC, id",
            plan_id.to_string(),
        )?;
        if snapshots.iter().any(|snapshot| snapshot.plan_id != plan_id) {
            return Err(PlanError::Validation(
                "snapshot query plan identity mismatch".into(),
            ));
        }
        Ok(snapshots)
    }
}

type GoalRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    i64,
    String,
    String,
    String,
);

fn decode_goal(row: GoalRow, expected_id: Uuid) -> PlanResult<Goal> {
    let (
        content,
        id,
        intent_id,
        intent,
        title,
        status,
        priority,
        session_id,
        workspace_id,
        version,
        metadata,
        created_at,
        updated_at,
    ) = row;
    let goal: Goal = serde_json::from_str(&content)?;
    let priority = i32::try_from(priority)
        .map_err(|_| PlanError::Validation("goal priority exceeds i32".into()))?;
    let version = to_u64(version, "goal version")?;
    let stored_intent = intent.as_deref().map(serde_json::from_str).transpose()?;
    let matches = goal.id == expected_id
        && goal.id.to_string() == id
        && goal.intent.as_ref().map(|value| value.id.to_string()) == intent_id
        && goal.intent == stored_intent
        && goal.title == title
        && goal.status.as_str() == status
        && goal.priority == priority
        && goal.session_id.map(|value| value.to_string()) == session_id
        && goal.workspace_id.map(|value| value.to_string()) == workspace_id
        && goal.version == version
        && goal.metadata == serde_json::from_str::<PlanningMetadata>(&metadata)?
        && goal.created_at.to_rfc3339() == created_at
        && goal.updated_at.to_rfc3339() == updated_at;
    if !matches {
        return Err(PlanError::Validation(
            "goal columns do not match serialized entity".into(),
        ));
    }
    goal.validate()?;
    Ok(goal)
}

type PlanRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    String,
    String,
    String,
    String,
);

fn decode_plan_base(row: PlanRow, expected_id: Uuid) -> PlanResult<Plan> {
    let (
        content,
        id,
        goal_id,
        strategy_key,
        status,
        version,
        review,
        metadata,
        graph,
        created_at,
        updated_at,
    ) = row;
    let plan: Plan = serde_json::from_str(&content)?;
    let version = to_u64(version, "plan version")?;
    let stored_review: Option<PlanReview> =
        review.as_deref().map(serde_json::from_str).transpose()?;
    let matches = plan.id == expected_id
        && plan.id.to_string() == id
        && plan.goal_id.to_string() == goal_id
        && plan.strategy_key == strategy_key
        && plan.status.as_str() == status
        && plan.version == version
        && plan.review == stored_review
        && plan.metadata == serde_json::from_str::<PlanningMetadata>(&metadata)?
        && plan.graph == serde_json::from_str::<PlanningGraph>(&graph)?
        && plan.created_at.to_rfc3339() == created_at
        && plan.updated_at.to_rfc3339() == updated_at;
    if !matches {
        return Err(PlanError::Validation(
            "plan columns do not match serialized entity".into(),
        ));
    }
    Ok(plan)
}

fn load_snapshot(
    connection: &Connection,
    suffix: &str,
    value: String,
) -> PlanResult<Vec<PlanSnapshot>> {
    let sql = format!(
        "SELECT content, id, plan_id, plan_version, label, hash, created_at FROM plan_snapshot {suffix}"
    );
    let rows = {
        let mut statement = connection.prepare(&sql)?;
        let values = statement
            .query_map([value], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        values
    };
    rows.into_iter()
        .map(
            |(content, id, plan_id, plan_version, label, hash, created_at)| {
                let snapshot: PlanSnapshot = serde_json::from_str(&content)?;
                let matches = snapshot.id.to_string() == id
                    && snapshot.plan_id.to_string() == plan_id
                    && snapshot.plan_version == to_u64(plan_version, "snapshot version")?
                    && snapshot.label == label
                    && snapshot.hash == hash
                    && snapshot.created_at.to_rfc3339() == created_at;
                if !matches {
                    return Err(PlanError::Validation(
                        "snapshot columns do not match serialized entity".into(),
                    ));
                }
                snapshot.validate()?;
                Ok(snapshot)
            },
        )
        .collect()
}

fn parse_uuid(value: &str, entity: &str) -> PlanResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|error| PlanError::Validation(format!("invalid {entity} UUID: {error}")))
}

fn ensure_intent_matches(stored_intent_id: Option<&str>, plan: &Plan) -> PlanResult<()> {
    let plan_intent_id = plan
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == PlanningNodeKind::Intent)
        .map(|node| node.id.to_string());
    if plan_intent_id.as_deref() != stored_intent_id {
        return Err(PlanError::Validation(
            "plan graph intent does not match its goal".into(),
        ));
    }
    Ok(())
}

fn to_i64(value: u64, name: &str) -> PlanResult<i64> {
    i64::try_from(value).map_err(|_| PlanError::Validation(format!("{name} exceeds i64")))
}

fn to_u64(value: i64, name: &str) -> PlanResult<u64> {
    u64::try_from(value).map_err(|_| PlanError::Validation(format!("negative {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CreateGoalRequest, CreatePlanRequest, PlanningContext};
    use crate::manager::PlanningManager;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    #[tokio::test]
    async fn sqlite_round_trip_preserves_plan_hierarchy() {
        let store = Arc::new(SqlitePlanningStore::new(":memory:").unwrap());
        let manager = PlanningManager::new(store.clone());
        let goal = manager
            .create_goal(CreateGoalRequest::new("plan", "persist it"))
            .await
            .unwrap();
        let plan = manager
            .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
            .await
            .unwrap();
        let loaded = store.find_plan(plan.id).await.unwrap().unwrap();
        assert_eq!(loaded, plan);
        assert_eq!(loaded.tasks.len(), 3);
    }

    #[test]
    fn all_tables_have_audit_columns_indexes_and_no_foreign_keys() {
        let store = SqlitePlanningStore::new(":memory:").unwrap();
        let connection = store.pool.get().unwrap();
        for table in ["goal", "plan", "task", "step", "plan_snapshot"] {
            let columns = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<BTreeSet<_>, _>>()
                .unwrap();
            for required in [
                "id",
                "create_time",
                "update_time",
                "create_user",
                "update_user",
            ] {
                assert!(columns.contains(required), "{table}.{required} is missing");
            }
            let foreign_keys: i64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(foreign_keys, 0, "{table} has a foreign key");
        }
        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_count >= 13);
    }

    #[tokio::test]
    async fn corrupt_structured_plan_column_is_reported() {
        let store = Arc::new(SqlitePlanningStore::new(":memory:").unwrap());
        let manager = PlanningManager::new(store.clone());
        let goal = manager
            .create_goal(CreateGoalRequest::new("plan", "persist it"))
            .await
            .unwrap();
        let plan = manager
            .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
            .await
            .unwrap();
        store
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE plan SET status = 'FAILED' WHERE id = ?1",
                [plan.id.to_string()],
            )
            .unwrap();
        assert!(store.find_plan(plan.id).await.is_err());
    }
}
