# Unknowns Report

## Metadata

- **Task / Feature:** P4 Workspace Runtime
- **Mode:** Standard
- **Date:** 2026-07-18
- **Prepared by:** Codex
- **Scope:** Local Workspace 生命周期、资源/项目/环境发现、索引、快照恢复、SQLite 持久化与既有 Context 集成边界

## Intent

### User-visible problem

Agent 目前具备 Session、Context、Model 和 Tool Runtime，但没有统一的工作环境抽象；文件、项目、环境和快照若直接进入 Tool 或 Agent Loop，会形成跨模块耦合。

### Desired behavior change

调用方可以通过一个稳定的 WorkspaceManager 打开本地工作区，发现项目、环境和资源，构建可搜索的 Workspace Graph，创建/恢复快照，并在关闭和进程重启后从 SQLite 恢复元数据。

### Affected users and workflows

- Runtime 开发者：通过 Provider、Scanner、Detector、Indexer、Snapshotter 扩展 Workspace。
- Agent/Context 调用方：读取结构化 Workspace 与 Environment，而不直接扫描路径。
- 本地 Coding Agent：执行 Open → Detect → Index → Ready → Snapshot/Restore → Close。

### Success criteria

- 独立 `core-agent-workspace` crate 可由根 `core-agent` 使用。
- Local Provider 只接受合法本地目录，资源扫描不跟随符号链接且有数量/深度边界。
- Workspace 具有可验证状态机，并支持 open/list/find/reload/mark_modified/snapshot/restore/close。
- Rust、Java/Maven/Gradle、Node、Python 项目与基础环境可确定性发现。
- 文件和项目进入可搜索的基础 Workspace Graph。
- SQLite 五张表符合审计列、注释、索引、无外键规范，并可严格恢复。
- 单元断言、端到端与全工作区回归通过。

### Non-goals

- Git Diff/Commit 引擎、Terminal 执行、AST/LSP/Symbol Index、Embedding/RAG。
- Remote/SSH/Docker/Cloud Workspace、多 Workspace 调度与同步。
- P8 企业权限/RBAC；P4 只提供 Policy 扩展点。
- 将文件操作硬编码为 P3 Tool；本 P 冻结 Resource 能力接口，由后续适配器接入 Tool。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | P4 是独立 Workspace Runtime，位于 Tool 与 Planning 之间 | High |
| Design reference | `design-docs/005-worksapce.md` | 九个组件、生命周期、五张表、Graph 与 MVP 排除项 | High |
| Code | `Cargo.toml`, `src/lib.rs` | 当前为多 crate Rust workspace，根 crate 负责组合导出 | High |
| Code | `core-agent-tool` | P3 有意未实现 Filesystem/Terminal/Git；扩展点使用 trait + manager 编排 | High |
| Code | `core-agent-context/src/domain/workspace_context.rs` | P1 WorkspaceContext 明确等待 P4 填充 | High |
| Schema | `core-agent-session`、`core-agent-tool` SQLite schema | 审计字段、逻辑索引、无外键是既有持久化约定 | High |
| Tests | 既有各 Runtime `tests/*_e2e.rs` | 每个 P 采用 crate 单元测试 + 独立 E2E | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Workspace 不是路径，而是 Agent 的 Operating Environment | P4 设计 | 核心对象以 identity/provider/URI/state/graph 表达，不暴露 path 作为身份 |
| P4 首版必须包含 Snapshot 和 Index 扩展点 | P4 设计 | 不能只做目录扫描 |
| P4 SQLite 建议正好五张表 | P4 设计 | 本次实现 `workspace/project/resource/environment/workspace_snapshot` |
| P3 未越界实现真实 Filesystem/Terminal/Git | P3 代码与 CHANGELOG | P4 可独立定义 Resource，不反向耦合 Tool |
| Context 现有 WorkspaceContext 是占位类型 | P1 代码 | 根 crate 适配器可打通，P4 crate 保持独立 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| Snapshot restore 是否应删除快照后新增文件 | Known unknown | 精确回滚有数据删除风险，Permission Runtime 尚未实现 | 5 | 4 | 5 | 4 | 80 | Decision | P4 采用非破坏性 overlay restore：恢复已有/缺失快照文件，不删除后来新增文件 |
| Workspace 身份应使用 path 还是 URI | Known unknown | 设计明确“不要 path”，未来 Provider 不只本地 | 5 | 5 | 4 | 5 | 100 | Decision | 对外冻结规范化 URI；Local Provider 内部才转换 canonical path |
| Workspace Graph 首版粒度 | Known unknown | 完整 Module/Package/Symbol/Git 图超出 MVP | 4 | 5 | 3 | 3 | 60 | Decision | 首版冻结通用 Node/Edge 合同，只生成 Workspace/Project/Environment/Resource 节点 |
| 是否让 P4 依赖 Context 或 Tool | Unknown known | 直接依赖会造成 Runtime 方向反转 | 4 | 4 | 4 | 4 | 64 | Decision | P4 crate 独立；根组合 crate 提供 Workspace → Context 适配器 |
| 环境变量是否保存值 | Unknown unknown candidate | 环境值常含凭据，设计只要求 Environment/Variables | 5 | 4 | 5 | 5 | 100 | Decision | 仅记录少量变量名称，不读取/持久化值 |
| 扫描超大仓库/符号链接的行为 | Unknown unknown candidate | 无边界递归可导致逃逸、循环或内存耗尽 | 5 | 4 | 3 | 4 | 80 | Decision | 不跟随符号链接，忽略常见构建目录，设置最大深度和资源数并明确报错 |
| 持久化资源刷新如何避免陈旧行 | Known unknown | reload 后资源可能删除或类型变化 | 4 | 4 | 2 | 3 | 48 | Decision | 单事务替换该 Workspace 的 project/resource/environment 行，Workspace/Snapshot 保持稳定 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| open 应可重复且保留 Workspace identity | 生命周期要求恢复/关闭后再打开 | Existing pattern + E2E |
| 列表和搜索顺序应稳定 | Agent 上下文和测试需要可复现 | Tests |
| Provider/Scanner/Detector 可替换 | 设计列出 Local/Remote/Docker 等演进 | Trait contract |
| 观察器失败不能破坏成功的工作区操作 | 既有 Model/Tool Runtime 已采用审计隔离 | Existing pattern + Tests |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| Windows file URI 与盘符规范化 | 当前工作区运行在 Windows | Local Provider round-trip unit/E2E |
| 非 UTF-8 文件名 | Rust Path 可表示但 URI 转换可能失败 | 明确返回 UnsupportedPath，不静默丢失 |
| Snapshot 根目录落入 Workspace | 会递归复制自身 | canonical 边界检查与测试 |
| SQLite 损坏枚举/JSON | 静默回退会污染 Agent 世界模型 | corrupt-row test |

