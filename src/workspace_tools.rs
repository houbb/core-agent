use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::checkpoint::CheckpointStore;
use crate::enterprise::{blocked_workspace_name, resolve_workspace_resource};
use crate::{
    FunctionTool, PermissionDecision, RawToolOutput, ToolContent, ToolDefinition, ToolError,
    ToolRegistration,
};

const MAX_SCANNED_ENTRIES: usize = 50_000;
const MAX_FIND_RESULTS: usize = 2_000;
const MAX_SEARCH_RESULTS: usize = 1_000;
const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PATCH_FILE_BYTES: usize = 256 * 1024;

pub(crate) fn registrations(
    workspace: &Path,
    checkpoints: Arc<CheckpointStore>,
) -> Result<Vec<ToolRegistration>, ToolError> {
    let root = std::fs::canonicalize(workspace)
        .map_err(|error| ToolError::execution("workspace_tools", error.to_string(), false))?;

    let mut find_definition = ToolDefinition::new(
        "workspace",
        "find_files",
        "1.0.0",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative directory, default ."},
                "pattern": {"type": "string", "description": "Glob pattern such as **/*.rs"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 2000}
            },
            "additionalProperties": false
        }),
    );
    find_definition.description =
        "Find workspace files by glob without reading file contents. Sensitive paths and symlinks are excluded.".into();
    find_definition.category = "filesystem.read".into();
    find_definition.default_permission = PermissionDecision::Allow;
    let find_key = find_definition.key.clone();
    let find_root = root.clone();
    let find_tool = Arc::new(FunctionTool::new(find_key, move |request, context| {
        let root = find_root.clone();
        async move { find_files(&root, &request.parameters, || context.is_cancelled()) }
    }));

    let mut search_definition = ToolDefinition::new(
        "workspace",
        "search_files",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "Rust-compatible regular expression"},
                "path": {"type": "string", "description": "Relative directory or file, default ."},
                "file_pattern": {"type": "string", "description": "Optional glob filter such as **/*.rs"},
                "case_insensitive": {"type": "boolean"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
            },
            "additionalProperties": false
        }),
    );
    search_definition.description = "Search bounded UTF-8 workspace files with a regular expression and return path, line, column and matching line. Sensitive paths, binary files and symlinks are excluded.".into();
    search_definition.category = "filesystem.read".into();
    search_definition.default_permission = PermissionDecision::Allow;
    let search_key = search_definition.key.clone();
    let search_root = root.clone();
    let search_tool = Arc::new(FunctionTool::new(search_key, move |request, context| {
        let root = search_root.clone();
        async move { search_files(&root, &request.parameters, || context.is_cancelled()) }
    }));

    let mut patch_definition = ToolDefinition::new(
        "workspace",
        "apply_patch",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["path", "operation", "new_text"],
            "properties": {
                "path": {"type": "string", "description": "Relative UTF-8 text file path"},
                "operation": {"type": "string", "enum": ["create", "update"]},
                "expected_sha256": {"type": "string", "description": "Required current SHA-256 for update"},
                "old_text": {"type": "string", "description": "Exact text to replace for update"},
                "new_text": {"type": "string", "description": "Full content for create, replacement text for update"},
                "replace_all": {"type": "boolean", "description": "Replace every exact occurrence; default false"}
            },
            "additionalProperties": false
        }),
    );
    patch_definition.description = "Create a file or apply an exact, incremental text replacement. Updates require the current SHA-256 and reject ambiguous matches; every change is checkpointed.".into();
    patch_definition.category = "filesystem.write".into();
    patch_definition.default_permission = PermissionDecision::Ask;
    let patch_key = patch_definition.key.clone();
    let patch_root = root;
    let patch_tool = Arc::new(FunctionTool::new(patch_key, move |request, context| {
        let root = patch_root.clone();
        let checkpoints = checkpoints.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let session_id = request.session_id.ok_or_else(|| {
                ToolError::InvalidArgument("apply_patch requires an active session".into())
            })?;
            let patch = plan_patch(&root, &request.parameters)?;
            let prepared = checkpoints
                .prepare_write(session_id, &patch.relative_path, &patch.content)
                .map_err(|error| ToolError::execution("apply_patch", error.to_string(), false))?;
            if let Err(error) = std::fs::write(&patch.absolute_path, patch.content.as_bytes()) {
                checkpoints
                    .abort_write(prepared)
                    .map_err(|checkpoint_error| {
                        ToolError::execution("apply_patch", checkpoint_error.to_string(), false)
                    })?;
                return Err(ToolError::execution(
                    "apply_patch",
                    error.to_string(),
                    false,
                ));
            }
            checkpoints
                .commit_write(prepared)
                .map_err(|error| ToolError::execution("apply_patch", error.to_string(), false))?;
            let mut output = RawToolOutput::default();
            output.content.push(ToolContent::Json(json!({
                "path": patch.relative_path,
                "operation": patch.operation,
                "replacements": patch.replacements,
                "sha256": sha256(patch.content.as_bytes()),
                "bytes": patch.content.len()
            })));
            Ok(output)
        }
    }));

    Ok(vec![
        ToolRegistration::new(find_definition, find_tool),
        ToolRegistration::new(search_definition, search_tool),
        ToolRegistration::new(patch_definition, patch_tool),
    ])
}

