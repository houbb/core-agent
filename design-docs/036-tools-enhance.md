# 036-tools-enhance: 博采众长，全面增强 Tool Runtime

## 目标

基于 OpenCode / Claude Code / Codex 三家工具设计理念，结合 `core-agent-tool` 现有 Runtime 基础设施，实现：

1. **所有工具插件化** — 每个 Tool 都是 `ToolProvider` 中的 `FunctionTool` 实例，通过 `BuiltinToolProvider` 统一注册
2. **统一可配置** — 通过 `core-agent-config` 配置层控制工具权限、超时、启停，Terminal 和 Desktop 共享同一套配置
3. **Agent 可正确触发** — 通过 `ToolCapability` 能力匹配 + 标准 Tool Calling → ToolRequest → 执行链路
4. **内置工具开箱即用** — 覆盖文件、Shell、Git、Web、代码智能、人机交互等核心场景，总计 41 个工具

---

## 现状分析

### 已有基础设施（无需修改）

`core-agent-tool` 已经提供了完善的 Tool Runtime 架构：

| 组件 | 状态 | 用途 |
|------|------|------|
| `ToolManager` | ✅ 完成 | 统一的执行入口，管理生命周期、权限、校验 |
| `ToolRegistry` | ✅ 完成 | 运行时 Tool 注册/发现 |
| `ToolCatalog` | ✅ 完成 | Tool 元数据持久化（SQLite） |
| `ToolExecutor` | ✅ 完成 | 执行委托 |
| `ToolValidator` | ✅ 完成 | JSON Schema 校验 |
| `ToolPermission` | ✅ 完成 | 权限检查（Allow/Ask/Deny） |
| `ToolLifecycle` | ✅ 完成 | 生命周期状态机 |
| `ToolInterceptor` | ✅ 完成 | 请求/结果拦截 |
| `ToolObserver` | ✅ 完成 | 观测/审计 |
| `ToolPolicy` | ✅ 完成 | 企业策略 |
| `FunctionTool` | ✅ 完成 | 函数式 Tool 适配器 |
| `StaticToolProvider` | ✅ 完成 | 静态 Provider，注册一组 Tool |
| `SqliteToolStore` | ✅ 完成 | 持久化存储 |
| `ToolCapability` | ✅ 完成 | 能力路径（如 `file.read`） |
| `PermissionDecision` | ✅ 完成 | Allow/Ask/Deny |

### 需要新增（不在已有代码中）

| 缺失点 | 方案 |
|--------|------|
| 内置工具实现 | 新增 `builtin/` 模块，每个工具一个文件 |
| 统一 Provider 注册 | 新增 `BuiltinToolProvider`，注册所有内置工具 |
| 配置驱动权限 | 新增 `ConfigDrivenPermission`，读取配置覆盖工具权限 |
| 配置层扩展 | 在 `core-agent-config` 中新增 `tools` 配置段 |
| Agent 触发链路 | 在 `core-agent-agent` 中实现 `handle_tool_call()` |

---

## 整体架构

```
                    Agent Runtime (core-agent-agent)
                           │
                           │  LLM Tool Call → ToolRequest
                           ▼
                    ToolManager.execute()
                           │
            ┌──────────────┼──────────────────┐
            │  Validate    │  Permission       │  Policy
            │  (JSON Schema)│  (Allow/Ask/Deny)  │  (企业策略)
            └──────────────┼──────────────────┘
                           │
                           ▼
                    ToolRegistry.find()
                           │
                           ▼
              ┌─────────────────────────────┐
              │      ToolProvider           │
              │                             │
              │  BuiltinToolProvider  ←── 新增：注册所有内置工具
              │    ├── file.* (11个)        │
              │    ├── shell.* (3个)        │
              │    ├── git.* (7个)          │
              │    ├── web.* (2个)          │
              │    ├── ask.* (3个)          │
              │    ├── todo.* (3个)         │
              │    ├── agent.* (3个)        │
              │    ├── plan.* (3个)         │
              │    ├── cron.* (3个)         │
              │    └── lsp.* (6个)          │
              │                             │
              │  McpToolProvider       ←── 未来：MCP Server 工具
              │  PluginToolProvider    ←── 未来：插件扩展工具
              │  RemoteToolProvider    ←── 未来：远程 HTTP 工具
              └─────────────────────────────┘
                           │
                           ▼
                    ToolExecutor.invoke()
                           │
                           ▼
                    ToolResult
```

