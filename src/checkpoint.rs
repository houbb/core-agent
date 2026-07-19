use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::enterprise::blocked_workspace_name;

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_TURN_BYTES: usize = 16 * 1024 * 1024;
const MAX_CHANGES_PER_TURN: usize = 256;
const MAX_HISTORY: usize = 20;

pub(crate) type CheckpointResult<T> = Result<T, CheckpointError>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CheckpointError {
    #[error("checkpoint conflict: {0}")]
    Conflict(String),
    #[error("checkpoint data is invalid: {0}")]
    Invalid(String),
    #[error("checkpoint I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("checkpoint serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileChange {
    path: String,
    before: Option<String>,
    before_sha256: Option<String>,
    after: Option<String>,
    after_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    id: Uuid,
    created_at: String,
    changes: Vec<FileChange>,
}

impl Checkpoint {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: chrono::Utc::now().to_rfc3339(),
            changes: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointState {
    version: u32,
    session_id: Uuid,
    active: Option<Checkpoint>,
    undo: Vec<Checkpoint>,
    redo: Vec<Checkpoint>,
}

impl CheckpointState {
    fn new(session_id: Uuid) -> Self {
        Self {
            version: FORMAT_VERSION,
            session_id,
            active: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn validate(&self, session_id: Uuid) -> CheckpointResult<()> {
        if self.version != FORMAT_VERSION || self.session_id != session_id {
            return Err(CheckpointError::Invalid(
                "version or session identity does not match".into(),
            ));
        }
        if self.undo.len() > MAX_HISTORY || self.redo.len() > MAX_HISTORY {
            return Err(CheckpointError::Invalid(
                "history exceeds the supported bound".into(),
            ));
        }
        for checkpoint in self
            .active
            .iter()
            .chain(self.undo.iter())
            .chain(self.redo.iter())
        {
            validate_checkpoint(checkpoint)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingWrite {
    session_id: Uuid,
    checkpoint_id: Uuid,
    path: String,
    before: Option<String>,
    before_sha256: Option<String>,
    after: String,
    after_sha256: String,
}

pub(crate) struct PreparedWrite {
    session_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckpointOutcome {
    pub checkpoint_id: Uuid,
    pub files: usize,
}

pub(crate) struct CheckpointStore {
    workspace: PathBuf,
    directory: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl CheckpointStore {
    pub fn new(workspace: &Path, directory: PathBuf) -> CheckpointResult<Self> {
        let workspace = std::fs::canonicalize(workspace)?;
        if !workspace.is_dir() {
            return Err(CheckpointError::Invalid(
                "workspace is not a directory".into(),
            ));
        }
        std::fs::create_dir_all(&directory)?;
        Ok(Self {
            workspace,
            directory,
            lock: std::sync::Mutex::new(()),
        })
    }

    pub fn begin_turn(&self, session_id: Uuid) -> CheckpointResult<()> {
        let _guard = self.lock()?;
        self.recover_pending(session_id)?;
        let mut state = self.load_state(session_id)?;
        finish_active(&mut state);
        state.active = Some(Checkpoint::new());
        self.save_state(&state)
    }

    pub fn prepare_write(
        &self,
        session_id: Uuid,
        relative: &str,
        after: &str,
    ) -> CheckpointResult<PreparedWrite> {
        let _guard = self.lock()?;
        if after.len() > MAX_FILE_BYTES {
            return Err(CheckpointError::Invalid(
                "file content exceeds 256 KiB".into(),
            ));
        }
        self.recover_pending(session_id)?;
        let mut state = self.load_state(session_id)?;
        let checkpoint = state.active.get_or_insert_with(Checkpoint::new);
        let relative = relative.replace('\\', "/");
        let path = self.safe_path(&relative)?;
        let before = read_optional_text(&path)?;
        let after = after.to_owned();
        let projected = projected_turn_size(checkpoint, &relative, &before, &after);
        if projected > MAX_TURN_BYTES {
            return Err(CheckpointError::Invalid(
                "checkpoint turn exceeds 16 MiB".into(),
            ));
        }
        if checkpoint.changes.len() >= MAX_CHANGES_PER_TURN
            && !checkpoint.changes.iter().any(|item| item.path == relative)
        {
            return Err(CheckpointError::Invalid(
                "checkpoint turn exceeds 256 files".into(),
            ));
        }
        let checkpoint_id = checkpoint.id;
        self.save_state(&state)?;
        let pending = PendingWrite {
            session_id,
            checkpoint_id,
            path: relative,
            before_sha256: content_hash(before.as_deref()),
            before,
            after_sha256: hash(after.as_bytes()),
            after,
        };
        self.atomic_write(
            &self.pending_file(session_id),
            &serde_json::to_vec(&pending)?,
        )?;
        Ok(PreparedWrite { session_id })
    }

    pub fn commit_write(&self, prepared: PreparedWrite) -> CheckpointResult<()> {
        let _guard = self.lock()?;
        self.recover_pending(prepared.session_id)
    }

    pub fn abort_write(&self, prepared: PreparedWrite) -> CheckpointResult<()> {
        let _guard = self.lock()?;
        let pending_path = self.pending_file(prepared.session_id);
        if !pending_path.exists() {
            return Ok(());
        }
        let pending: PendingWrite = serde_json::from_slice(&std::fs::read(&pending_path)?)?;
        let current = read_optional_text(&self.safe_path(&pending.path)?)?;
        if content_hash(current.as_deref()) != pending.before_sha256 {
            return Err(CheckpointError::Conflict(format!(
                "{} changed while a write was being aborted",
                pending.path
            )));
        }
        std::fs::remove_file(pending_path)?;
        Ok(())
    }

    pub fn finish_turn(&self, session_id: Uuid) -> CheckpointResult<Option<CheckpointOutcome>> {
        let _guard = self.lock()?;
        self.recover_pending(session_id)?;
        let mut state = self.load_state(session_id)?;
        let outcome = state.active.as_ref().and_then(|checkpoint| {
            (!checkpoint.changes.is_empty()).then_some(CheckpointOutcome {
                checkpoint_id: checkpoint.id,
                files: checkpoint.changes.len(),
            })
        });
        finish_active(&mut state);
        self.save_state(&state)?;
        Ok(outcome)
    }

    pub fn undo(&self, session_id: Uuid) -> CheckpointResult<Option<CheckpointOutcome>> {
        let _guard = self.lock()?;
        self.recover_pending(session_id)?;
        let mut state = self.load_state(session_id)?;
        finish_active(&mut state);
        let Some(checkpoint) = state.undo.pop() else {
            self.save_state(&state)?;
            return Ok(None);
        };
        self.apply(&checkpoint, false)?;
        let outcome = CheckpointOutcome {
            checkpoint_id: checkpoint.id,
            files: checkpoint.changes.len(),
        };
        state.redo.push(checkpoint);
        trim_history(&mut state.redo);
        self.save_state(&state)?;
        Ok(Some(outcome))
    }

    pub fn redo(&self, session_id: Uuid) -> CheckpointResult<Option<CheckpointOutcome>> {
        let _guard = self.lock()?;
        self.recover_pending(session_id)?;
        let mut state = self.load_state(session_id)?;
        finish_active(&mut state);
        let Some(checkpoint) = state.redo.pop() else {
            self.save_state(&state)?;
            return Ok(None);
        };
        self.apply(&checkpoint, true)?;
        let outcome = CheckpointOutcome {
            checkpoint_id: checkpoint.id,
            files: checkpoint.changes.len(),
        };
        state.undo.push(checkpoint);
        trim_history(&mut state.undo);
        self.save_state(&state)?;
        Ok(Some(outcome))
    }

    fn recover_pending(&self, session_id: Uuid) -> CheckpointResult<()> {
        let path = self.pending_file(session_id);
        if !path.exists() {
            return Ok(());
        }
        let pending: PendingWrite = serde_json::from_slice(&std::fs::read(&path)?)?;
        if pending.session_id != session_id {
            return Err(CheckpointError::Invalid(
                "pending write belongs to another session".into(),
            ));
        }
        let current = read_optional_text(&self.safe_path(&pending.path)?)?;
        let current_hash = content_hash(current.as_deref());
        if current_hash == pending.before_sha256 {
            std::fs::remove_file(path)?;
            return Ok(());
        }
        if current_hash != Some(pending.after_sha256.clone()) {
            return Err(CheckpointError::Conflict(format!(
                "{} no longer matches either side of the pending write",
                pending.path
            )));
        }
        let mut state = self.load_state(session_id)?;
        let checkpoint = state.active.as_mut().ok_or_else(|| {
            CheckpointError::Invalid("pending write has no active checkpoint".into())
        })?;
        if checkpoint.id != pending.checkpoint_id {
            return Err(CheckpointError::Invalid(
                "pending write checkpoint identity does not match".into(),
            ));
        }
        if let Some(change) = checkpoint
            .changes
            .iter_mut()
            .find(|change| change.path == pending.path)
        {
            change.after = Some(pending.after);
            change.after_sha256 = Some(pending.after_sha256);
        } else {
            checkpoint.changes.push(FileChange {
                path: pending.path,
                before: pending.before,
                before_sha256: pending.before_sha256,
                after: Some(pending.after),
                after_sha256: Some(pending.after_sha256),
            });
        }
        state.redo.clear();
        self.save_state(&state)?;
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn apply(&self, checkpoint: &Checkpoint, forward: bool) -> CheckpointResult<()> {
        let mut operations = Vec::with_capacity(checkpoint.changes.len());
        for change in &checkpoint.changes {
            let path = self.safe_path(&change.path)?;
            let current = read_optional_text(&path)?;
            let expected = if forward {
                &change.before_sha256
            } else {
                &change.after_sha256
            };
            if &content_hash(current.as_deref()) != expected {
                return Err(CheckpointError::Conflict(format!(
                    "{} was modified outside this Agent checkpoint",
                    change.path
                )));
            }
            let desired = if forward {
                change.after.clone()
            } else {
                change.before.clone()
            };
            operations.push((path, current, desired));
        }
        let mut applied: Vec<(PathBuf, Option<String>)> = Vec::new();
        for (path, previous, desired) in operations {
            if let Err(error) = write_optional_text(&path, desired.as_deref()) {
                for (rollback_path, rollback_content) in applied.into_iter().rev() {
                    let _ = write_optional_text(&rollback_path, rollback_content.as_deref());
                }
                return Err(error);
            }
            applied.push((path, previous));
        }
        Ok(())
    }

    fn safe_path(&self, relative: &str) -> CheckpointResult<PathBuf> {
        let relative_path = Path::new(relative);
        if relative.trim().is_empty()
            || relative.len() > 4_096
            || relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
            || relative_path
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(value) => value.to_str(),
                    _ => None,
                })
                .any(blocked_workspace_name)
        {
            return Err(CheckpointError::Invalid(
                "path is outside the checkpoint workspace boundary".into(),
            ));
        }
        let candidate = self.workspace.join(relative_path);
        let parent = std::fs::canonicalize(
            candidate
                .parent()
                .ok_or_else(|| CheckpointError::Invalid("path has no parent".into()))?,
        )?;
        if !parent.starts_with(&self.workspace) {
            return Err(CheckpointError::Invalid(
                "path escaped the checkpoint workspace".into(),
            ));
        }
        let candidate = parent.join(
            candidate
                .file_name()
                .ok_or_else(|| CheckpointError::Invalid("path has no filename".into()))?,
        );
        if candidate.exists() {
            let metadata = std::fs::symlink_metadata(&candidate)?;
            let canonical = std::fs::canonicalize(&candidate)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || !canonical.starts_with(&self.workspace)
            {
                return Err(CheckpointError::Invalid(
                    "checkpoint target is not a regular workspace file".into(),
                ));
            }
            return Ok(canonical);
        }
        Ok(candidate)
    }

    fn load_state(&self, session_id: Uuid) -> CheckpointResult<CheckpointState> {
        let path = self.state_file(session_id);
        let backup = path.with_extension("json.bak");
        if !path.exists() && backup.exists() {
            std::fs::rename(&backup, &path)?;
        } else if path.exists() && backup.exists() {
            std::fs::remove_file(backup)?;
        }
        if !path.exists() {
            return Ok(CheckpointState::new(session_id));
        }
        let bytes = std::fs::read(path)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(CheckpointError::Invalid(
                "checkpoint state exceeds 64 MiB".into(),
            ));
        }
        let state: CheckpointState = serde_json::from_slice(&bytes)?;
        state.validate(session_id)?;
        Ok(state)
    }

    fn save_state(&self, state: &CheckpointState) -> CheckpointResult<()> {
        state.validate(state.session_id)?;
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(CheckpointError::Invalid(
                "checkpoint state exceeds 64 MiB".into(),
            ));
        }
        self.atomic_write(&self.state_file(state.session_id), &bytes)
    }

    fn atomic_write(&self, path: &Path, bytes: &[u8]) -> CheckpointResult<()> {
        let temporary = path.with_extension("json.tmp");
        let backup = path.with_extension("json.bak");
        if temporary.exists() {
            std::fs::remove_file(&temporary)?;
        }
        std::fs::write(&temporary, bytes)?;
        if path.exists() {
            if backup.exists() {
                std::fs::remove_file(&backup)?;
            }
            std::fs::rename(path, &backup)?;
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            if backup.exists() && !path.exists() {
                let _ = std::fs::rename(&backup, path);
            }
            return Err(error.into());
        }
        if backup.exists() {
            std::fs::remove_file(backup)?;
        }
        Ok(())
    }

    fn state_file(&self, session_id: Uuid) -> PathBuf {
        self.directory.join(format!("{session_id}.json"))
    }

    fn pending_file(&self, session_id: Uuid) -> PathBuf {
        self.directory.join(format!("{session_id}.pending.json"))
    }

    fn lock(&self) -> CheckpointResult<std::sync::MutexGuard<'_, ()>> {
        self.lock
            .lock()
            .map_err(|_| CheckpointError::Invalid("checkpoint lock is poisoned".into()))
    }
}

fn validate_checkpoint(checkpoint: &Checkpoint) -> CheckpointResult<()> {
    if checkpoint.changes.len() > MAX_CHANGES_PER_TURN {
        return Err(CheckpointError::Invalid(
            "checkpoint contains too many files".into(),
        ));
    }
    let mut total = 0_usize;
    for change in &checkpoint.changes {
        total = total
            .saturating_add(change.before.as_ref().map_or(0, String::len))
            .saturating_add(change.after.as_ref().map_or(0, String::len));
        if change
            .before
            .as_ref()
            .is_some_and(|value| value.len() > MAX_FILE_BYTES)
            || change
                .after
                .as_ref()
                .is_some_and(|value| value.len() > MAX_FILE_BYTES)
            || content_hash(change.before.as_deref()) != change.before_sha256
            || content_hash(change.after.as_deref()) != change.after_sha256
        {
            return Err(CheckpointError::Invalid(
                "checkpoint content or digest is invalid".into(),
            ));
        }
    }
    if total > MAX_TURN_BYTES {
        return Err(CheckpointError::Invalid(
            "checkpoint turn exceeds 16 MiB".into(),
        ));
    }
    Ok(())
}

fn finish_active(state: &mut CheckpointState) {
    if let Some(active) = state.active.take() {
        if !active.changes.is_empty() {
            state.undo.push(active);
            trim_history(&mut state.undo);
        }
    }
}

fn trim_history(history: &mut Vec<Checkpoint>) {
    if history.len() > MAX_HISTORY {
        history.drain(..history.len() - MAX_HISTORY);
    }
}

fn projected_turn_size(
    checkpoint: &Checkpoint,
    relative: &str,
    before: &Option<String>,
    after: &str,
) -> usize {
    checkpoint
        .changes
        .iter()
        .filter(|change| change.path != relative)
        .map(|change| {
            change.before.as_ref().map_or(0, String::len)
                + change.after.as_ref().map_or(0, String::len)
        })
        .sum::<usize>()
        + checkpoint
            .changes
            .iter()
            .find(|change| change.path == relative)
            .and_then(|change| change.before.as_ref())
            .or(before.as_ref())
            .map_or(0, String::len)
        + after.len()
}

fn read_optional_text(path: &Path) -> CheckpointResult<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_FILE_BYTES as u64
    {
        return Err(CheckpointError::Invalid(
            "checkpoint only supports regular UTF-8 files up to 256 KiB".into(),
        ));
    }
    Ok(Some(std::fs::read_to_string(path)?))
}

fn write_optional_text(path: &Path, content: Option<&str>) -> CheckpointResult<()> {
    match content {
        Some(content) => {
            std::fs::write(path, content)?;
        }
        None if path.exists() => {
            std::fs::remove_file(path)?;
        }
        None => {}
    }
    Ok(())
}

fn hash(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

fn content_hash(content: Option<&str>) -> Option<String> {
    content.map(|value| hash(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_checkpoint_undo_redo_and_conflict_detection() {
        let workspace = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("existing.txt"), "before").unwrap();
        let session = Uuid::new_v4();
        let store = CheckpointStore::new(workspace.path(), data.path().to_path_buf()).unwrap();
        store.begin_turn(session).unwrap();

        let prepared = store
            .prepare_write(session, "existing.txt", "after")
            .unwrap();
        std::fs::write(workspace.path().join("existing.txt"), "after").unwrap();
        store.commit_write(prepared).unwrap();
        let prepared = store.prepare_write(session, "created.txt", "new").unwrap();
        std::fs::write(workspace.path().join("created.txt"), "new").unwrap();
        store.commit_write(prepared).unwrap();
        store.finish_turn(session).unwrap();

        let reopened = CheckpointStore::new(workspace.path(), data.path().to_path_buf()).unwrap();
        assert_eq!(reopened.undo(session).unwrap().unwrap().files, 2);
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("existing.txt")).unwrap(),
            "before"
        );
        assert!(!workspace.path().join("created.txt").exists());
        reopened.redo(session).unwrap();
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("existing.txt")).unwrap(),
            "after"
        );
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("created.txt")).unwrap(),
            "new"
        );

        reopened.undo(session).unwrap();
        std::fs::write(workspace.path().join("existing.txt"), "manual").unwrap();
        assert!(matches!(
            reopened.redo(session),
            Err(CheckpointError::Conflict(_))
        ));
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("existing.txt")).unwrap(),
            "manual"
        );
    }
}