#[derive(Debug, Deserialize)]
struct FindInput {
    #[serde(default = "default_dot")]
    path: String,
    #[serde(default = "default_glob")]
    pattern: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchInput {
    query: String,
    #[serde(default = "default_dot")]
    path: String,
    file_pattern: Option<String>,
    #[serde(default)]
    case_insensitive: bool,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct PatchInput {
    path: String,
    operation: String,
    expected_sha256: Option<String>,
    old_text: Option<String>,
    new_text: String,
    #[serde(default)]
    replace_all: bool,
}

struct PlannedPatch {
    absolute_path: PathBuf,
    relative_path: String,
    operation: &'static str,
    content: String,
    replacements: usize,
}

fn default_dot() -> String {
    ".".into()
}

fn default_glob() -> String {
    "**/*".into()
}

fn find_files<F>(root: &Path, parameters: &Value, cancelled: F) -> Result<RawToolOutput, ToolError>
where
    F: Fn() -> bool,
{
    let input: FindInput = parse_input("find_files", parameters)?;
    let directory = readable_path(root, &input.path)?;
    if !directory.is_dir() {
        return Err(ToolError::InvalidArgument(
            "find_files path must be a directory".into(),
        ));
    }
    let matcher = glob_regex(&input.pattern)?;
    let limit = input
        .limit
        .unwrap_or(MAX_FIND_RESULTS)
        .clamp(1, MAX_FIND_RESULTS);
    let mut files = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;
    walk_files(root, &directory, &cancelled, |_path, relative| {
        scanned += 1;
        if matcher.is_match(relative) && files.len() < limit {
            files.push(relative.to_owned());
        } else if matcher.is_match(relative) {
            truncated = true;
        }
        !truncated && scanned < MAX_SCANNED_ENTRIES
    })?;
    if scanned >= MAX_SCANNED_ENTRIES {
        truncated = true;
    }
    files.sort();
    json_output(json!({
        "matches": files,
        "count": files.len(),
        "scanned": scanned,
        "truncated": truncated
    }))
}

fn search_files<F>(
    root: &Path,
    parameters: &Value,
    cancelled: F,
) -> Result<RawToolOutput, ToolError>
where
    F: Fn() -> bool,
{
    let input: SearchInput = parse_input("search_files", parameters)?;
    if input.query.is_empty() || input.query.len() > 2_048 {
        return Err(ToolError::InvalidArgument(
            "search_files query must contain 1..=2048 bytes".into(),
        ));
    }
    let query = RegexBuilder::new(&input.query)
        .case_insensitive(input.case_insensitive)
        .size_limit(2 * 1024 * 1024)
        .build()
        .map_err(|error| ToolError::InvalidArgument(format!("invalid query regex: {error}")))?;
    let file_matcher = input.file_pattern.as_deref().map(glob_regex).transpose()?;
    let start = readable_path(root, &input.path)?;
    let limit = input
        .limit
        .unwrap_or(MAX_SEARCH_RESULTS)
        .clamp(1, MAX_SEARCH_RESULTS);
    let mut matches = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut truncated = false;
    let mut search_one = |path: &Path, relative: &str| -> Result<bool, ToolError> {
        if file_matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.is_match(relative))
        {
            return Ok(true);
        }
        scanned_files += 1;
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| ToolError::execution("search_files", error.to_string(), false))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_SEARCH_FILE_BYTES
        {
            skipped_files += 1;
            return Ok(true);
        }
        let bytes = std::fs::read(path)
            .map_err(|error| ToolError::execution("search_files", error.to_string(), false))?;
        let post_read = std::fs::symlink_metadata(path)
            .map_err(|error| ToolError::execution("search_files", error.to_string(), false))?;
        if post_read.file_type().is_symlink()
            || !post_read.is_file()
            || post_read.len() != metadata.len()
        {
            return Err(ToolError::PermissionDenied(
                "search target changed while it was being read".into(),
            ));
        }
        if bytes.contains(&0) {
            skipped_files += 1;
            return Ok(true);
        }
        let Ok(content) = std::str::from_utf8(&bytes) else {
            skipped_files += 1;
            return Ok(true);
        };
        for (line_index, line) in content.lines().enumerate() {
            for found in query.find_iter(line) {
                if matches.len() >= limit {
                    truncated = true;
                    return Ok(false);
                }
                matches.push(json!({
                    "path": relative,
                    "line": line_index + 1,
                    "column": line[..found.start()].chars().count() + 1,
                    "text": truncate_chars(line, 2_000)
                }));
            }
        }
        Ok(true)
    };