---

## 一、工具插件化实现方案

### 1.1 每个 Tool 是一个 FunctionTool + ToolDefinition

```rust
// 示例：file.read 工具定义
let file_read_tool = FunctionTool::new("builtin/file.read@1.0.0", |request, _context| {
    async move {
        let path = request.parameters["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("path is required".into()))?;
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| ToolError::execution("file.read", e.to_string(), false))?;
        Ok(RawToolOutput::text(content))
    }
});

let file_read_definition = ToolDefinition {
    key: "builtin/file.read@1.0.0".into(),
    provider_key: "builtin".into(),
    name: "file.read".into(),
    description: "Read the content of a file at the given path.".into(),
    input_schema: serde_json::json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {
                "type": "string",
                "description": "The absolute path to the file to read"
            },
            "limit": {
                "type": "integer",
                "description": "Optional maximum number of lines to read",
                "minimum": 1
            },
            "offset": {
                "type": "integer",
                "description": "Optional line offset to start reading from",
                "minimum": 0
            }
        },
        "additionalProperties": false
    }),
    category: "file".into(),
    capabilities: {
        let mut set = BTreeSet::new();
        set.insert(ToolCapability::new("file.read").unwrap());
        set
    },
    default_permission: PermissionDecision::Allow,
    timeout_ms: 30_000,
    // ... 其余字段使用默认值
};
```

### 1.2 BuiltinToolProvider 统一注册所有内置工具

```rust
pub struct BuiltinToolProvider {
    definition: ToolProviderDefinition,
    registrations: Vec<ToolRegistration>,
}

impl BuiltinToolProvider {
    pub fn new() -> Self {
        Self {
            definition: ToolProviderDefinition::new(
                "builtin", "Builtin Tools", ToolProviderKind::Builtin,
            ),
            registrations: Self::collect_all_tools(),
        }
    }

    fn collect_all_tools() -> Vec<ToolRegistration> {
        let mut tools = Vec::new();
        // 文件工具
        tools.extend(Self::file_tools());
        // Shell 工具
        tools.extend(Self::shell_tools());
        // Git 工具
        tools.extend(Self::git_tools());
        // Web 工具
        tools.extend(Self::web_tools());
        // 人机交互工具
        tools.extend(Self::ask_tools());
        // 任务管理工具
        tools.extend(Self::todo_tools());
        // Agent 工具
        tools.extend(Self::agent_tools());
        // 规划工具
        tools.extend(Self::plan_tools());
        // 调度工具
        tools.extend(Self::cron_tools());
        // LSP 工具
        tools.extend(Self::lsp_tools());
        tools
    }
}
```

### 1.3 加载 Provider

```rust
// 在 App 初始化时
let mut tool_manager = ToolManager::builder()
    .catalog(sqlite_store.clone())
    .permission(config_driven_permission)
    .lifecycle(sqlite_store.clone())
    .build();

// 加载所有内置工具（一次调用注册所有）
tool_manager.load_provider(&BuiltinToolProvider::new()).await?;

// 加载 MCP 工具（如果配置了）
if let Some(mcp_config) = &config.tools.providers.mcp {
    for server in &mcp_config.servers {
        tool_manager.load_provider(&McpToolProvider::new(server)).await?;
    }
}
```

---

## 二、完整 Tool 清单与定义

### 2.1 文件操作工具 (File Tools) — 11 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `file.read` | `file.read` | Allow | 读取文件内容，支持行范围 |
| `file.write` | `file.write` | Allow | 创建/覆盖文件 |
| `file.edit` | `file.edit` | Allow | 精确替换（old_string → new_string） |
| `file.patch` | `file.patch` | Allow | 批量修改（多个 edit 组合） |
| `file.glob` | `file.glob` | Allow | 文件模式匹配发现 |
| `file.grep` | `file.grep` | Allow | 全文内容搜索（ripgrep） |
| `file.delete` | `file.delete` | Ask | 删除文件 |
| `file.move` | `file.move` | Ask | 移动/重命名文件 |
| `file.copy` | `file.copy` | Allow | 复制文件 |
| `file.info` | `file.info` | Allow | 获取文件元信息 |
| `file.list` | `file.list` | Allow | 列出目录内容 |

