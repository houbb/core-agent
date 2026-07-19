use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{validate_actor, Snapshot, Workspace};
use crate::error::{WorkspaceError, WorkspaceResult};

use super::{WorkspaceCatalog, WorkspaceRegistry};

#[derive(Default)]
pub struct InMemoryWorkspaceRegistry {
    workspaces: RwLock<BTreeMap<Uuid, Workspace>>,
}

impl WorkspaceRegistry for InMemoryWorkspaceRegistry {
    fn register(&self, workspace: Workspace) -> WorkspaceResult<()> {
        let mut workspaces = self
            .workspaces
            .write()
            .map_err(|_| WorkspaceError::Internal("workspace registry lock poisoned".into()))?;
        if workspaces
            .values()
            .any(|current| current.uri == workspace.uri && current.id != workspace.id)
        {
            return Err(WorkspaceError::Conflict(format!(
                "URI `{}` is already registered",
                workspace.uri
            )));
        }
        workspaces.insert(workspace.id, workspace);
        Ok(())
    }

    fn find(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        Ok(self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace registry lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    fn find_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>> {
        Ok(self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace registry lock poisoned".into()))?
            .values()
            .find(|workspace| workspace.uri == uri)
            .cloned())
    }

    fn list(&self) -> WorkspaceResult<Vec<Workspace>> {
        let mut values = self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace registry lock poisoned".into()))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(values)
    }

    fn remove(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        Ok(self
            .workspaces
            .write()
            .map_err(|_| WorkspaceError::Internal("workspace registry lock poisoned".into()))?
            .remove(&id))
    }
}

#[derive(Default)]
pub struct InMemoryWorkspaceCatalog {
    workspaces: RwLock<BTreeMap<Uuid, Workspace>>,
    snapshots: RwLock<BTreeMap<Uuid, Snapshot>>,
}

#[async_trait]
impl WorkspaceCatalog for InMemoryWorkspaceCatalog {
    async fn save_workspace(&self, workspace: &Workspace, actor: &str) -> WorkspaceResult<()> {
        workspace.validate()?;
        validate_actor(actor)?;
        let mut workspaces = self
            .workspaces
            .write()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?;
        if workspaces
            .values()
            .any(|current| current.uri == workspace.uri && current.id != workspace.id)
        {
            return Err(WorkspaceError::Conflict(format!(
                "URI `{}` is already stored",
                workspace.uri
            )));
        }
        workspaces.insert(workspace.id, workspace.clone());
        Ok(())
    }

    async fn find_workspace(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        Ok(self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    async fn find_workspace_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>> {
        Ok(self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?
            .values()
            .find(|workspace| workspace.uri == uri)
            .cloned())
    }

    async fn list_workspaces(&self) -> WorkspaceResult<Vec<Workspace>> {
        let mut values = self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(values)
    }

    async fn remove_workspace(&self, id: Uuid) -> WorkspaceResult<bool> {
        let removed = self
            .workspaces
            .write()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?
            .remove(&id)
            .is_some();
        if removed {
            self.snapshots
                .write()
                .map_err(|_| WorkspaceError::Internal("snapshot catalog lock poisoned".into()))?
                .retain(|_, snapshot| snapshot.workspace_id != id);
        }
        Ok(removed)
    }

    async fn save_snapshot(&self, snapshot: &Snapshot, actor: &str) -> WorkspaceResult<()> {
        snapshot.validate()?;
        validate_actor(actor)?;
        if !self
            .workspaces
            .read()
            .map_err(|_| WorkspaceError::Internal("workspace catalog lock poisoned".into()))?
            .contains_key(&snapshot.workspace_id)
        {
            return Err(WorkspaceError::NotFound(snapshot.workspace_id.to_string()));
        }
        self.snapshots
            .write()
            .map_err(|_| WorkspaceError::Internal("snapshot catalog lock poisoned".into()))?
            .insert(snapshot.id, snapshot.clone());
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> WorkspaceResult<Option<Snapshot>> {
        Ok(self
            .snapshots
            .read()
            .map_err(|_| WorkspaceError::Internal("snapshot catalog lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    async fn list_snapshots(&self, workspace_id: Uuid) -> WorkspaceResult<Vec<Snapshot>> {
        let mut values = self
            .snapshots
            .read()
            .map_err(|_| WorkspaceError::Internal("snapshot catalog lock poisoned".into()))?
            .values()
            .filter(|snapshot| snapshot.workspace_id == workspace_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at));
        Ok(values)
    }

    async fn remove_snapshot(&self, id: Uuid) -> WorkspaceResult<bool> {
        Ok(self
            .snapshots
            .write()
            .map_err(|_| WorkspaceError::Internal("snapshot catalog lock poisoned".into()))?
            .remove(&id)
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn registry_rejects_two_identities_for_one_uri() {
        let registry = InMemoryWorkspaceRegistry::default();
        let first = Workspace::new("one", "local", "file:///one/", BTreeMap::new()).unwrap();
        let second = Workspace::new("two", "local", "file:///one/", BTreeMap::new()).unwrap();
        registry.register(first).unwrap();
        assert!(registry.register(second).is_err());
    }

    #[tokio::test]
    async fn catalog_rejects_two_identities_for_one_uri() {
        let catalog = InMemoryWorkspaceCatalog::default();
        let first = Workspace::new("one", "local", "file:///one/", BTreeMap::new()).unwrap();
        let second = Workspace::new("two", "local", "file:///one/", BTreeMap::new()).unwrap();
        catalog.save_workspace(&first, "tester").await.unwrap();
        assert!(catalog.save_workspace(&second, "tester").await.is_err());
    }
}
