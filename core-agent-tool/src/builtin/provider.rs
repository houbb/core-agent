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
use super::ast::*;
use super::code_index::*;
use super::dependency::*;
use super::decompiler::*;
use super::project::*;
use super::runtime::*;
use super::enterprise::*;
use super::ai::*;
use super::user::*;

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
            "git.push", "Push commits to remote.",
            "git", PermissionDecision::Ask, 60_000,
            file_schema(serde_json::json!({
                "remote": {"type": "string", "description": "Remote name", "default": "origin"},
                "branch": {"type": "string", "description": "Branch to push"},
                "force": {"type": "boolean", "description": "Force push with lease", "default": false},
                "set_upstream": {"type": "boolean", "description": "Set upstream tracking", "default": true},
                "path": {"type": "string", "description": "Repository path"},
                "working_dir": {"type": "string", "description": "Working directory"}
            }), vec![]),
            git_push_tool(), &["git", "git.push"],
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

        // === AST Tools (2) ===
        tools.push(Self::tool(
            "ast.search", "Search code using AST-aware patterns with language filtering.",
            "ast", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "pattern": {"type": "string", "description": "Regex pattern to search"},
                "language": {"type": "string", "description": "Programming language filter (java, rust, python, ts, go, etc.)", "default": "all"},
                "path": {"type": "string", "description": "Search directory"}
            }), vec!["pattern"]),
            ast_search_tool(), &["ast", "ast.search"],
        ));

        tools.push(Self::tool(
            "ast.replace", "Replace code patterns with rewrite templates.",
            "ast", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "pattern": {"type": "string", "description": "Regex pattern to match"},
                "rewrite": {"type": "string", "description": "Replacement text"},
                "language": {"type": "string", "description": "Programming language filter", "default": "all"},
                "path": {"type": "string", "description": "Search directory"},
                "dry_run": {"type": "boolean", "description": "Preview changes without applying", "default": false}
            }), vec!["pattern", "rewrite"]),
            ast_replace_tool(), &["ast", "ast.replace"],
        ));

        // === Code Index Tools (2) ===
        tools.push(Self::tool(
            "code_index.index", "Scan directory and extract symbols (classes, methods, fields).",
            "code_index", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Directory to scan"},
                "language": {"type": "string", "description": "Programming language", "default": "all"}
            }), vec![]),
            code_index_index_tool(), &["code-index", "code-index.index"],
        ));

        tools.push(Self::tool(
            "code_index.query", "Query symbols from the code index by name.",
            "code_index", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "symbol": {"type": "string", "description": "Symbol name to search"},
                "kind": {"type": "string", "description": "Symbol kind (class, method, field, all)", "default": "all"},
                "path": {"type": "string", "description": "Search directory"},
                "language": {"type": "string", "description": "Programming language", "default": "all"}
            }), vec!["symbol"]),
            code_index_query_tool(), &["code-index", "code-index.query"],
        ));

        // === Dependency Tools (1) ===
        tools.push(Self::tool(
            "dependency.inspect", "Inspect project dependencies for various languages.",
            "dependency", PermissionDecision::Allow, 60_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Project directory"},
                "language": {"type": "string", "description": "Language (java, rust, node, python, auto)", "default": "auto"}
            }), vec![]),
            dependency_inspect_tool(), &["dependency", "dependency.inspect"],
        ));

        // === Decompiler Tools (1) ===
        tools.push(Self::tool(
            "decompiler.decompile", "Decompile Java class files or JAR archives.",
            "decompiler", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Path to .class or .jar file"},
                "class": {"type": "string", "description": "Specific class name to decompile from JAR"},
                "verbose": {"type": "boolean", "description": "Verbose output", "default": false}
            }), vec!["path"]),
            decompiler_decompile_tool(), &["decompiler", "decompiler.decompile"],
        ));

        // === Project Tools (4) ===
        tools.push(Self::tool(
            "project.analyzer", "Analyze project structure and identify framework.",
            "project", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Project directory"}
            }), vec![]),
            project_analyzer_tool(), &["project", "project.analyzer"],
        ));

        tools.push(Self::tool(
            "architecture.graph", "Generate architecture dependency graph in JSON/text.",
            "project", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Project directory"},
                "format": {"type": "string", "enum": ["json", "text"], "default": "json"}
            }), vec![]),
            architecture_graph_tool(), &["architecture", "architecture.graph"],
        ));

        tools.push(Self::tool(
            "callgraph.query", "Analyze function call relationships.",
            "project", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "function": {"type": "string", "description": "Function name to trace"},
                "path": {"type": "string", "description": "Search directory"},
                "depth": {"type": "integer", "description": "Max call depth", "minimum": 1, "maximum": 10, "default": 3}
            }), vec!["function"]),
            callgraph_query_tool(), &["callgraph", "callgraph.query"],
        ));

        tools.push(Self::tool(
            "api.analyzer", "Analyze REST API endpoints in a project.",
            "project", PermissionDecision::Allow, 30_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Project directory"},
                "language": {"type": "string", "description": "Language (java, node, rust, auto)", "default": "auto"}
            }), vec![]),
            api_analyzer_tool(), &["api", "api.analyzer"],
        ));

        // === Runtime/Observability Tools (5, stub) ===
        tools.push(Self::tool(
            "log.query", "Query logs from ELK/Loki/ClickHouse. (Requires configuration)",
            "runtime", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "Log query string"},
                "time_range": {"type": "string", "description": "Time range (e.g. 1h, 24h)"}
            }), vec![]),
            log_query_tool(), &["observability", "log.query"],
        ));

        tools.push(Self::tool(
            "metric.query", "Query metrics from Prometheus. (Requires configuration)",
            "runtime", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "PromQL query"},
                "time_range": {"type": "string", "description": "Time range"}
            }), vec![]),
            metric_query_tool(), &["observability", "metric.query"],
        ));

        tools.push(Self::tool(
            "trace.query", "Query traces from Jaeger/SkyWalking. (Requires configuration)",
            "runtime", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "trace_id": {"type": "string", "description": "Trace ID to query"},
                "service": {"type": "string", "description": "Service name filter"}
            }), vec![]),
            trace_query_tool(), &["observability", "trace.query"],
        ));

        tools.push(Self::tool(
            "cmdb.query", "Query CMDB for service/instance/owner info. (Requires configuration)",
            "runtime", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "Search query"},
                "type": {"type": "string", "description": "Entity type (service, instance, owner)", "default": "service"}
            }), vec![]),
            cmdb_query_tool(), &["cmdb", "cmdb.query"],
        ));

        tools.push(Self::tool(
            "k8s.query", "Query Kubernetes resources. (Requires kubectl)",
            "runtime", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "resource": {"type": "string", "description": "K8s resource type (pods, deployments, services)", "default": "pods"},
                "namespace": {"type": "string", "description": "K8s namespace", "default": "default"}
            }), vec![]),
            k8s_query_tool(), &["k8s", "k8s.query"],
        ));

        // === Enterprise Tools (4, stub) ===
        tools.push(Self::tool(
            "knowledge.search", "Search knowledge base / Vector DB / Wiki. (Requires configuration)",
            "enterprise", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "query": {"type": "string", "description": "Search query"}
            }), vec!["query"]),
            knowledge_search_tool(), &["knowledge", "knowledge.search"],
        ));

        tools.push(Self::tool(
            "ticket.create", "Create a ticket in Jira/ServiceNow. (Requires configuration)",
            "enterprise", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "title": {"type": "string", "description": "Ticket title"},
                "description": {"type": "string", "description": "Ticket description"},
                "priority": {"type": "string", "description": "Priority (low, medium, high, critical)", "default": "medium"}
            }), vec!["title"]),
            ticket_create_tool(), &["ticket", "ticket.create"],
        ));

        tools.push(Self::tool(
            "notification.send", "Send notification via Slack/DingTalk/Email. (Requires configuration)",
            "enterprise", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "channel": {"type": "string", "description": "Notification channel"},
                "message": {"type": "string", "description": "Message content"}
            }), vec!["message"]),
            notification_send_tool(), &["notification", "notification.send"],
        ));

        tools.push(Self::tool(
            "browser.navigate", "Navigate to a URL using browser automation. (Requires Playwright)",
            "enterprise", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "url": {"type": "string", "description": "URL to navigate to"}
            }), vec!["url"]),
            browser_navigate_tool(), &["browser", "browser.navigate"],
        ));

        tools.push(Self::tool(
            "browser.screenshot", "Take a screenshot of a page. (Requires Playwright)",
            "enterprise", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "url": {"type": "string", "description": "URL to screenshot"}
            }), vec!["url"]),
            browser_screenshot_tool(), &["browser", "browser.screenshot"],
        ));

        // === AI Tools (5, stub) ===
        tools.push(Self::tool(
            "code.review", "Review code changes for quality and security. (Requires LLM/SAST)",
            "ai", PermissionDecision::Deny, 60_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "File or directory to review"},
                "diff": {"type": "boolean", "description": "Review against git diff", "default": false}
            }), vec![]),
            code_review_tool(), &["ai", "code.review"],
        ));

        tools.push(Self::tool(
            "test.generate", "Generate unit/integration tests. (Requires LLM)",
            "ai", PermissionDecision::Deny, 60_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Source file path"},
                "framework": {"type": "string", "description": "Test framework", "default": "auto"}
            }), vec![]),
            test_generate_tool(), &["ai", "test.generate"],
        ));

        tools.push(Self::tool(
            "security.scan", "Scan code for security vulnerabilities. (Requires Semgrep/SonarQube)",
            "ai", PermissionDecision::Deny, 60_000,
            file_schema(serde_json::json!({
                "path": {"type": "string", "description": "Path to scan"},
                "severity": {"type": "string", "description": "Minimum severity", "default": "all"}
            }), vec![]),
            security_scan_tool(), &["ai", "security.scan"],
        ));

        tools.push(Self::tool(
            "data.analyze", "Analyze data from SQL/CSV/Excel sources. (Requires configuration)",
            "ai", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "source": {"type": "string", "description": "Data source"},
                "query": {"type": "string", "description": "Query or analysis request"}
            }), vec![]),
            data_analyze_tool(), &["ai", "data.analyze"],
        ));

        tools.push(Self::tool(
            "vision.analyze", "Analyze images/screenshots using vision models. (Requires vision model)",
            "ai", PermissionDecision::Deny, 30_000,
            file_schema(serde_json::json!({
                "image": {"type": "string", "description": "Image file path"},
                "prompt": {"type": "string", "description": "Analysis prompt", "default": "Describe this image"}
            }), vec![]),
            vision_analyze_tool(), &["ai", "vision.analyze"],
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