**`file.read` Schema：**
```json
{
  "type": "object", "required": ["path"],
  "properties": {
    "path": { "type": "string", "description": "File path to read" },
    "limit": { "type": "integer", "description": "Max lines to read", "minimum": 1 },
    "offset": { "type": "integer", "description": "Line offset", "minimum": 0 }
  },
  "additionalProperties": false
}
```

**`file.edit` Schema：**
```json
{
  "type": "object", "required": ["path", "old_string", "new_string"],
  "properties": {
    "path": { "type": "string", "description": "File path to edit" },
    "old_string": { "type": "string", "description": "Exact text to replace" },
    "new_string": { "type": "string", "description": "Replacement text" },
    "replace_all": { "type": "boolean", "default": false }
  },
  "additionalProperties": false
}
```

**`file.patch` Schema：**
```json
{
  "type": "object", "required": ["patches"],
  "properties": {
    "patches": {
      "type": "array",
      "items": {
        "type": "object", "required": ["path", "old_string", "new_string"],
        "properties": {
          "path": { "type": "string" },
          "old_string": { "type": "string" },
          "new_string": { "type": "string" }
        }
      },
      "minItems": 1, "maxItems": 100
    }
  }
}
```

**`file.grep` Schema：**
```json
{
  "type": "object", "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string", "description": "Regex pattern to search" },
    "path": { "type": "string", "description": "Search scope path" },
    "glob": { "type": "string", "description": "File glob filter" },
    "-i": { "type": "boolean", "description": "Case insensitive" },
    "output_mode": { "type": "string", "enum": ["content", "files_with_matches", "count"] },
    "context": { "type": "integer", "description": "Lines of context" }
  }
}
```

### 2.2 Shell 执行工具 (Shell Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `shell.exec` | `shell.exec` | Ask | 执行 Shell 命令 |
| `shell.script` | `shell.script` | Deny | 执行脚本文件 |
| `shell.bg` | `shell.bg` | Ask | 后台执行命令 |

**`shell.exec` Schema：**
```json
{
  "type": "object", "required": ["command"],
  "properties": {
    "command": { "type": "string", "description": "Shell command to execute" },
    "working_dir": { "type": "string", "description": "Working directory" },
    "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 600000 },
    "env": { "type": "object", "additionalProperties": { "type": "string" } }
  }
}
```

### 2.3 Git 工具 (Git Tools) — 7 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `git.diff` | `git.diff` | Allow | 查看工作区变更 |
| `git.status` | `git.status` | Allow | 查看仓库状态 |
| `git.log` | `git.log` | Allow | 查看提交历史 |
| `git.commit` | `git.commit` | Ask | 创建提交 |
| `git.branch` | `git.branch` | Allow | 分支操作 |
| `git.checkout` | `git.checkout` | Ask | 切换分支/恢复文件 |
| `git.push` | `git.push` | Deny | 推送代码 |

### 2.4 代码智能工具 (LSP Tools) — 6 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `lsp.definition` | `lsp.definition` | Allow | 跳转到定义 |
| `lsp.references` | `lsp.references` | Allow | 查找引用 |
| `lsp.hover` | `lsp.hover` | Allow | 查看类型/文档 |
| `lsp.completion` | `lsp.completion` | Allow | 代码补全 |
| `lsp.diagnostics` | `lsp.diagnostics` | Allow | 获取诊断信息 |
| `lsp.symbols` | `lsp.symbols` | Allow | 工作区符号搜索 |

### 2.5 网络工具 (Web Tools) — 2 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `web.fetch` | `web.fetch` | Allow | 获取网页内容 |
| `web.search` | `web.search` | Allow | 搜索引擎查询 |

### 2.6 人机交互工具 (Ask Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `ask.user` | `ask.user` | Allow | 向用户提问获取答案 |
| `ask.confirm` | `ask.confirm` | Allow | 请求用户确认（Yes/No） |
| `ask.select` | `ask.select` | Allow | 让用户从选项中选择 |

### 2.7 任务管理工具 (Todo Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `todo.add` | `todo.add` | Allow | 添加待办项 |
| `todo.update` | `todo.update` | Allow | 更新待办状态 |
| `todo.list` | `todo.list` | Allow | 列出待办 |

