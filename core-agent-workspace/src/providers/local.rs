use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use url::Url;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::domain::{
    validate_actor, Environment, GraphEdge, GraphNode, GraphNodeKind, GraphRelation, Project,
    ProjectKind, Resource, ResourceCapability, ResourceType, Snapshot, Workspace, WorkspaceGraph,
    WorkspaceOpenRequest, WorkspaceSearchHit,
};
use crate::error::{WorkspaceError, WorkspaceResult};
use crate::infrastructure::{
    EnvironmentDetector, ProjectScanner, ResourceProvider, WorkspaceIndexer, WorkspaceProvider,
    WorkspaceSnapshot,
};

const DEFAULT_IGNORED: [&str; 4] = [".git", ".core-agent", "node_modules", "target"];

pub(crate) fn path_from_file_uri(uri: &str) -> WorkspaceResult<PathBuf> {
    let url = Url::parse(uri)
        .map_err(|error| WorkspaceError::UnsupportedUri(format!("{uri}: {error}")))?;
    if url.scheme() != "file" || !url.username().is_empty() || url.password().is_some() {
        return Err(WorkspaceError::UnsupportedUri(uri.into()));
    }
    url.to_file_path()
        .map_err(|_| WorkspaceError::UnsupportedUri(uri.into()))
}

fn directory_uri(path: &Path) -> WorkspaceResult<String> {
    Url::from_directory_path(path)
        .map(|url| url.to_string())
        .map_err(|_| {
            WorkspaceError::UnsupportedUri(format!(
                "cannot represent directory `{}` as file URI",
                path.display()
            ))
        })
}

fn file_uri(path: &Path) -> WorkspaceResult<String> {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| {
            WorkspaceError::UnsupportedUri(format!(
                "cannot represent file `{}` as file URI",
                path.display()
            ))
        })
}

fn map_walk_error(error: walkdir::Error) -> WorkspaceError {
    WorkspaceError::Io(
        error.into_io_error().unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid directory entry")
        }),
    )
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_depth: usize,
    pub max_resources: usize,
    pub ignored_names: BTreeSet<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_resources: 50_000,
            ignored_names: DEFAULT_IGNORED.into_iter().map(String::from).collect(),
        }
    }
}

impl ScanOptions {
    fn includes(&self, entry: &DirEntry) -> bool {
        entry.depth() == 0
            || !self
                .ignored_names
                .contains(entry.file_name().to_string_lossy().as_ref())
    }
}

#[derive(Default)]
pub struct LocalWorkspaceProvider;

#[async_trait]
impl WorkspaceProvider for LocalWorkspaceProvider {
    fn key(&self) -> &str {
        "local"
    }

    fn supports(&self, uri: &str) -> bool {
        Url::parse(uri).is_ok_and(|url| url.scheme() == "file")
    }

    async fn load(&self, request: &WorkspaceOpenRequest) -> WorkspaceResult<Workspace> {
        let root = path_from_file_uri(&request.uri)?;
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(WorkspaceError::Validation(format!(
                "local workspace `{}` is not a directory",
                root.display()
            )));
        }
        Workspace::new(
            &request.name,
            self.key(),
            directory_uri(&root)?,
            request.metadata.clone(),
        )
    }
}

pub struct LocalResourceProvider {
    options: ScanOptions,
}

impl LocalResourceProvider {
    pub fn new(options: ScanOptions) -> Self {
        Self { options }
    }

    fn classify(path: &Path, is_directory: bool) -> ResourceType {
        if is_directory {
            return ResourceType::Directory;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "md" | "markdown" | "mdx" => ResourceType::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => ResourceType::Image,
            "pdf" => ResourceType::Pdf,
            "db" | "sqlite" | "sqlite3" => ResourceType::Database,
            "bin" | "exe" | "dll" | "so" | "dylib" | "class" | "jar" | "zip" => {
                ResourceType::Binary
            }
            _ => ResourceType::File,
        }
    }