    if start.is_file() {
        let relative = relative_string(root, &start)?;
        search_one(&start, &relative)?;
    } else if start.is_dir() {
        let mut traversal_error = None;
        walk_files(
            root,
            &start,
            &cancelled,
            |path, relative| match search_one(path, relative) {
                Ok(keep_going) => keep_going,
                Err(error) => {
                    traversal_error = Some(error);
                    false
                }
            },
        )?;
        if let Some(error) = traversal_error {
            return Err(error);
        }
    } else {
        return Err(ToolError::InvalidArgument(
            "search_files path must be a file or directory".into(),
        ));
    }
    if scanned_files >= MAX_SCANNED_ENTRIES {
        truncated = true;
    }
    json_output(json!({
        "matches": matches,
        "count": matches.len(),
        "scannedFiles": scanned_files,
        "skippedFiles": skipped_files,
        "truncated": truncated
    }))
}

fn plan_patch(root: &Path, parameters: &Value) -> Result<PlannedPatch, ToolError> {
    let input: PatchInput = parse_input("apply_patch", parameters)?;
    let (absolute_path, relative_path) = writable_path(root, &input.path)?;
    match input.operation.as_str() {
        "create" => {
            if absolute_path.exists() {
                return Err(ToolError::InvalidArgument(format!(
                    "{} already exists; use update with expected_sha256",
                    input.path
                )));
            }
            validate_patch_size(&input.new_text)?;
            Ok(PlannedPatch {
                absolute_path,
                relative_path,
                operation: "create",
                content: input.new_text,
                replacements: 0,
            })
        }
        "update" => {
            let expected = input.expected_sha256.ok_or_else(|| {
                ToolError::InvalidArgument("update requires expected_sha256".into())
            })?;
            if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(ToolError::InvalidArgument(
                    "expected_sha256 must be a 64-character hexadecimal digest".into(),
                ));
            }
            let old_text = input
                .old_text
                .ok_or_else(|| ToolError::InvalidArgument("update requires old_text".into()))?;
            if old_text.is_empty() {
                return Err(ToolError::InvalidArgument(
                    "old_text must not be empty".into(),
                ));
            }
            let current = std::fs::read_to_string(&absolute_path)
                .map_err(|error| ToolError::execution("apply_patch", error.to_string(), false))?;
            let actual = sha256(current.as_bytes());
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(ToolError::PermissionDenied(format!(
                    "{} changed since it was read (expected {expected}, actual {actual})",
                    input.path
                )));
            }
            let occurrences = current.matches(&old_text).count();
            if occurrences == 0 {
                return Err(ToolError::InvalidArgument(
                    "old_text was not found in the current file".into(),
                ));
            }
            if occurrences > 1 && !input.replace_all {
                return Err(ToolError::InvalidArgument(format!(
                    "old_text is ambiguous ({occurrences} occurrences); add surrounding context or set replace_all"
                )));
            }
            let replacements = if input.replace_all { occurrences } else { 1 };
            let content = if input.replace_all {
                current.replace(&old_text, &input.new_text)
            } else {
                current.replacen(&old_text, &input.new_text, 1)
            };
            validate_patch_size(&content)?;
            Ok(PlannedPatch {
                absolute_path,
                relative_path,
                operation: "update",
                content,
                replacements,
            })
        }
        _ => Err(ToolError::InvalidArgument(
            "operation must be create or update".into(),
        )),
    }
}

