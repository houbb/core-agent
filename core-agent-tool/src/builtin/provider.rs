use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{PermissionDecision, ToolCapability, ToolDefinition, ToolProviderDefinition, ToolProviderKind};
use crate::infrastructure::{ToolProvider, ToolRegistration};

use super::file::*;
use super::shell::*;
use super::git::*;
use super::web::*;
use super::ask::*;
use super::todo::*;
use super::agent::*;
use super::plan::*;
use super::cron::*;
use super::lsp::*;

/// Provider that registers all 41 builtin tools.
pub struct BuiltinToolProvider {
    definition: ToolProviderDefinition,
    registrations: Vec<ToolRegistration>,
}

impl BuiltinToolProvider {
    pub fn new() -> Self {
        let definition = ToolProviderDefinition::new(
            "builtin",
            "Builtin Tools",
            ToolProviderKind::Builtin,
        );
        let registrations = Self::collect_all_tools();
        Self { definition, registrations }
    }

    fn tool(
        name: &str,
        description: &str,
        category: &str,
        permission: PermissionDecision,
        timeout_ms: u64,
        schema: serde_json::Value,
        tool: Arc<dyn crate::infrastructure::Tool>,
        capabilities: &[&str],
    ) -> ToolRegistration {
        let key = format!("builtin/{name}@1.0.0");
        let cap_set: BTreeSet<ToolCapability> = capabilities
            .iter()
            .filter_map(|c| ToolCapability::new(c).ok())
            .collect();
        let mut definition = ToolDefinition::new("builtin", name, "1.0.0", schema);
        definition.key = key;
        definition.description = description.into();
        definition.category = category.into();
        definition.capabilities = cap_set;
        definition.default_permission = permission;
        definition.timeout_ms = timeout_ms;
        ToolRegistration::new(definition, tool)
    }