## Decisions Required

当前没有需要阻断实现的用户决策。高影响项均由设计文档或安全保守原则确定，且保持后续可扩展。

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| 本地资源能否完整经历 open/reload/index | E2E | 新增/删除资源在 reload 后准确反映 | Low | Implementation |
| Snapshot overlay 能否恢复修改并保留新增文件 | E2E | 快照文件恢复、新文件不删除 | Low | Implementation |
| SQLite 是否可跨 Runtime 实例恢复 | E2E | close/reopen 后 identity 与结构不变 | Low | Implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 默认 Local 扫描忽略 `.git`、`target`、`node_modules`、`.core-agent` | 都是高体积/内部目录，不影响基础项目识别 | 通过可配置 ScanOptions 调整 |
| 环境检测使用文件和进程环境推断，不执行外部命令 | 避免超时、副作用和机器差异 | 后续增加独立命令型 Detector |
| 默认 Snapshot 存储在系统临时目录 | 不污染工作区且可由 Builder 替换 | 注入持久化/云 Snapshotter |
| search 首版为规范化子串匹配 | 可预测且接口未来可替换 | 替换 WorkspaceIndexer 实现 |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| 精确回滚/删除语义 | 等 P8 Permission 与 P6 Execution rollback | 保留 RestoreMode 扩展空间 |
| Git branch/commit/diff graph | P4 MVP 明确不做 Git Diff Engine | P4.x Git Index |
| 增量索引与超大 monorepo 性能 | P4.7 路线项 | Indexer 可替换、记录资源数 |
| Remote URI credential/host-key 策略 | P4 MVP 明确排除 Remote | 新 Provider 前重新 Unknowns Discovery |

## Recommended Implementation Boundary

### Implement now

- Domain：Workspace/Project/Resource/Environment/Snapshot/Graph 与严格校验。
- Runtime：Manager、Registry、Provider、Project/Resource/Environment Manager、Lifecycle。
- Local：目录加载、受限资源扫描、项目与环境发现、基础索引、overlay 快照。
- Persistence：SQLite 五表、事务刷新、严格读取。
- Integration：根 crate 导出与 Workspace → Context adapter。

### Do not implement now

- 实际 Shell/Git 写操作、工具适配、远程连接、符号/向量索引、UI。
- 自动删除用户文件的 restore。
- 捕获环境变量值或文件正文到 SQLite。

### Interfaces or data contracts to freeze

- Workspace identity = UUID + provider key + canonical URI。
- Resource identity = UUID + workspace ID + URI + type + capabilities。
- Graph 使用通用 Node/Edge，不绑定 AST/LSP。
- Provider/Scanner/Detector/Snapshotter/Indexer/Observer/Policy/Interceptor traits。

### Areas that must remain reversible

- 扫描忽略规则和上限。
- 项目识别 marker。
- 默认 Snapshot 存储实现。
- search 排序与打分实现。

## Verification Plan

### Automated

- Unit tests：状态机、校验、扫描、检测、索引、Registry、Snapshot、SQLite schema/round-trip/corruption。
- Integration tests：完整 open/reload/snapshot/restore/close/reopen/Context adapter。
- Migration tests：P4 为新表，无旧 schema；验证幂等建表及审计列。
- Contract tests：扩展 trait 注入、Policy/Interceptor/Observer 隔离。
- Static analysis：fmt、clippy warnings-as-errors（P4 crate）、全工作区 tests。

### Manual

- Happy path：临时 Rust 项目打开并 Ready。
- Empty state：空目录仍形成 Workspace/Environment/Graph。
- Failure path：无效 URI、文件路径、资源上限明确失败。
- Recovery path：Snapshot overlay 与 SQLite 重开。
- Permission boundaries：Policy deny；不跟随符号链接、不记录变量值。
- Mobile / responsive：不适用（P4 无 UI）。
- Accessibility：不适用（P4 无 UI）。
- Performance：受 max_depth/max_resources 限制，避免无界扫描。

### Observability

- Logs：P4 不绑定日志框架。
- Metrics：Observer 接收阶段、状态和资源计数。
- Alerts：由未来 Observation Runtime 实现。
- Audit trail：五表审计列 + Snapshot 元数据。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file
