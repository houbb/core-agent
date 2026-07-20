# 内置 Agent / SubAgent 现状分析

> 对标 Claude Code、ChatGPT (Codex)、OpenCode，梳理当前 Agent 子类型体系，识别缺失，按优先级排序补充。

---

## 一、当前已有的 Agent / SubAgent 全景

### 1.1 SubAgent Profile（基础子智能体）

| Profile | 位置 | 描述 | 工具限制 |
|---------|------|------|----------|
| `General` | `src/subagent_runtime.rs` | 通用委托任务，隔离独立模型上下文 | 只读：`filesystem.read`、`guidance.read`、`memory.read`、`process.read` |
| `Explore` | `src/subagent_runtime.rs` | 只读探索工作区回答问题 | 同上，prompt 强调"搜索/读取/不修改" |
| `Review` | `src/subagent_runtime.rs` | 代码/设计审查 | 同上，prompt 强调"正确性/安全/回归风险" |

**支持机制：**
- 最大 4 轮只读 Tool 循环，超限报错
- 独立模型上下文，与父会话隔离
- 支持 CancellationToken 取消
- 16 KiB 任务 / 128 KiB 输出限制

### 1.2 Agent（完整 Agent Runtime）

路径：`core-agent-agent/`

| 概念 | 实现 |
|------|------|
| Agent 定义 | `AgentProfile`（key/name/model/planner/workspace/memory/policy/capabilities/toolset） |
| 生命周期 | 8 种状态：CREATED → READY → RUNNING → WAITING → PAUSED → COMPLETED → FAILED → DESTROYED |
| 操作 | 9 种：CREATE / START / RUN / STOP / FINISH / DESTROY / SNAPSHOT / RESTORE / RECONCILE |
| 策略 | `AgentPolicyDefinition` 支持 ALLOW / ASK / DENY 逐操作控制 |
| 持久化 | `InMemoryAgentStore` + `SqliteAgentStore` |
| 快照 | `AgentSnapshot` 支持 Checkpoint 恢复 |

### 1.3 Multi-Agent（多智能体协作）

路径：`core-agent-multi/`

| 概念 | 实现 |
|------|------|
| 组织 | `Organization` — 多 Agent 归属顶级容器 |
| 团队 | `Team` — 目标驱动的 Agent 团队，有状态机 + 策略 |
| 角色 | `Role` — 能力要求定义的抽象角色 |
| 成员 | `AgentMember` — Agent 在团队中的实例，8 种状态 |
| 协作 | `Collaboration` — 完整的任务分配/执行/结果/流转 |
| 路由 | `DeterministicAgentRouter`、`AgentDirectory`、`AgentDispatcher` |

### 1.4 内置 Agent 工具（41 个 Builtin Tools）

| 类别 | 工具 | 用途 |
|------|------|------|
| **Agent** | `agent.spawn` | 创建子 Agent |
| | `agent.list` | 列出活跃 Agent |
| | `agent.send` | 发送消息给 Agent |
| | `ask.user` | 向用户提问 |
| **Plan** | `plan.create` | 创建计划 |
| | `plan.update` | 更新计划 |
| | `plan.review` | 审查计划 |
| **File** | `file.read` / `file.write` / `file.edit` / `file.patch` | 文件读写 |
| | `file.search` / `file.glob` / `file.grep` | 文件搜索 |
| | `file.diff` / `file.rename` / `file.delete` | 文件操作 |
| **Shell** | `run_command` / `start_command` / `poll_command` / `cancel_command` | 命令执行 |
| **Git** | `git.status` / `git.diff` / `git.log` / `git.commit` / `git.push` / `git.branch` | Git 操作 |
| **Web** | `web_search` / `web_fetch` | 网络搜索 |
| **Memory** | `remember_memory` / `recall_memory` / `forget_memory` | 记忆管理 |
| **Cron** | `cron_create` / `cron_delete` / `cron_list` | 定时任务 |
| **LSP** | `lsp.hover` / `lsp.definition` / `lsp.references` / `lsp.completion` | 语言服务 |
| **AST** | `ast.parse` / `ast.query` | 代码结构分析 |
| **Code** | `code_index` / `dependency_graph` | 代码索引 |
| **Project** | `project_scan` / `project_insight` | 项目扫描 |
| **Runtime** | `runtime_info` / `runtime_metrics` | 运行时信息 |
| **Enterprise** | `enterprise_audit` / `enterprise_policy` | 企业治理 |
| **AI** | `ai_embed` / `ai_generate` | AI 辅助 |
| **Todo** | `todo_write` / `todo_read` | 任务列表 |