### 2.8 Agent 工具 (Agent Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `agent.spawn` | `agent.spawn` | Ask | 创建子 Agent |
| `agent.send` | `agent.send` | Allow | 向 Agent 发消息 |
| `agent.list` | `agent.list` | Allow | 列出活跃 Agent |

### 2.9 规划工具 (Plan Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `plan.create` | `plan.create` | Allow | 创建执行计划 |
| `plan.update` | `plan.update` | Allow | 更新计划状态 |
| `plan.review` | `plan.review` | Allow | 审查计划 |

### 2.10 调度工具 (Cron Tools) — 3 个

| Tool | 能力 | 默认权限 | 描述 |
|------|------|---------|------|
| `cron.create` | `cron.create` | Ask | 创建定时任务 |
| `cron.list` | `cron.list` | Allow | 列出定时任务 |
| `cron.delete` | `cron.delete` | Ask | 删除定时任务 |

---

## 三、统一配置化方案

### 3.1 配置层扩展

在 `core-agent-config` 中新增 `ConfigTools` 域对象：

```rust
// core-agent-config/src/domain.rs 新增
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTools {
    #[serde(default = "default_tools_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tools_permission")]
    pub default_permission: String,
    #[serde(default)]
    pub overrides: Vec<ConfigToolOverride>,
    #[serde(default)]
    pub providers: ConfigToolProviders,
    #[serde(default)]
    pub capability_groups: Vec<ConfigCapabilityGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigToolOverride {
    pub tool: String,
    pub permission: Option<String>,
    pub timeout_ms: Option<u64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigCapabilityGroup {
    pub name: String,
    pub capabilities: Vec<String>,
    pub permission: String,
}
```

### 3.2 用户配置文件示例

```yaml
# core-agent-config.yaml
version: 2
activeModel: deepseek-v4-flash
# ... 现有配置 ...

tools:
  enabled: true
  defaultPermission: ask

  overrides:
    - tool: "file.read"
      permission: allow
      timeout_ms: 30000
    - tool: "file.write"
      permission: allow
    - tool: "file.edit"
      permission: allow
    - tool: "file.delete"
      permission: ask
    - tool: "shell.exec"
      permission: ask
      timeout_ms: 120000
    - tool: "shell.script"
      permission: deny
      enabled: false
    - tool: "git.push"
      permission: deny
    - tool: "git.commit"
      permission: ask
    - tool: "web.search"
      permission: allow
      enabled: true

  providers:
    builtin:
      enabled: true
    mcp:
      enabled: true
      servers:
        - name: "github"
          url: "http://localhost:8080/mcp/github"
    plugin:
      enabled: false

  capabilityGroups:
    - name: "development"
      capabilities: ["file.*", "shell.exec", "git.*", "lsp.*"]
      permission: allow
    - name: "production"
      capabilities: ["shell.exec", "git.push"]
      permission: deny
    - name: "network"
      capabilities: ["web.*"]
      permission: ask
```

### 3.3 ConfigDrivenPermission 实现

```rust
/// 读取配置的工具权限，支持覆盖和能力组匹配
pub struct ConfigDrivenPermission {
    config: ConfigTools,
}

impl ConfigDrivenPermission {
    pub fn new(config: &ConfigTools) -> Self {
        Self { config: config.clone() }
    }
}

#[async_trait]
impl ToolPermission for ConfigDrivenPermission {
    async fn check(
        &self,
        request: &ToolRequest,
        tool: &ToolDefinition,
    ) -> ToolRuntimeResult<PermissionDecision> {
        // 1. 精确匹配 override
        if let Some(override_) = self.config.overrides.iter().find(|o| o.tool == tool.name) {
            if let Some(permission) = &override_.permission {
                if let Some(decision) = PermissionDecision::parse(permission) {
                    return Ok(decision);
                }
            }
        }

        // 2. 能力组匹配
        for group in &self.config.capability_groups {
            let matches = tool.capabilities.iter().any(|cap| {
                group.capabilities.iter().any(|pattern| {
                    matches_capability_pattern(cap.as_str(), pattern)
                })
            });
            if matches {
                if let Some(decision) = PermissionDecision::parse(&group.permission) {
                    return Ok(decision);
                }
            }
        }

        // 3. 回退到工具默认权限
        Ok(tool.default_permission)
    }
}

/// 能力模式匹配，支持通配符
/// "file.*" 匹配 "file.read", "file.write" 等
fn matches_capability_pattern(capability: &str, pattern: &str) -> bool {
    if pattern.ends_with(".*") {
        let prefix = &pattern[..pattern.len() - 2];
        capability == prefix || capability.starts_with(&format!("{}.", prefix))
    } else {
        capability == pattern
    }
}
```