    fn capabilities(path: &Path, resource_type: ResourceType) -> BTreeSet<ResourceCapability> {
        let mut capabilities = BTreeSet::from([
            ResourceCapability::Read,
            ResourceCapability::Write,
            ResourceCapability::Delete,
            ResourceCapability::Watch,
        ]);
        if matches!(
            resource_type,
            ResourceType::Directory | ResourceType::File | ResourceType::Markdown
        ) {
            capabilities.insert(ResourceCapability::Search);
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ["exe", "bat", "cmd", "ps1", "sh"].contains(&extension.as_str()) {
            capabilities.insert(ResourceCapability::Execute);
        }
        capabilities
    }

    fn has_included_child(&self, directory: &Path) -> WorkspaceResult<bool> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            if self
                .options
                .ignored_names
                .contains(name.to_string_lossy().as_ref())
            {
                continue;
            }
            if !entry.file_type()?.is_symlink() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Default for LocalResourceProvider {
    fn default() -> Self {
        Self::new(ScanOptions::default())
    }
}

#[async_trait]
impl ResourceProvider for LocalResourceProvider {
    fn key(&self) -> &str {
        "local-file"
    }

    fn supports(&self, workspace: &Workspace) -> bool {
        workspace.provider_key == "local" && workspace.uri.starts_with("file:")
    }

    async fn scan(&self, workspace: &Workspace) -> WorkspaceResult<Vec<Resource>> {
        let root = path_from_file_uri(&workspace.uri)?.canonicalize()?;
        let mut resources = Vec::new();
        let walker = WalkDir::new(&root)
            .max_depth(self.options.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| self.options.includes(entry));
        for entry in walker {
            let entry = entry.map_err(map_walk_error)?;
            if entry.file_type().is_dir()
                && entry.depth() == self.options.max_depth
                && self.has_included_child(entry.path())?
            {
                return Err(WorkspaceError::LimitExceeded(format!(
                    "resource scan reached maximum depth {}",
                    self.options.max_depth
                )));
            }
            if entry.depth() == 0 || entry.file_type().is_symlink() {
                continue;
            }
            if resources.len() >= self.options.max_resources {
                return Err(WorkspaceError::LimitExceeded(format!(
                    "resource scan exceeded {} entries",
                    self.options.max_resources
                )));
            }
            let name = entry.file_name().to_str().ok_or_else(|| {
                WorkspaceError::UnsupportedUri(format!(
                    "non-UTF-8 resource name under `{}`",
                    root.display()
                ))
            })?;
            let is_directory = entry.file_type().is_dir();
            let resource_type = Self::classify(entry.path(), is_directory);
            let metadata = entry.metadata().map_err(map_walk_error)?;
            let uri = if is_directory {
                directory_uri(entry.path())?
            } else {
                file_uri(entry.path())?
            };
            resources.push(Resource::new(
                workspace.id,
                resource_type,
                uri,
                name,
                (!is_directory).then_some(metadata.len()),
                Self::capabilities(entry.path(), resource_type),
                self.key(),
            ));
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        Ok(resources)
    }
}

#[derive(Default)]
pub struct LocalProjectScanner;

fn project_marker(name: &str) -> bool {
    matches!(
        name,
        "Cargo.toml"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "package.json"
            | "pyproject.toml"
            | "requirements.txt"
    )
}

fn project_kind(markers: &BTreeSet<String>) -> ProjectKind {
    if markers.contains("Cargo.toml") {
        ProjectKind::Rust
    } else if markers.contains("pom.xml") {
        ProjectKind::Maven
    } else if markers.iter().any(|marker| marker.contains("gradle")) {
        ProjectKind::Gradle
    } else if markers.contains("package.json") {
        ProjectKind::Node
    } else if markers.contains("pyproject.toml") || markers.contains("requirements.txt") {
        ProjectKind::Python
    } else {
        ProjectKind::Generic
    }
}

#[async_trait]
impl ProjectScanner for LocalProjectScanner {
    fn key(&self) -> &str {
        "local-markers"
    }

    async fn scan(
        &self,
        workspace: &Workspace,
        resources: &[Resource],
    ) -> WorkspaceResult<Vec<Project>> {
        let mut roots: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for resource in resources
            .iter()
            .filter(|resource| project_marker(&resource.name))
        {
            let path = path_from_file_uri(&resource.uri)?;
            let parent = path.parent().ok_or_else(|| {
                WorkspaceError::Validation(format!(
                    "project marker has no parent: {}",
                    resource.uri
                ))
            })?;
            roots
                .entry(directory_uri(parent)?)
                .or_default()
                .insert(resource.name.clone());
        }
        if roots.is_empty() {
            roots.insert(workspace.uri.clone(), BTreeSet::new());
        }

        let root_uris = roots.keys().cloned().collect::<Vec<_>>();
        let mut projects = roots
            .into_iter()
            .map(|(root_uri, markers)| {
                let path = path_from_file_uri(&root_uri)?;
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&workspace.name)
                    .to_string();
                let mut project = Project::new(
                    workspace.id,
                    name,
                    project_kind(&markers),
                    &root_uri,
                    markers.into_iter().collect(),
                );
                project.module_count = root_uris
                    .iter()
                    .filter(|candidate| candidate.starts_with(&root_uri))
                    .count()
                    .max(1) as u32;
                Ok(project)
            })
            .collect::<WorkspaceResult<Vec<_>>>()?;
        projects.sort_by(|left, right| left.root_uri.cmp(&right.root_uri));
        Ok(projects)
    }
}

#[derive(Default)]
pub struct LocalEnvironmentDetector;

#[async_trait]
impl EnvironmentDetector for LocalEnvironmentDetector {
    fn key(&self) -> &str {
        "local-environment"
    }

    async fn detect(
        &self,
        workspace: &Workspace,
        projects: &[Project],
        resources: &[Resource],
    ) -> WorkspaceResult<Environment> {
        let mut environment = Environment::new(workspace.id, std::env::consts::OS);
        environment.shell = std::env::var("SHELL")
            .or_else(|_| std::env::var("COMSPEC"))
            .ok()
            .and_then(|value| {
                Path::new(&value)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            });
        let root = path_from_file_uri(&workspace.uri)?;
        if root.join(".git").exists() {
            environment.git = Some("repository".into());
        }
        for project in projects {
            match project.kind {
                ProjectKind::Rust => {
                    environment.languages.insert("rust".into());
                    environment.runtimes.insert("rust".into());
                    environment.package_managers.insert("cargo".into());
                }
                ProjectKind::Maven => {
                    environment.languages.insert("java".into());
                    environment.runtimes.insert("jvm".into());
                    environment.package_managers.insert("maven".into());
                }
                ProjectKind::Gradle => {
                    environment.languages.insert("java".into());
                    environment.runtimes.insert("jvm".into());
                    environment.package_managers.insert("gradle".into());
                }
                ProjectKind::Node => {
                    environment.languages.insert("javascript".into());
                    environment.runtimes.insert("node".into());
                    environment.package_managers.insert("npm".into());
                }
                ProjectKind::Python => {
                    environment.languages.insert("python".into());
                    environment.runtimes.insert("python".into());
                    environment.package_managers.insert("pip".into());
                }
                ProjectKind::Generic => {}
            }
        }
        for resource in resources {
            let extension = path_from_file_uri(&resource.uri)?
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let language = match extension.as_str() {
                "rs" => Some("rust"),
                "java" | "kt" => Some("java"),
                "js" | "jsx" | "mjs" => Some("javascript"),
                "ts" | "tsx" => Some("typescript"),
                "py" => Some("python"),
                "go" => Some("go"),
                "cs" => Some("csharp"),
                _ => None,
            };
            if let Some(language) = language {
                environment.languages.insert(language.into());
            }
        }
        for key in ["PATH", "HOME", "USERPROFILE", "CI"] {
            if std::env::var_os(key).is_some() {
                environment.variable_names.insert(key.into());
            }
        }
        environment.validate()?;
        Ok(environment)
    }
}

#[derive(Default)]
pub struct LocalWorkspaceIndexer;

#[async_trait]
impl WorkspaceIndexer for LocalWorkspaceIndexer {
    async fn build(&self, workspace: &Workspace) -> WorkspaceResult<WorkspaceGraph> {
        let workspace_node = format!("workspace:{}", workspace.id);
        let mut graph = WorkspaceGraph {
            nodes: vec![GraphNode::new(
                &workspace_node,
                GraphNodeKind::Workspace,
                &workspace.name,
                Some(workspace.uri.clone()),
            )],
            edges: Vec::new(),
        };
        for project in &workspace.projects {
            let id = format!("project:{}", project.id);
            let mut node = GraphNode::new(
                &id,
                GraphNodeKind::Project,
                &project.name,
                Some(project.root_uri.clone()),
            );
            node.metadata.insert(
                "project_kind".into(),
                Value::String(project.kind.as_str().into()),
            );
            graph.nodes.push(node);
            graph.edges.push(GraphEdge {
                source: workspace_node.clone(),
                target: id,
                relation: GraphRelation::Contains,
            });
        }
        if let Some(environment) = &workspace.environment {
            let id = format!("environment:{}", environment.id);
            graph.nodes.push(GraphNode::new(
                &id,
                GraphNodeKind::Environment,
                &environment.os,
                None,
            ));
            graph.edges.push(GraphEdge {
                source: workspace_node.clone(),
                target: id,
                relation: GraphRelation::DetectedIn,
            });
        }
        for resource in &workspace.resources {
            let id = format!("resource:{}", resource.id);
            graph.nodes.push(GraphNode::new(
                &id,
                GraphNodeKind::Resource,
                &resource.name,
                Some(resource.uri.clone()),
            ));
            let source = resource
                .project_id
                .map(|project_id| format!("project:{project_id}"))
                .unwrap_or_else(|| workspace_node.clone());
            graph.edges.push(GraphEdge {
                source,
                target: id,
                relation: GraphRelation::Contains,
            });
        }
        graph.nodes.sort_by(|left, right| left.id.cmp(&right.id));
        graph.edges.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then(left.target.cmp(&right.target))
        });
        graph.validate()?;
        Ok(graph)
    }