### 1.5 Slash 命令（作为 Agent 入口）

| 命令 | 路由 | 功能 |
|------|------|------|
| `/plan` | Agent | 创建实施计划 |
| `/review` | Agent | 审查当前变更 |
| `/explain` | Agent | 解释代码 |
| `/test` | Agent | 运行/规划测试 |
| `/fix` | Agent | 修复当前问题 |
| `/refactor` | Agent | 重构目标 |
| `/commit` | Agent | 生成提交提案 |
| `/pr` | Agent | 生成 PR 提案 |

---

## 二、对标分析

### 2.1 Claude Code 的 Agent 类型

| 类型 | 用途 | 本项目是否已有 |
|------|------|:---:|
| `claude`（通用 Agent） | 兜底全能 Agent | ✅ `General` |
| `claude-code-guide` | Claude Code 使用指南 | ❌ 缺失 |
| `Explore` | 只读搜索/探索 | ✅ 同名 |
| `Plan` | 架构设计/实现计划 | ✅ `/plan` 命令 |
| `general-purpose` | 通用多步任务 | ✅ `General` |
| `statusline-setup` | 状态行配置 | ❌ 缺失 |
| `deep-research` | 深度研究 | ❌ 缺失 |
| `dataviz` | 数据可视化 | ❌ 缺失 |
| `simplify` | 代码简化 | ❌ 缺失 |
| `review` | 代码审查 | ✅ `Review` |
| `security-review` | 安全审查 | ❌ 缺失 |
| `loop` | 定时循环任务 | ❌ 缺失 |
| `run` | 启动运行应用 | ❌ 缺失 |
| `init` | 初始化 CLAUDE.md | ❌ 缺失 |
| `fewer-permission-prompts` | 权限优化 | ❌ 缺失 |

### 2.2 ChatGPT / Codex 的典型能力

| 能力 | 说明 | 本项目是否已有 |
|------|------|:---:|
| 通用代码生成 | 多语言代码生成 | ✅ 通过 LLM 内置 |
| 代码审查 | 审查变更、建议改进 | ✅ `Review` |
| 代码解释 | 解释代码逻辑 | ✅ `/explain` |
| 测试生成 | 自动生成单元测试 | ⚠️ `/test` 仅为入口 |
| 调试诊断 | 分析错误栈、定位 Bug | ❌ 缺失专用 Agent |
| 架构设计 | 系统设计、模块划分 | ✅ `/plan` |
| 文档生成 | 自动生成/更新文档 | ❌ 缺失专用 Agent |
| 重构 | 大规模代码重构 | ✅ `/refactor` |
| 迁移 | 版本迁移、框架升级 | ❌ 缺失专用 Agent |
| PR 审查 | 全面 PR 审查 | ✅ `/pr` + `/review` |
| 安全审计 | 安全漏洞扫描 | ❌ 缺失专用 Agent |

### 2.3 OpenCode 的典型能力

| 能力 | 说明 | 本项目是否已有 |
|------|------|:---:|
| Shell 执行 | 命令运行 | ✅ `run_command` |
| 文件读写 | 文件操作 | ✅ 完整 |
| 搜索 | Glob/Grep | ✅ 完整 |
| 编辑 | 增量编辑/补丁 | ✅ `file.edit` / `file.patch` |
| Web | 搜索/抓取 | ✅ 完整 |
| Git | 版本控制 | ✅ 完整 |
| LSP | 语言服务 | ✅ 完整 |

---

## 三、缺失 Agent 类型及优先级

### P0 — 必须补充（核心开发体验缺失）