### 3.4 配置与现有系统的集成

```rust
// 在 App 初始化时
let config = standard_config_manager()?
    .resolve(&ConfigRequest::global()).await?;

let config_permission = ConfigDrivenPermission::new(&config.config.tools);

let tool_manager = ToolManager::builder()
    .catalog(sqlite_store.clone())
    .permission(config_permission)
    .lifecycle(sqlite_store.clone())
    .build();

// 加载内置工具
tool_manager.load_provider(&BuiltinToolProvider::new()).await?;
```

---

## 四、Agent 正确触发 Tool 的完整链路

### 4.1 LLM Tool Calling → ToolRequest 映射

```rust
// 在 Agent Runtime 中处理 LLM 返回的 Tool Call
impl AgentRuntime {
    async fn handle_tool_call(&self, tool_call: ToolCall) -> AgentResult<ToolResult> {
        // Step 1: 解析 LLM 输出的 Tool Call
        let tool_name = &tool_call.name;        // "file.read"
        let arguments = &tool_call.arguments;    // {"path": "/src/main.rs"}

        // Step 2: 构建完整的 Tool Key
        let tool_key = format!("builtin/{}@1.0.0", tool_name);

        // Step 3: 构建 ToolRequest
        let request = ToolRequest {
            id: Uuid::new_v4(),
            tool: tool_key,
            parameters: arguments.clone(),
            session_id: Some(self.session_id),
            subject: Some(self.actor.clone()),
            metadata: BTreeMap::from([
                ("trace_id".into(), self.trace_id.to_string()),
                ("session_id".into(), self.session_id.to_string()),
            ]),
            timeout_ms: None,
            created_at: Utc::now(),
        };

        // Step 4: 通过 ToolManager 执行（自动完成：校验 → 权限 → 策略 → 执行）
        self.tool_manager.execute(request).await
            .map_err(|e| AgentError::from(e))
    }
}
```

### 4.2 执行流程（完整链路）

```
LLM Output (Tool Call)
    │
    │  tool_call.name = "file.read"
    │  tool_call.arguments = {"path": "/src/main.rs"}
    ▼
Agent Runtime.handle_tool_call()
    │
    ├── 1. 解析 Tool Call
    │   tool_name = "file.read"
    │   params = {"path": "/src/main.rs"}
    │
    ├── 2. 构建完整 Tool Key
    │   key = format!("builtin/{}@1.0.0", tool_name)
    │   → "builtin/file.read@1.0.0"
    │
    ├── 3. 构建 ToolRequest
    │   request.tool = "builtin/file.read@1.0.0"
    │   request.parameters = {"path": "/src/main.rs"}
    │   request.session_id = "xxx"
    │   request.subject = "agent-1"
    │
    ├── 4. ToolManager.execute() 内部
    │   │
    │   ├── 4a. Interceptor 拦截请求
    │   │   (可选：日志、审计、修改参数)
    │   │
    │   ├── 4b. Catalog 查找 Tool Definition
    │   │   → 找到 file.read 定义，确认启用
    │   │
    │   ├── 4c. Validator 校验参数
    │   │   → JSON Schema 校验：path 必填，类型 string
    │   │
    │   ├── 4d. Policy 评估
    │   │   → 企业策略检查（白名单、黑名单、额度）
    │   │
    │   ├── 4e. Permission 检查
    │   │   → ConfigDrivenPermission.check()
    │   │   → 1) 精确 override → Allow
    │   │   → 2) 能力组匹配 → 无
    │   │   → 3) 回退默认 → Allow
    │   │
    │   ├── 4f. Registry 查找 Tool 实例
    │   │   → 找到 FunctionTool 实例
    │   │
    │   ├── 4g. Executor 执行
    │   │   → 调用 file_read_handler(request, context)
    │   │   → tokio::fs::read_to_string("/src/main.rs")
    │   │
    │   └── 4h. Mapper 映射结果
    │       → RawToolOutput → ToolResult
    │
    └── 5. 返回 ToolResult 给 Agent
        result.status = Success
        result.content = [Text("fn main() { ... }")]
```