fn walk_files<F, V>(
    root: &Path,
    start: &Path,
    cancelled: &F,
    mut visitor: V,
) -> Result<(), ToolError>
where
    F: Fn() -> bool,
    V: FnMut(&Path, &str) -> bool,
{
    let mut pending = vec![start.to_path_buf()];
    let mut visited = 0;
    while let Some(directory) = pending.pop() {
        if cancelled() {
            return Err(ToolError::Cancelled("workspace traversal".into()));
        }
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|error| ToolError::execution("workspace_traversal", error.to_string(), false))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                ToolError::execution("workspace_traversal", error.to_string(), false)
            })?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            visited += 1;
            if visited > MAX_SCANNED_ENTRIES {
                return Ok(());
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if blocked_workspace_name(name) {
                continue;
            }
            let file_type = entry.file_type().map_err(|error| {
                ToolError::execution("workspace_traversal", error.to_string(), false)
            })?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let relative = relative_string(root, &path)?;
                if !visitor(&path, &relative) {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn readable_path(root: &Path, relative: &str) -> Result<PathBuf, ToolError> {
    resolve_workspace_resource(root, relative)
}

fn writable_path(root: &Path, relative: &str) -> Result<(PathBuf, String), ToolError> {
    let relative_path = Path::new(relative);
    if relative.trim().is_empty()
        || relative.len() > 4_096
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
        || relative_path
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .any(blocked_workspace_name)
    {
        return Err(ToolError::PermissionDenied(
            "path is outside the writable workspace boundary".into(),
        ));
    }
    let parent = relative_path.parent().unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(root.join(parent))
        .map_err(|error| ToolError::execution("apply_patch", error.to_string(), false))?;
    if !parent.starts_with(root) {
        return Err(ToolError::PermissionDenied(
            "path escaped the writable workspace boundary".into(),
        ));
    }
    let file_name = relative_path
        .file_name()
        .ok_or_else(|| ToolError::InvalidArgument("path must identify a file".into()))?;
    let absolute = parent.join(file_name);
    if absolute
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(ToolError::PermissionDenied(
            "symbolic-link writes are not allowed".into(),
        ));
    }
    if absolute.exists() && !absolute.is_file() {
        return Err(ToolError::InvalidArgument(
            "path must identify a regular file".into(),
        ));
    }
    Ok((absolute, relative_path.to_string_lossy().replace('\\', "/")))
}

fn glob_regex(pattern: &str) -> Result<Regex, ToolError> {
    if pattern.is_empty() || pattern.len() > 1_024 || pattern.contains('\\') {
        return Err(ToolError::InvalidArgument(
            "glob pattern must contain 1..=1024 bytes and use / separators".into(),
        ));
    }
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut expression = String::from("^");
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                if chars.get(index) == Some(&'/') {
                    expression.push_str("(?:.*/)?");
                    index += 1;
                } else {
                    expression.push_str(".*");
                }
            }
            '*' => {
                expression.push_str("[^/]*");
                index += 1;
            }
            '?' => {
                expression.push_str("[^/]");
                index += 1;
            }
            character => {
                expression.push_str(&regex::escape(&character.to_string()));
                index += 1;
            }
        }
    }
    expression.push('$');
    Regex::new(&expression)
        .map_err(|error| ToolError::InvalidArgument(format!("invalid glob pattern: {error}")))
}