| 缺失 Agent | 对标源 | 缺失影响 | 建议实现方式 |
|-----------|:------:|----------|-------------|
| **Test Agent** — 测试生成/运行/诊断 | Codex / Claude Code | 当前 `/test` 只是入口，没有专用 Agent 分析测试失败、生成测试用例、自动化测试修复 | 新增 `SubAgentProfile::Test`，集成 `run_command` + `file.grep` + `code_index`，能在测试失败后自动分析栈追踪并修复 |
| **Debug Agent** — 错误诊断/调试 | Codex / ChatGPT | 异常栈追踪、log 分析、定位根因是最高频开发场景之一，当前只能靠人工 /explain | 新增 `SubAgentProfile::Debug`，集成 `run_command` + `file.read` + `lsp`，可分析错误输出并定位代码位置 |
| **Code Review Agent**（增强） | Claude Code `security-review` | 当前 `Review` 只有通用审查，缺少安全专项审查 | 在 `Review` 基础上增加安全审查维度，新增 `SubAgentProfile::SecurityReview` |

### P1 — 重要补充（效率提升）

| 缺失 Agent | 对标源 | 缺失影响 | 建议实现方式 |
|-----------|:------:|----------|-------------|
| **Doc Agent** — 文档生成/更新 | Codex / Claude Code | 每次功能变更后手动更新 README/CHANGELOG 繁琐，容易遗漏 | 新增 `SubAgentProfile::Doc`，集成 `file.read` + `file.write` + `git.diff`，自动生成变更日志 |
| **Migration Agent** — 代码迁移/升级 | Codex | 框架升级（如 Spring Boot 2→3）、API 迁移等场景高频且易出错 | 新增 `SubAgentProfile::Migration`，集成 `file.glob` + `file.grep` + `file.edit` + `ast.parse` |
| **Architecture Agent** — 架构分析/设计评审 | Claude Code `Plan` | 当前 `/plan` 偏实施计划，缺少架构层面的依赖分析、耦合度检查 | 增强 `Plan` 或新增 `SubAgentProfile::Architecture`，集成 `dependency_graph` + `code_index` + `ast.parse` |

### P2 — 锦上添花（特定场景优化）

| 缺失 Agent | 对标源 | 缺失影响 | 建议实现方式 |
|-----------|:------:|----------|-------------|
| **Deploy Agent** — 部署/CI-CD 诊断 | Codex / ChatGPT | 部署失败时排查复杂 | 新增 `SubAgentProfile::Deploy`，集成 `run_command` + `web_fetch` |
| **DataViz Agent** — 数据可视化 | Claude Code | 性能分析、数据展示场景 | 新增 Skill，非 SubAgent |
| **Research Agent** — 深度研究 | Claude Code | 需要多源信息综合的场景 | 新增 `SubAgentProfile::Research`，集成 `web_search` + `web_fetch` |
| **Init Agent** — 项目初始化 | Claude Code | 新项目/新模块初始化配置 | 新增 `SubAgentProfile::Init`，集成 `file.write` + `run_command` |

---

## 四、优先级总结

```
优先级     Agent 类型              对标源             当前状态
─────────────────────────────────────────────────────────────────
P0 🔴     Test Agent              Codex/CC           ❌ 缺失
P0 🔴     Debug Agent             Codex/CC           ❌ 缺失
P0 🔴     Security Review Agent   CC security-review ❌ 缺失

P1 🟡     Doc Agent               Codex/CC           ❌ 缺失
P1 🟡     Migration Agent         Codex              ❌ 缺失
P1 🟡     Architecture Agent      CC Plan (增强)      ⚠️ 部分

P2 🟢     Deploy Agent            Codex/CC           ❌ 缺失
P2 🟢     Research Agent          CC deep-research   ❌ 缺失
P2 🟢     Init Agent              CC init            ❌ 缺失
P2 🟢     DataViz Agent           CC dataviz         ❌ 缺失
```

### 推荐实施路线

1. **P0 阶段** — 扩展 `SubAgentProfile` 枚举，增加 `Test` / `Debug` / `SecurityReview` 三个 Profile，各自有专用 Prompt + 工具集约束
2. **P1 阶段** — 增加 `Doc` / `Migration` / `Architecture` 三个 Profile，复用现有工具集
3. **P2 阶段** — 增加 `Deploy` / `Research` / `Init` 等场景化 Profile

每个 Profile 的核心差异在于：
- **Prompt** — 角色定位 + 行为约束
- **Tool Filter** — 允许访问的工具集合（当前硬编码为 `filesystem.read | guidance.read | memory.read | process.read`，需扩展为可配置）
- **Max Turns** — 允许的推理轮数（当前硬编码 4 轮）