### 4.3 通过 Capability 匹配（Planner 场景）

```rust
/// Planner 不直接硬编码工具名称，而是通过能力查找
impl Planner {
    async fn find_tools_for_task(&self, task: &Task) -> PlannerResult<Vec<ToolDefinition>> {
        // 根据任务需求推导所需能力
        let required_capability = match task.kind {
            TaskKind::ReadFile => ToolCapability::new("file.read")?,
            TaskKind::SearchCode => ToolCapability::new("file.grep")?,
            TaskKind::ExecuteCommand => ToolCapability::new("shell.exec")?,
            TaskKind::CreateCommit => ToolCapability::new("git.commit")?,
            _ => return Ok(vec![]),
        };

        // 通过能力查找工具（可跨 Provider）
        let tools = self.tool_manager
            .find_by_capability(&required_capability, false)
            .await?;

        Ok(tools)
    }
}
```

### 4.4 Provider 热切换

```rust
// 场景：从 Builtin 切换到 MCP 实现
// 1. 卸载旧的
tool_manager.unregister_tool("builtin/file.read@1.0.0").await?;

// 2. 注册新的（MCP Provider 提供相同能力）
tool_manager.load_provider(&mcp_file_provider).await?;

// 3. Planner 通过 Capability 查询自动找到新的实现
let tools = tool_manager.find_by_capability(
    &ToolCapability::new("file.read").unwrap(), false
).await?;
// → ["mcp/s3-file.read@1.0.0"]
```

---

## 五、目录结构

### 5.1 新增代码目录

```
core-agent-tool/src/
├── lib.rs                          # 导出 BuiltinToolProvider
├── domain/                         # ✅ 已有，无需修改
├── error.rs                        # ✅ 已有
├── application/                    # ✅ 已有
├── infrastructure/                 # ✅ 已有
│   └── defaults.rs                 # 🆕 新增 ConfigDrivenPermission
├── providers/                      # ✅ 已有
│   ├── mod.rs
│   ├── function.rs
│   └── static_provider.rs
│
└── builtin/                        # 🆕 新增：内置工具实现
    ├── mod.rs
    ├── provider.rs                 # BuiltinToolProvider
    ├── file/
    │   ├── mod.rs
    │   ├── read.rs
    │   ├── write.rs
    │   ├── edit.rs
    │   ├── patch.rs
    │   ├── glob.rs
    │   ├── grep.rs
    │   ├── delete.rs
    │   ├── move_.rs
    │   ├── copy.rs
    │   ├── info.rs
    │   └── list.rs
    ├── shell/
    │   ├── mod.rs
    │   ├── exec.rs
    │   ├── script.rs
    │   └── bg.rs
    ├── git/
    │   ├── mod.rs
    │   ├── diff.rs
    │   ├── status.rs
    │   ├── log.rs
    │   ├── commit.rs
    │   ├── branch.rs
    │   └── checkout.rs
    ├── web/
    │   ├── mod.rs
    │   ├── fetch.rs
    │   └── search.rs
    ├── ask/
    │   ├── mod.rs
    │   ├── user.rs
    │   ├── confirm.rs
    │   └── select.rs
    ├── todo/
    │   ├── mod.rs
    │   ├── add.rs
    │   ├── update.rs
    │   └── list.rs
    ├── agent/
    │   ├── mod.rs
    │   ├── spawn.rs
    │   └── send.rs
    ├── plan/
    │   ├── mod.rs
    │   ├── create.rs
    │   ├── update.rs
    │   └── review.rs
    ├── cron/
    │   ├── mod.rs
    │   ├── create.rs
    │   ├── list.rs
    │   └── delete.rs
    └── lsp/
        ├── mod.rs
        ├── definition.rs
        ├── references.rs
        ├── hover.rs
        ├── completion.rs
        ├── diagnostics.rs
        └── symbols.rs
```

### 5.2 配置层新增