    fn collect_all_tools() -> Vec<ToolRegistration> {
        let mut tools: Vec<ToolRegistration> = Vec::new();

        // === File Tools (11) ===
        let file_schema = |props: serde_json::Value, required: Vec<&str>| -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "required": required,
                "properties": props,
                "additionalProperties": false
            })
        };

        tools.push(Self::tool(
            "file.read", "Read the content of a file at the given path.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "File path to read"},
                "limit": {"type": "integer", "description": "Maximum lines to read", "minimum": 1},
                "offset": {"type": "integer", "description": "Line offset to start from", "minimum": 0}
            }), vec!["path"]),
            file_read_tool(), &["file", "file.read"],
        ));

        tools.push(Self::tool(
            "file.write", "Create or overwrite a file.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "File path to write"},
                "content": {"type": "string", "description": "Content to write"}
            }), vec!["path", "content"]),
            file_write_tool(), &["file", "file.write"],
        ));

        tools.push(Self::tool(
            "file.edit", "Replace exact text in a file (old_string → new_string).",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "File path to edit"},
                "old_string": {"type": "string", "description": "Exact text to replace"},
                "new_string": {"type": "string", "description": "Replacement text"},
                "replace_all": {"type": "boolean", "description": "Replace all occurrences", "default": false}
            }), vec!["path", "old_string", "new_string"]),
            file_edit_tool(), &["file", "file.edit"],
        ));

        tools.push(Self::tool(
            "file.patch", "Apply multiple edits to multiple files in one call.",
            "file", PermissionDecision::Allow, 60_000,
            file_schema(serde_json::json!({
                "patches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["path", "old_string", "new_string"],
                        "properties": {
                            "path": {"type": "string"},
                            "old_string": {"type": "string"},
                            "new_string": {"type": "string"}
                        }
                    },
                    "minItems": 1,
                    "maxItems": 100
                }
            }), vec!["patches"]),
            file_patch_tool(), &["file", "file.patch"],
        ));

        tools.push(Self::tool(
            "file.glob", "Find files matching a glob pattern.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "pattern": {"type": "string", "description": "Glob pattern (e.g. **/*.rs)"},
                "path": {"type": "string", "description": "Base directory"}
            }), vec!["pattern"]),
            file_glob_tool(), &["file", "file.glob"],
        ));

        tools.push(Self::tool(
            "file.grep", "Search file contents using regex patterns.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "pattern": {"type": "string", "description": "Regex pattern to search"},
                "path": {"type": "string", "description": "Search scope path"},
                "glob": {"type": "string", "description": "File glob filter"},
                "-i": {"type": "boolean", "description": "Case insensitive search"},
                "output_mode": {"type": "string", "enum": ["content", "files_with_matches", "count"], "default": "content"},
                "context": {"type": "integer", "description": "Lines of context", "minimum": 0}
            }), vec!["pattern"]),
            file_grep_tool(), &["file", "file.grep", "search"],
        ));

        tools.push(Self::tool(
            "file.delete", "Delete a file or empty directory.",
            "file", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Path to delete"}
            }), vec!["path"]),
            file_delete_tool(), &["file", "file.delete"],
        ));

        tools.push(Self::tool(
            "file.move", "Move or rename a file or directory.",
            "file", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "source": {"type": "string", "description": "Source path"},
                "dest": {"type": "string", "description": "Destination path"}
            }), vec!["source", "dest"]),
            file_move_tool(), &["file", "file.move"],
        ));

        tools.push(Self::tool(
            "file.copy", "Copy a file or directory.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "source": {"type": "string", "description": "Source path"},
                "dest": {"type": "string", "description": "Destination path"}
            }), vec!["source", "dest"]),
            file_copy_tool(), &["file", "file.copy"],
        ));

        tools.push(Self::tool(
            "file.info", "Get metadata about a file or directory.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Path to inspect"}
            }), vec!["path"]),
            file_info_tool(), &["file", "file.info"],
        ));

        tools.push(Self::tool(
            "file.list", "List the contents of a directory.",
            "file", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Directory path"},
                "include_hidden": {"type": "boolean", "description": "Include hidden files", "default": false}
            }), vec!["path"]),
            file_list_tool(), &["file", "file.list"],
        ));

        // === Shell Tools (3) ===
        tools.push(Self::tool(
            "shell.exec", "Execute a shell command and return its output.",
            "shell", PermissionDecision::Ask, 120_000,
            file_schema(serde_json::json!({
                "command": {"type": "string", "description": "Shell command to execute"},
                "working_dir": {"type": "string", "description": "Working directory"},
                "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds", "minimum": 1000, "maximum": 600000},
                "env": {"type": "object", "description": "Environment variables", "additionalProperties": {"type": "string"}}
            }), vec!["command"]),
            shell_exec_tool(), &["shell", "shell.exec"],
        ));

        tools.push(Self::tool(
            "shell.script", "Execute a script file. (Default: Deny for security)",
            "shell", PermissionDecision::Deny, 120_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Script file path"}
            }), vec!["path"]),
            shell_script_tool(), &["shell", "shell.script"],
        ));

        tools.push(Self::tool(
            "shell.bg", "Execute a command in the background.",
            "shell", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "command": {"type": "string", "description": "Command to run in background"},
                "working_dir": {"type": "string", "description": "Working directory"}
            }), vec!["command"]),
            shell_bg_tool(), &["shell", "shell.bg"],
        ));

        // === Git Tools (7) ===
        tools.push(Self::tool(
            "git.diff", "Show working tree changes.",
            "git", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Repository path"},
                "staged": {"type": "boolean", "description": "Show staged changes only", "default": false},
                "working_dir": {"type": "string", "description": "Working directory"}
            }), vec![]),
            git_diff_tool(), &["git", "git.diff"],
        ));

        tools.push(Self::tool(
            "git.status", "Show repository status.",
            "git", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Repository path"}
            }), vec![]),
            git_status_tool(), &["git", "git.status"],
        ));

        tools.push(Self::tool(
            "git.log", "Show commit history.",
            "git", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Repository path"},
                "max_count": {"type": "integer", "description": "Maximum commits to show", "minimum": 1, "maximum": 1000}
            }), vec![]),
            git_log_tool(), &["git", "git.log"],
        ));

        tools.push(Self::tool(
            "git.commit", "Create a commit with staged changes.",
            "git", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "message": {"type": "string", "description": "Commit message"},
                "all": {"type": "boolean", "description": "Auto-stage all changes", "default": true},
                "path": {"type": "string", "description": "Repository path"}
            }), vec!["message"]),
            git_commit_tool(), &["git", "git.commit"],
        ));

        tools.push(Self::tool(
            "git.branch", "List or create branches.",
            "git", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "name": {"type": "string", "description": "Branch name (optional, creates if provided)"},
                "path": {"type": "string", "description": "Repository path"}
            }), vec![]),
            git_branch_tool(), &["git", "git.branch"],
        ));

        tools.push(Self::tool(
            "git.checkout", "Switch branches or restore files.",
            "git", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "branch": {"type": "string", "description": "Branch to switch to"},
                "path": {"type": "string", "description": "Repository path"}
            }), vec!["branch"]),
            git_checkout_tool(), &["git", "git.checkout"],
        ));

        tools.push(Self::tool(
            "git.push", "Push commits to remote. (Default: Deny for safety)",
            "git", PermissionDecision::Deny, 60_000,
            file_schema(serde_json::json!({
                "remote": {"type": "string", "description": "Remote name", "default": "origin"},
                "branch": {"type": "string", "description": "Branch to push"},
                "path": {"type": "string", "description": "Repository path"}
            }), vec![]),
            Arc::new(StubTool("builtin/git.push@1.0.0", "git.push is disabled by default")),
            &["git", "git.push"],
        ));

        // === Web Tools (2) ===
        tools.push(Self::tool(
            "web.fetch", "Fetch a URL and return its content.",
            "web", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "url": {"type": "string", "description": "URL to fetch"}
            }), vec!["url"]),
            web_fetch_tool(), &["web", "web.fetch"],
        ));

        tools.push(Self::tool(
            "web.search", "Search the web using a search engine.",
            "web", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "Search query"}
            }), vec!["query"]),
            web_search_tool(), &["web", "web.search"],
        ));

        // === Ask Tools (3) ===
        tools.push(Self::tool(
            "ask.user", "Ask the user a question and get their answer.",
            "ask", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "question": {"type": "string", "description": "Question to ask the user"}
            }), vec!["question"]),
            ask_user_tool(), &["ask", "ask.user"],
        ));

        tools.push(Self::tool(
            "ask.confirm", "Ask the user for a Yes/No confirmation.",
            "ask", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "message": {"type": "string", "description": "Confirmation message"}
            }), vec!["message"]),
            ask_confirm_tool(), &["ask", "ask.confirm"],
        ));

        tools.push(Self::tool(
            "ask.select", "Ask the user to choose from a list of options.",
            "ask", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "question": {"type": "string", "description": "Question to ask"},
                "options": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 10}
            }), vec!["question", "options"]),
            ask_select_tool(), &["ask", "ask.select"],
        ));

        // === Todo Tools (3) ===
        tools.push(Self::tool(
            "todo.add", "Add a todo item.",
            "todo", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "task": {"type": "string", "description": "Task description"}
            }), vec!["task"]),
            todo_add_tool(), &["todo", "todo.add"],
        ));

        tools.push(Self::tool(
            "todo.update", "Update a todo item status.",
            "todo", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "id": {"type": "integer", "description": "Todo item ID"},
                "status": {"type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"], "description": "New status"}
            }), vec!["id", "status"]),
            todo_update_tool(), &["todo", "todo.update"],
        ));

        tools.push(Self::tool(
            "todo.list", "List all todo items.",
            "todo", PermissionDecision::Allow, 30_000,
            serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
            todo_list_tool(), &["todo", "todo.list"],
        ));

        // === Agent Tools (3) ===
        tools.push(Self::tool(
            "agent.spawn", "Create a sub-agent for a task.",
            "agent", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "task": {"type": "string", "description": "Task description for the sub-agent"},
                "capabilities": {"type": "array", "items": {"type": "string"}, "description": "Required capabilities"}
            }), vec!["task"]),
            agent_spawn_tool(), &["agent", "agent.spawn"],
        ));

        tools.push(Self::tool(
            "agent.send", "Send a message to another agent.",
            "agent", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "agent_id": {"type": "string", "description": "Target agent ID"},
                "message": {"type": "string", "description": "Message content"}
            }), vec!["agent_id", "message"]),
            agent_send_tool(), &["agent", "agent.send"],
        ));

        tools.push(Self::tool(
            "agent.list", "List active sub-agents.",
            "agent", PermissionDecision::Allow, 30_000,
            serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
            agent_list_tool(), &["agent", "agent.list"],
        ));

        // === Plan Tools (3) ===
        tools.push(Self::tool(
            "plan.create", "Create an execution plan.",
            "plan", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "goal": {"type": "string", "description": "Goal description"},
                "tasks": {"type": "array", "items": {"type": "string"}, "description": "Task list"}
            }), vec!["goal"]),
            plan_create_tool(), &["plan", "plan.create"],
        ));

        tools.push(Self::tool(
            "plan.update", "Update plan status.",
            "plan", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "plan_id": {"type": "string", "description": "Plan ID"},
                "status": {"type": "string", "description": "New status"}
            }), vec!["plan_id", "status"]),
            plan_update_tool(), &["plan", "plan.update"],
        ));

        tools.push(Self::tool(
            "plan.review", "Review a plan for approval.",
            "plan", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "plan_id": {"type": "string", "description": "Plan ID to review"}
            }), vec!["plan_id"]),
            plan_review_tool(), &["plan", "plan.review"],
        ));

        // === Cron Tools (3) ===
        tools.push(Self::tool(
            "cron.create", "Create a scheduled task.",
            "cron", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "expression": {"type": "string", "description": "Cron expression"},
                "task": {"type": "string", "description": "Task description"}
            }), vec!["expression", "task"]),
            cron_create_tool(), &["cron", "cron.create"],
        ));

        tools.push(Self::tool(
            "cron.list", "List scheduled tasks.",
            "cron", PermissionDecision::Allow, 30_000,
            serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
            cron_list_tool(), &["cron", "cron.list"],
        ));

        tools.push(Self::tool(
            "cron.delete", "Delete a scheduled task.",
            "cron", PermissionDecision::Ask, 30_000,
            file_schema(serde_json::json!({
                "id": {"type": "string", "description": "Scheduled task ID"}
            }), vec!["id"]),
            cron_delete_tool(), &["cron", "cron.delete"],
        ));

        // === LSP Tools (6) ===
        tools.push(Self::tool(
            "lsp.definition", "Go to definition of a symbol.",
            "lsp", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "symbol": {"type": "string", "description": "Symbol name"},
                "path": {"type": "string", "description": "File path"}
            }), vec!["symbol"]),
            lsp_definition_tool(), &["lsp", "lsp.definition"],
        ));

        tools.push(Self::tool(
            "lsp.references", "Find references to a symbol.",
            "lsp", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "symbol": {"type": "string", "description": "Symbol name"},
                "path": {"type": "string", "description": "File path"}
            }), vec!["symbol"]),
            lsp_references_tool(), &["lsp", "lsp.references"],
        ));

        tools.push(Self::tool(
            "lsp.hover", "Show type information and documentation.",
            "lsp", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "symbol": {"type": "string", "description": "Symbol name"},
                "path": {"type": "string", "description": "File path"}
            }), vec!["symbol"]),
            lsp_hover_tool(), &["lsp", "lsp.hover"],
        ));

        tools.push(Self::tool(
            "lsp.completion", "Get code completion suggestions.",
            "lsp", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "prefix": {"type": "string", "description": "Code prefix"},
                "path": {"type": "string", "description": "File path"}
            }), vec!["prefix"]),
            lsp_completion_tool(), &["lsp", "lsp.completion"],
        ));

        tools.push(Self::tool(
            "lsp.diagnostics", "Get diagnostics for a file.",
            "lsp", PermissionDecision::Allow, 30_000,
            serde_json::json!({"type": "object", "properties": {
                "path": {"type": "string", "description": "File path"}
            }, "additionalProperties": false}),
            lsp_diagnostics_tool(), &["lsp", "lsp.diagnostics"],
        ));

        tools.push(Self::tool(
            "lsp.symbols", "Search workspace symbols.",
            "lsp", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "Symbol query"}
            }), vec!["query"]),
            lsp_symbols_tool(), &["lsp", "lsp.symbols"],
        ));

        tools
    }
}

#[async_trait]
impl ToolProvider for BuiltinToolProvider {
    fn definition(&self) -> ToolProviderDefinition {
        self.definition.clone()
    }

    async fn discover(&self) -> crate::error::ToolRuntimeResult<Vec<ToolRegistration>> {
        Ok(self.registrations.clone())
    }
}

mod stub {
    use std::sync::Arc;
    use async_trait::async_trait;
    use crate::domain::{RawToolOutput, ToolRequest};
    use crate::error::{ToolError, ToolRuntimeResult};
    use crate::infrastructure::{Tool, ToolContext};

    pub struct StubTool(pub &'static str, pub &'static str);

    #[async_trait]
    impl Tool for StubTool {
        fn key(&self) -> &str { self.0 }
        async fn execute(&self, _req: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
            Err(ToolError::execution("stub", self.1, false))
        }
    }
}

use stub::StubTool;