fn parse_input<T>(tool: &str, parameters: &Value) -> Result<T, ToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(parameters.clone())
        .map_err(|error| ToolError::InvalidArgument(format!("{tool}: {error}")))
}

fn relative_string(root: &Path, path: &Path) -> Result<String, ToolError> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| ToolError::PermissionDenied("path escaped the workspace".into()))
}

fn validate_patch_size(content: &str) -> Result<(), ToolError> {
    if content.len() > MAX_PATCH_FILE_BYTES {
        return Err(ToolError::InvalidArgument(
            "patched file exceeds 256 KiB".into(),
        ));
    }
    Ok(())
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn json_output(value: Value) -> Result<RawToolOutput, ToolError> {
    let mut output = RawToolOutput {
        content: vec![ToolContent::Json(value)],
        ..RawToolOutput::default()
    };
    output.metadata.insert("bounded".into(), "true".into());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_supports_recursive_and_single_segment_patterns() {
        let rust = glob_regex("**/*.rs").unwrap();
        assert!(rust.is_match("src/lib.rs"));
        assert!(rust.is_match("lib.rs"));
        assert!(!rust.is_match("src/lib.ts"));
        let direct = glob_regex("src/*.rs").unwrap();
        assert!(direct.is_match("src/lib.rs"));
        assert!(!direct.is_match("src/deep/lib.rs"));
    }

    #[test]
    fn find_and_search_are_bounded_and_skip_sensitive_paths() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "fn alpha() {}\n").unwrap();
        std::fs::write(temp.path().join(".git/config"), "alpha secret").unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();

        let found = find_files(&root, &json!({"pattern": "**/*.rs"}), || false).unwrap();
        assert_eq!(
            found.content,
            vec![ToolContent::Json(json!({
                "matches": ["src/lib.rs"],
                "count": 1,
                "scanned": 1,
                "truncated": false
            }))]
        );

        let searched = search_files(&root, &json!({"query": "alpha"}), || false).unwrap();
        let ToolContent::Json(value) = &searched.content[0] else {
            panic!("expected JSON output")
        };
        assert_eq!(value["count"], 1);
        assert_eq!(value["matches"][0]["path"], "src/lib.rs");
        assert_eq!(value["matches"][0]["line"], 1);
    }

    #[test]
    fn patch_requires_hash_and_rejects_ambiguous_replacement() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("note.txt"), "same same").unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let hash = sha256(b"same same");

        let error = plan_patch(
            &root,
            &json!({
                "path": "note.txt",
                "operation": "update",
                "expected_sha256": hash,
                "old_text": "same",
                "new_text": "new"
            }),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("ambiguous"));

        let patch = plan_patch(
            &root,
            &json!({
                "path": "note.txt",
                "operation": "update",
                "expected_sha256": sha256(b"same same"),
                "old_text": "same same",
                "new_text": "new"
            }),
        )
        .unwrap();
        assert_eq!(patch.content, "new");
        assert_eq!(patch.replacements, 1);
    }

    #[test]
    fn patch_rejects_traversal_and_stale_hash() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("note.txt"), "current").unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        assert!(plan_patch(
            &root,
            &json!({"path":"../escape.txt","operation":"create","new_text":"x"})
        )
        .is_err());
        assert!(plan_patch(
            &root,
            &json!({
                "path":"note.txt",
                "operation":"update",
                "expected_sha256": sha256(b"stale"),
                "old_text":"current",
                "new_text":"updated"
            })
        )
        .is_err());
    }
}