```
core-agent-config/src/
├── lib.rs                          # 🆕 导出 ConfigTools
├── domain.rs                       # 🆕 新增 ConfigTools 域对象
```

### 5.3 Agent Runtime 新增

```
core-agent-agent/src/
├── lib.rs
├── domain.rs
├── coordinator.rs
├── manager.rs
├── infrastructure.rs
└── tool_call.rs                    # 🆕 新增：Tool Call 处理
```

---

## 六、与现有代码的集成点

### 6.1 不修改现有核心

现有基础设施**完全不需要修改**：

- `ToolManager` — 已经支持 `load_provider()` 动态加载
- `ToolRegistry` — 已经支持 `register()` / `find()`
- `ToolCatalog` — 已经支持 `upsert_tool()` / `find_by_capability()`
- `ToolPermission` — 通过 `check()` 接口注入新实现
- `ToolValidator` — 已经支持 JSON Schema 校验
- `SqliteToolStore` — 已经支持持久化
- `ToolCapability` — 已经支持能力路径匹配

### 6.2 新增组件依赖关系

```
core-agent-tool (新增 builtin/ 模块)
    ├── builtin/file.read   → tokio::fs
    ├── builtin/shell.exec  → tokio::process::Command
    ├── builtin/git.*       → 调用 git CLI
    ├── builtin/web.fetch   → reqwest (已有依赖)
    ├── builtin/web.search  → reqwest + 搜索引擎 API
    ├── builtin/ask.*       → 事件系统通知
    ├── builtin/todo.*      → SQLite 存储
    ├── builtin/agent.*     → core-agent-agent (可选)
    ├── builtin/plan.*      → core-agent-plan (可选)
    └── builtin/lsp.*       → lsp-client crate (新增依赖)

core-agent-config (新增 ConfigTools 域对象)
    └── 依赖已有分层配置机制

core-agent-agent (新增 tool_call.rs)
    └── 依赖 core-agent-tool
```

### 6.3 在 App 层组装

```rust
// core-agent-app/src/lib.rs
pub async fn build_agent_runtime(config: &ResolvedConfig) -> AgentRuntime {
    let sqlite = Arc::new(SqliteToolStore::new(":memory:").unwrap());

    let tool_manager = ToolManager::builder()
        .catalog(sqlite.clone())
        .permission(ConfigDrivenPermission::new(&config.config.tools))
        .lifecycle(sqlite.clone())
        .build();

    // 加载内置工具
    tool_manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();

    // 构建 Agent Runtime
    AgentRuntime::builder()
        .tool_manager(tool_manager)
        .build()
}
```

---

## 七、测试策略

### 7.1 单元测试（每个工具独立）

```rust
#[tokio::test]
async fn file_read_returns_content() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.txt");
    tokio::fs::write(&path, "hello world").await.unwrap();

    let tool = file_read_tool();
    let request = ToolRequest::new(
        "builtin/file.read@1.0.0",
        serde_json::json!({"path": path.to_string_lossy()}),
    );
    let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
    assert_eq!(result.content[0], ToolContent::Text("hello world".into()));
}

#[tokio::test]
async fn file_read_rejects_empty_path() {
    let request = ToolRequest::new(
        "builtin/file.read@1.0.0",
        serde_json::json!({"path": ""}),
    );
    let result = file_read_tool().execute(&request, &ToolContext::default()).await;
    assert!(result.is_err());
}
```

### 7.2 集成测试（完整 Tool Runtime 链路）

```rust
#[tokio::test]
async fn builtin_tool_provider_registers_all_tools() {
    let store = Arc::new(SqliteToolStore::new(":memory:").unwrap());
    let manager = ToolManager::builder()
        .catalog(store.clone())
        .lifecycle(store.clone())
        .build();

    let count = manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    assert_eq!(count, 41, "should register all 41 builtin tools");

    let tools = manager.list().await.unwrap();
    assert_eq!(tools.len(), 41);

    // 按类别验证
    let file_tools = manager.find_by_capability(
        &ToolCapability::new("file").unwrap(), true
    ).await.unwrap();
    assert_eq!(file_tools.len(), 11);

    let shell_tools = manager.find_by_capability(
        &ToolCapability::new("shell").unwrap(), true
    ).await.unwrap();
    assert_eq!(shell_tools.len(), 3);
}

#[tokio::test]
async fn config_driven_permission_overrides_default() {
    let config = ConfigTools {
        overrides: vec![ConfigToolOverride {
            tool: "shell.exec".into(),
            permission: Some("DENY".into()),
            timeout_ms: None,
            enabled: Some(true),
        }],
        ..Default::default()
    };
    let permission = ConfigDrivenPermission::new(&config);

    let mut definition = ToolDefinition::new("builtin", "shell.exec", "1", json!({}));
    definition.default_permission = PermissionDecision::Ask;

    let decision = permission.check(&ToolRequest::new("", json!({})), &definition).await.unwrap();
    assert_eq!(decision, PermissionDecision::Deny);
}
```