---

## 五、核心变更点

### 5.1 当前 SubAgent 限制

```rust
// src/subagent_runtime.rs — 当前硬编码限制
const MAX_SUBAGENT_TASK_BYTES: usize = 16 * 1024;   // 16 KiB 任务上限
const MAX_SUBAGENT_OUTPUT_BYTES: usize = 128 * 1024; // 128 KiB 输出上限
// 最大 4 轮 tool 循环
// 工具过滤只允许 4 类只读 category
```

### 5.2 需要改造的点

1. **`SubAgentProfile` 枚举扩展** — 从 3 个 → 10+ 个 Profile
2. **Tool Filter 可配置化** — 每个 Profile 可配置允许的工具集，不再硬编码 4 类只读
3. **Max Turns 可配置化** — Test/Debug Agent 可能需要更多轮次
4. **Profile 注册机制** — 支持通过配置文件或代码注册新的 SubAgent Profile
5. **Profile 与 Slash 命令联动** — `/test` 命令自动触发 `Test` Profile

---

## 六、与现有架构的关系

```
                      ┌──────────────────────┐
                      │   SubAgentRuntime     │
                      │  (src/subagent_runtime)│
                      └──────────┬───────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
         ┌────┴────┐      ┌─────┴─────┐      ┌─────┴─────┐
         │ General │      │  Explore  │      │  Review   │
         │ P0 ✅   │      │  P0 ✅    │      │  P0 ✅    │
         └─────────┘      └───────────┘      └─────┬─────┘
                                                    │
                              ┌─────────────────────┼─────────────────────┐
                              │                     │                     │
                         ┌────┴────┐          ┌─────┴─────┐         ┌────┴────┐
                         │  Test   │          │   Debug   │         │ Security│
                         │ P0 🔴   │          │  P0 🔴    │         │ P0 🔴   │
                         └─────────┘          └───────────┘         └─────────┘

                              ┌───────────────┬───────────────┬───────────────┐
                         ┌────┴────┐    ┌─────┴─────┐   ┌─────┴─────┐   ┌────┴────┐
                         │  Doc    │    │ Migration │   │Architect  │   │ Deploy  │
                         │ P1 🟡   │    │  P1 🟡    │   │  P1 🟡    │   │ P2 🟢   │
                         └─────────┘    └───────────┘   └───────────┘   └─────────┘
```

---

## 七、实施建议

### 第一步：Profile 枚举扩展（最小改动）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentProfile {
    General,
    Explore,
    Review,
    Test,           // 新增
    Debug,          // 新增
    SecurityReview, // 新增
    Doc,            // 延迟
    Migration,      // 延迟
    Architecture,   // 延迟
    Deploy,         // 延迟
    Research,       // 延迟
    Init,           // 延迟
}
```

### 第二步：工具集可配置化

```rust
impl SubAgentProfile {
    fn allowed_categories(self) -> &'static [&'static str] {
        match self {
            Self::General => &["filesystem.read", "guidance.read", "memory.read", "process.read"],
            Self::Explore => &["filesystem.read", "guidance.read"],
            Self::Review => &["filesystem.read", "guidance.read", "memory.read"],
            Self::Test => &["filesystem.*", "process.*", "guidance.read", "memory.read"],
            Self::Debug => &["filesystem.read", "process.*", "lsp.*", "ast.*"],
            Self::SecurityReview => &["filesystem.read", "guidance.read", "memory.read", "git.*"],
            // ... 后续扩展
        }
    }
    
    fn max_turns(self) -> usize {
        match self {
            Self::General => 4,
            Self::Explore => 4,
            Self::Review => 4,
            Self::Test => 8,    // 测试需要更多轮次
            Self::Debug => 8,
            Self::SecurityReview => 6,
            // ...
        }
    }
}
```

---

## 八、最终建议

**当前最优先补充的 3 个 Agent：**

1. **Test Agent** — 开发过程最刚需，每次变更都需要测试验证
2. **Debug Agent** — 开发中最高频的"找 Bug"场景
3. **Security Review Agent** — 安全审查是独立且不可替代的维度

这三者实现后，再逐步补充 Doc、Migration、Architecture 等 Agent。