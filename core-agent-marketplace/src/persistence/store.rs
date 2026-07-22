use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    MarketplacePackage, MarketplaceQuery, MarketplaceSnapshot, validate_actor,
};
use crate::error::{MarketplaceError, MarketplaceResult};
use crate::infrastructure::MarketplaceStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteMarketplaceStore {
    connection: Mutex<Connection>,
}

impl SqliteMarketplaceStore {
    pub fn new(path: impl AsRef<Path>) -> MarketplaceResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> MarketplaceResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> MarketplaceResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> MarketplaceResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| MarketplaceError::Internal("marketplace SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl MarketplaceStore for SqliteMarketplaceStore {
    async fn record(&self, pkg: &MarketplacePackage, actor: &str) -> MarketplaceResult<()> {
        validate_actor(actor)?;
        pkg.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM marketplace_package WHERE id = ?1",
                [pkg.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(MarketplaceError::Conflict(
                "package already exists".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO marketplace_package (
                id, asset_type, name, key, version, author, description,
                state, rating, downloads, tags, content, metadata,
                version_count, actor, content_json, created_at, updated_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?14, ?15, ?16, ?17, ?17, ?18, ?18, ?19, ?19)",
            params![
                pkg.id.to_string(),
                pkg.asset_type.as_str(),
                pkg.name,
                pkg.key,
                pkg.version,
                pkg.author,
                pkg.description,
                pkg.state.as_str(),
                pkg.rating,
                u64_i64(pkg.downloads)?,
                serde_json::to_string(&pkg.tags)?,
                serde_json::to_string(&pkg.content)?,
                serde_json::to_string(&pkg.metadata)?,
                u64_i64(pkg.version_count)?,
                pkg.actor,
                serde_json::to_string(pkg)?,
                pkg.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> MarketplaceResult<Option<MarketplacePackage>> {
        let connection = self.lock()?;
        let raw: Option<(
            String, String, String, String, String, String, String, String, f64, i64, String,
            String, String, i64, String, String, String,
        )> = connection
            .query_row(
                "SELECT id, asset_type, name, key, version, author, description,
                        state, rating, downloads, tags, content, metadata,
                        version_count, actor, content_json, created_at
                 FROM marketplace_package WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                        row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?, row.get(16)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: MarketplacePackage = serde_json::from_str(&raw.15)?;
        value.validate()?;
        Ok(Some(value))
    }

    async fn find_by_key(
        &self,
        key: &str,
        version: &str,
    ) -> MarketplaceResult<Option<MarketplacePackage>> {
        let id: Option<String> = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT id FROM marketplace_package WHERE key = ?1 AND version = ?2",
                    [key, version],
                    |row| row.get(0),
                )
                .optional()?
        };
        match id {
            Some(id_str) => {
                let uuid = Uuid::parse_str(&id_str)
                    .map_err(|e| MarketplaceError::Validation(format!("invalid uuid: {e}")))?;
                self.find(uuid).await
            }
            None => Ok(None),
        }
    }

    async fn list(&self, query: &MarketplaceQuery) -> MarketplaceResult<Vec<MarketplacePackage>> {
        query.validate()?;
        let ids = {
            let connection = self.lock()?;
            list_ids_sync(&connection, query)?
        };
        let mut packages = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| MarketplaceError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(pkg) = self.find(uuid).await? {
                packages.push(pkg);
            }
        }
        Ok(packages)
    }

    async fn count(&self, query: &MarketplaceQuery) -> MarketplaceResult<u64> {
        query.validate()?;
        let connection = self.lock()?;
        let mut sql = String::from("SELECT COUNT(*) FROM marketplace_package");
        let mut clauses: Vec<String> = Vec::new();
        if let Some(asset_type) = &query.asset_type {
            clauses.push(format!("asset_type = '{}'", asset_type.as_str()));
        }
        if let Some(state) = &query.state {
            clauses.push(format!("state = '{}'", state.as_str()));
        }
        if !clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
        }
        let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as u64)
    }

    async fn snapshot(&self) -> MarketplaceResult<MarketplaceSnapshot> {
        let connection = self.lock()?;
        let total: i64 =
            connection.query_row("SELECT COUNT(*) FROM marketplace_package", [], |row| {
                row.get(0)
            })?;
        let downloads: i64 = connection.query_row(
            "SELECT COALESCE(SUM(downloads), 0) FROM marketplace_package",
            [],
            |row| row.get(0),
        )?;
        let avg_rating: f64 = connection.query_row(
            "SELECT COALESCE(AVG(rating), 0.0) FROM marketplace_package",
            [],
            |row| row.get(0),
        )?;

        let mut by_type = std::collections::BTreeMap::new();
        let mut statement =
            connection.prepare("SELECT asset_type, COUNT(*) FROM marketplace_package GROUP BY asset_type")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        for row in rows {
            let (k, v) = row?;
            by_type.insert(k, v);
        }

        let mut by_state = std::collections::BTreeMap::new();
        let mut statement =
            connection.prepare("SELECT state, COUNT(*) FROM marketplace_package GROUP BY state")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        for row in rows {
            let (k, v) = row?;
            by_state.insert(k, v);
        }

        Ok(MarketplaceSnapshot {
            total_packages: total as u64,
            by_type,
            by_state,
            total_downloads: downloads as u64,
            avg_rating: (avg_rating * 100.0).round() / 100.0,
        })
    }
}

fn u64_i64(value: u64) -> MarketplaceResult<i64> {
    i64::try_from(value)
        .map_err(|_| MarketplaceError::Validation("integer exceeds SQLite range".into()))
}

fn list_ids_sync(connection: &Connection, query: &MarketplaceQuery) -> MarketplaceResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM marketplace_package");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(asset_type) = &query.asset_type {
        clauses.push(format!("asset_type = '{}'", asset_type.as_str()));
    }
    if let Some(state) = &query.state {
        clauses.push(format!("state = '{}'", state.as_str()));
    }
    if let Some(author) = &query.author {
        clauses.push(format!("author = '{}'", author.replace('\'', "''")));
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    sql.push_str(&format!(
        " ORDER BY rating DESC, id LIMIT {} OFFSET {}",
        query.limit, query.offset
    ));
    let mut statement = connection.prepare(&sql)?;
    let result: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}