### 7.3 E2E 测试（Agent → Tool 调用链路）

```rust
#[tokio::test]
async fn agent_handle_tool_call_invokes_correct_tool() {
    let mut app = TestApp::new().await;

    // 模拟 LLM 输出的 Tool Call
    let tool_call = ToolCall {
        name: "file.read".into(),
        arguments: json!({"path": "/tmp/test.txt"}),
    };

    // Agent 处理 Tool Call
    let result = app.agent.handle_tool_call(tool_call).await.unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert!(!result.content.is_empty());
}
```

---

## 八、实现优先级

### Phase 1: 核心文件工具 (P0) — 预计 5 天

| 工具 | 工作量 | 依赖 |
|------|--------|------|
| `file.read` | 1天 | 无 |
| `file.write` | 1天 | 无 |
| `file.edit` | 1天 | 无 |
| `file.glob` | 1天 | 无 |
| `file.grep` | 1天 | 无 |
| `file.list` | 0.5天 | 无 |
| `file.info` | 0.5天 | 无 |
| `BuiltinToolProvider` | 1天 | 以上工具 |

### Phase 2: Shell + Git 工具 (P0) — 预计 4 天

| 工具 | 工作量 | 依赖 |
|------|--------|------|
| `shell.exec` | 2天 | 权限控制 |
| `git.diff` | 0.5天 | shell.exec |
| `git.status` | 0.5天 | shell.exec |
| `git.log` | 0.5天 | shell.exec |
| `git.commit` | 0.5天 | shell.exec |

### Phase 3: 配置驱动 + 人机交互 (P1) — 预计 3 天

| 组件 | 工作量 | 依赖 |
|------|--------|------|
| `ConfigTools` 域对象 | 1天 | core-agent-config |
| `ConfigDrivenPermission` | 1天 | 以上 |
| `ask.user` | 1天 | 事件系统 |

### Phase 4: 网络 + 任务管理 (P1) — 预计 3 天

| 工具 | 工作量 | 依赖 |
|------|--------|------|
| `web.fetch` | 1天 | reqwest |
| `web.search` | 1天 | 搜索 API |
| `todo.*` | 1天 | SQLite |

### Phase 5: 高级工具 (P2) — 预计 5 天

| 工具 | 工作量 | 依赖 |
|------|--------|------|
| `file.patch` | 1天 | file.edit |
| `agent.spawn` | 2天 | core-agent-agent |
| `plan.*` | 1天 | core-agent-plan |
| `cron.*` | 1天 | 调度器 |

### Phase 6: LSP 工具 (P2) — 预计 5 天

| 工具 | 工作量 | 依赖 |
|------|--------|------|
| LSP 客户端集成 | 3天 | lsp-client crate |
| `lsp.*` 工具 | 2天 | 以上 |

---

## 九、总览

| 维度 | 现状 | 增强后 |
|------|------|--------|
| 工具数量 | 0 个内置工具（只有 Runtime） | 41 个内置工具 |
| 插件化 | 架构支持，无实例 | 全部工具通过 `BuiltinToolProvider` 插件化注册 |
| 配置化 | 只有 Runtime 配置 | 统一工具级配置（权限、超时、启停） |
| Agent 触发 | 无 | 标准 Tool Calling → ToolRequest → 执行链路 |
| 能力匹配 | 架构支持，未使用 | Planner 通过 Capability 自动发现工具 |
| 测试覆盖 | Runtime 层测试 | 单元测试 + 集成测试 + E2E 测试 |
| Desktop/Terminal 共享 | 无 | 共享同一套配置 + 同一套工具实现 |