    async fn search(
        &self,
        graph: &WorkspaceGraph,
        query: &str,
        limit: usize,
    ) -> WorkspaceResult<Vec<WorkspaceSearchHit>> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut hits = graph
            .nodes
            .iter()
            .filter_map(|node| {
                let label = node.label.to_ascii_lowercase();
                let uri = node.uri.as_deref().unwrap_or_default().to_ascii_lowercase();
                let score = if label == query {
                    100
                } else if label.starts_with(&query) {
                    80
                } else if label.contains(&query) {
                    60
                } else if uri.contains(&query) {
                    40
                } else {
                    return None;
                };
                Some(WorkspaceSearchHit {
                    node_id: node.id.clone(),
                    kind: node.kind,
                    label: node.label.clone(),
                    uri: node.uri.clone(),
                    score,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(left.label.cmp(&right.label))
                .then(left.node_id.cmp(&right.node_id))
        });
        hits.truncate(limit);
        Ok(hits)
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    pub max_files: u64,
    pub max_bytes: u64,
    pub ignored_names: BTreeSet<String>,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            max_files: 50_000,
            max_bytes: 512 * 1024 * 1024,
            ignored_names: DEFAULT_IGNORED.into_iter().map(String::from).collect(),
        }
    }
}

pub struct LocalWorkspaceSnapshot {
    root: PathBuf,
    options: SnapshotOptions,
}

impl LocalWorkspaceSnapshot {
    pub fn new(root: impl Into<PathBuf>, options: SnapshotOptions) -> Self {
        Self {
            root: root.into(),
            options,
        }
    }

    fn includes(&self, entry: &DirEntry) -> bool {
        entry.depth() == 0
            || !self
                .options
                .ignored_names
                .contains(entry.file_name().to_string_lossy().as_ref())
    }

    fn checked_target(destination: &Path, relative: &Path) -> WorkspaceResult<PathBuf> {
        let mut target = destination.to_path_buf();
        for component in relative.components() {
            match component {
                std::path::Component::Normal(component) => target.push(component),
                std::path::Component::CurDir => continue,
                _ => {
                    return Err(WorkspaceError::Validation(
                        "snapshot entry contains a non-relative path component".into(),
                    ));
                }
            }
            match fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(WorkspaceError::Validation(format!(
                        "snapshot target contains symbolic link `{}`",
                        target.display()
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(target)
    }

    fn copy_overlay(&self, source: &Path, destination: &Path) -> WorkspaceResult<(u64, u64)> {
        let mut file_count = 0_u64;
        let mut total_bytes = 0_u64;
        let walker = WalkDir::new(source)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| self.includes(entry));
        for entry in walker {
            let entry = entry.map_err(map_walk_error)?;
            if entry.file_type().is_symlink() {
                continue;
            }
            let relative = entry.path().strip_prefix(source).map_err(|error| {
                WorkspaceError::Internal(format!("snapshot relative path failed: {error}"))
            })?;
            let target = Self::checked_target(destination, relative)?;
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
                continue;
            }
            let size = entry.metadata().map_err(map_walk_error)?.len();
            file_count = file_count.saturating_add(1);
            total_bytes = total_bytes.saturating_add(size);
            if file_count > self.options.max_files || total_bytes > self.options.max_bytes {
                return Err(WorkspaceError::LimitExceeded(format!(
                    "snapshot exceeds {} files or {} bytes",
                    self.options.max_files, self.options.max_bytes
                )));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
        }
        Ok((file_count, total_bytes))
    }
}

impl Default for LocalWorkspaceSnapshot {
    fn default() -> Self {
        Self::new(
            std::env::temp_dir().join("core-agent-workspace-snapshots"),
            SnapshotOptions::default(),
        )
    }
}

#[async_trait]
impl WorkspaceSnapshot for LocalWorkspaceSnapshot {
    async fn create(
        &self,
        workspace: &Workspace,
        label: &str,
        actor: &str,
    ) -> WorkspaceResult<Snapshot> {
        workspace.validate()?;
        validate_actor(actor)?;
        if label.trim().is_empty() || label.len() > 256 {
            return Err(WorkspaceError::Validation(
                "snapshot label must contain 1..=256 characters".into(),
            ));
        }
        let source = path_from_file_uri(&workspace.uri)?.canonicalize()?;
        fs::create_dir_all(&self.root)?;
        let snapshot_root = self.root.canonicalize()?;
        if snapshot_root.starts_with(&source) {
            return Err(WorkspaceError::Validation(
                "snapshot storage cannot be inside the workspace".into(),
            ));
        }
        let id = Uuid::new_v4();
        let destination = snapshot_root
            .join(workspace.id.to_string())
            .join(id.to_string());
        fs::create_dir_all(&destination)?;
        let copied = self.copy_overlay(&source, &destination);
        let (resource_count, total_bytes) = match copied {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_dir_all(&destination);
                return Err(error);
            }
        };
        let mut snapshot = Snapshot {
            id,
            workspace_id: workspace.id,
            label: label.into(),
            storage_uri: directory_uri(&destination)?,
            resource_count,
            total_bytes,
            metadata: BTreeMap::new(),
            created_at: Utc::now(),
        };
        snapshot
            .metadata
            .insert("restore_mode".into(), Value::String("overlay".into()));
        snapshot.validate()?;
        Ok(snapshot)
    }

    async fn restore(
        &self,
        workspace: &Workspace,
        snapshot: &Snapshot,
        actor: &str,
    ) -> WorkspaceResult<()> {
        workspace.validate()?;
        snapshot.validate()?;
        validate_actor(actor)?;
        if snapshot.workspace_id != workspace.id {
            return Err(WorkspaceError::Validation(
                "snapshot belongs to a different workspace".into(),
            ));
        }
        let snapshot_root = self.root.canonicalize()?;
        let source = path_from_file_uri(&snapshot.storage_uri)?.canonicalize()?;
        if !source.starts_with(&snapshot_root) {
            return Err(WorkspaceError::Validation(
                "snapshot storage is outside the configured snapshot root".into(),
            ));
        }
        let destination = path_from_file_uri(&workspace.uri)?.canonicalize()?;
        self.copy_overlay(&source, &destination)?;
        Ok(())
    }

    async fn discard(&self, snapshot: &Snapshot) -> WorkspaceResult<()> {
        snapshot.validate()?;
        let snapshot_root = self.root.canonicalize()?;
        let path = path_from_file_uri(&snapshot.storage_uri)?.canonicalize()?;
        if path == snapshot_root || !path.starts_with(&snapshot_root) {
            return Err(WorkspaceError::Validation(
                "snapshot storage is outside the configured snapshot root".into(),
            ));
        }
        let parent = path.parent().map(Path::to_path_buf);
        fs::remove_dir_all(&path)?;
        if let Some(parent) = parent {
            if parent != snapshot_root && fs::read_dir(&parent)?.next().is_none() {
                fs::remove_dir(&parent)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::infrastructure::{ProjectScanner, ResourceProvider, WorkspaceProvider};

    #[tokio::test]
    async fn local_scan_ignores_build_directories_and_detects_projects() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname='demo'",
        )
        .unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        fs::write(directory.path().join("src/lib.rs"), "pub fn demo() {}").unwrap();
        fs::create_dir(directory.path().join("target")).unwrap();
        fs::write(directory.path().join("target/ignored.bin"), "ignored").unwrap();

        let request = WorkspaceOpenRequest::local("demo", directory.path()).unwrap();
        let workspace = LocalWorkspaceProvider.load(&request).await.unwrap();
        let resources = LocalResourceProvider::default()
            .scan(&workspace)
            .await
            .unwrap();
        assert!(resources.iter().any(|resource| resource.name == "lib.rs"));
        assert!(!resources
            .iter()
            .any(|resource| resource.name == "ignored.bin"));
        let projects = LocalProjectScanner
            .scan(&workspace, &resources)
            .await
            .unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::Rust);
    }

    #[tokio::test]
    async fn snapshot_restore_is_non_destructive_overlay() {
        let directory = tempdir().unwrap();
        let snapshots = tempdir().unwrap();
        fs::write(directory.path().join("tracked.txt"), "before").unwrap();
        let request = WorkspaceOpenRequest::local("demo", directory.path()).unwrap();
        let workspace = LocalWorkspaceProvider.load(&request).await.unwrap();
        let snapshotter = LocalWorkspaceSnapshot::new(snapshots.path(), SnapshotOptions::default());
        let snapshot = snapshotter
            .create(&workspace, "before edit", "tester")
            .await
            .unwrap();
        fs::write(directory.path().join("tracked.txt"), "after").unwrap();
        fs::write(directory.path().join("new.txt"), "keep").unwrap();
        snapshotter
            .restore(&workspace, &snapshot, "tester")
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("tracked.txt")).unwrap(),
            "before"
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("new.txt")).unwrap(),
            "keep"
        );
        let snapshot_path = path_from_file_uri(&snapshot.storage_uri).unwrap();
        snapshotter.discard(&snapshot).await.unwrap();
        assert!(!snapshot_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn snapshot_restore_refuses_symbolic_link_targets() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let snapshots = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(directory.path().join("tracked.txt"), "before").unwrap();
        fs::write(outside.path().join("outside.txt"), "outside").unwrap();
        let request = WorkspaceOpenRequest::local("demo", directory.path()).unwrap();
        let workspace = LocalWorkspaceProvider.load(&request).await.unwrap();
        let snapshotter = LocalWorkspaceSnapshot::new(snapshots.path(), SnapshotOptions::default());
        let snapshot = snapshotter
            .create(&workspace, "before edit", "tester")
            .await
            .unwrap();
        fs::remove_file(directory.path().join("tracked.txt")).unwrap();
        symlink(
            outside.path().join("outside.txt"),
            directory.path().join("tracked.txt"),
        )
        .unwrap();
        assert!(snapshotter
            .restore(&workspace, &snapshot, "tester")
            .await
            .is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("outside.txt")).unwrap(),
            "outside"
        );
    }
}
