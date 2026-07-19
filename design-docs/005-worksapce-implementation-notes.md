# Implementation Notes

## Metadata

- **Task / Feature:** P4 Workspace Runtime
- **Date started:** 2026-07-18
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/005-worksapce-unknowns-report.md`
- **Related plan / issue / PR:** `design-docs/005-worksapce.md`

## Confirmed Discoveries

### Discovery D-001

- **What was discovered:** P3 明确没有实现真实 Filesystem/Terminal/Git。
- **Evidence:** `core-agent-tool` 只提供通用 Tool/Provider/Executor；P3 CHANGELOG 将这些能力推迟到 P4。
- **Why it matters:** Workspace 应定义结构化资源和环境边界，不把 IO 塞回 Tool Runtime。
- **Affected scope:** P4 crate 依赖方向、根 crate 集成。
- **Action taken:** 设计为独立 crate，后续 Tool 通过 adapter 使用 Resource API。

### Discovery D-002

- **What was discovered:** P1 的 WorkspaceContext 是等待 P4 填充的占位结构。
- **Evidence:** `core-agent-context/src/domain/workspace_context.rs` 注释与字段。
- **Why it matters:** P4 应与现有 Context 打通，但不能让低层 Runtime 反向依赖 Context。
- **Affected scope:** 根 `core-agent`。
- **Action taken:** 在根组合层提供转换函数，P4 crate 不依赖 P1。

### Discovery D-003

- **What was discovered:** 所有既有 SQLite Runtime 都执行审计列、逻辑索引和无外键约定。
- **Evidence:** Session/Tool schema 与 AGENTS.md。
- **Why it matters:** P4 五表必须从首版一致执行。
- **Affected scope:** P4 persistence。
- **Action taken:** 五表均包含固定审计字段、SQL 注释和查询索引。

### Discovery D-004

- **What was discovered:** `WorkspaceContext` 可以在根组合层直接由 P4 aggregate 填充，无需 P4 反向依赖 P1。
- **Evidence:** 根 `integrations::workspace_context/environment_context` E2E 使用真实 Local Workspace 成功构建 Context。
- **Why it matters:** 既打通既有模块，又保持 Runtime 依赖图无环。
- **Affected scope:** 根 `core-agent` crate。
- **Action taken:** 只传递项目、环境、资源计数和 Graph，不传文件正文或环境变量值。

### Discovery D-005

- **What was discovered:** Snapshot 文件创建与 Catalog/Registry 提交跨越两个资源域，失败时不能依赖数据库事务自动回滚。
- **Evidence:** 三轮 review 与注入 Catalog 失败 E2E。
- **Why it matters:** 否则提交失败会遗留快照目录或 Snapshot 元数据。
- **Affected scope:** WorkspaceSnapshot、WorkspaceCatalog、WorkspaceManager。
- **Action taken:** 新增 `discard/remove_snapshot` 补偿合同；非法状态在 IO 前校验，失败时同时尝试清理 Catalog 与文件。

## Decisions

### Decision DEC-001

- **Decision:** Workspace 对外使用 canonical URI，而不是 path 作为身份。
- **Alternatives considered:** 裸路径；URI；Provider 私有句柄。
- **Reason:** 符合设计“Workspace 不等于目录”，并为 SSH/Docker/Cloud 保留协议空间。
- **Evidence:** P4 设计的 Workspace/Resource 对象和 Provider 演进路线。
- **Owner / approver:** Architecture（由设计文档确认）
- **Reversibility:** High；内部 Local Provider 可自由改变路径处理。
- **Follow-up:** Remote Provider 开发前定义 URI credential 规则。

### Decision DEC-002

- **Decision:** P4 restore 使用 overlay 语义，不删除快照后新增文件。
- **Alternatives considered:** 精确镜像恢复；overlay；只生成 diff。
- **Reason:** P8 Permission 和 P6 rollback 尚未存在，默认删除用户文件不可接受。
- **Evidence:** P4 MVP 需要 restore，但没有授权/审批契约。
- **Owner / approver:** Security/Architecture
- **Reversibility:** Medium；未来可新增显式 Exact 模式。
- **Follow-up:** P6/P8 实现时重新评估精确恢复。

### Decision DEC-003

- **Decision:** SQLite 只保存资源元数据和 Graph，不保存文件正文；正文仅存在可替换 Snapshotter。
- **Alternatives considered:** 文件正文 BLOB；外部快照；仅 manifest。
- **Reason:** 控制数据库体积、减少敏感数据扩散，并隔离 Snapshot 存储。
- **Evidence:** Workspace 五表边界与企业安全预期。
- **Owner / approver:** Architecture/Security
- **Reversibility:** High。
- **Follow-up:** Cloud Snapshot Provider 自行实现加密和保留策略。

### Decision DEC-004

- **Decision:** READY/MODIFIED/SNAPSHOT/CLOSED aggregate 的 Graph 节点必须与 Workspace/Project/Environment/Resource identity 和类型完全一致。
- **Alternatives considered:** Graph 仅做弱引用；只校验边存在；严格 aggregate invariant。
- **Reason:** SQLite 不使用外键，Graph 是 Agent 的世界模型，缺行、错挂或类型篡改不能静默进入 Context。
- **Evidence:** 冷读、结构列损坏和缺 Environment 测试。
- **Owner / approver:** Architecture
- **Reversibility:** Medium；未来增加 Module/Symbol 时扩展 NodeKind 和 invariant 即可。
- **Follow-up:** P4.x 增加新图节点时同步更新持久化和测试。

## Assumptions

### Assumption A-001

- **Assumption:** Local 扫描默认忽略 `.git`、`target`、`node_modules` 和 `.core-agent`。
- **Why it is currently acceptable:** 避免常见高体积与内部目录，项目 marker 仍可在根/模块目录发现。
- **Risk:** 调用方可能需要索引生成文件。
- **How it will be validated:** E2E 验证忽略项不进入 Resource，但普通嵌套文件进入。
- **Reversal plan:** 通过 `ScanOptions` 自定义忽略集合。

### Assumption A-002

- **Assumption:** 默认环境检测不执行 `git --version`、`java -version` 等外部命令。
- **Why it is currently acceptable:** 文件 marker 和 OS/Shell 足以满足 P4.0~P4.2 的基础发现，且不会引入命令副作用。
- **Risk:** 已安装但项目未使用的 Runtime 不会被发现。
- **How it will be validated:** 项目类型与环境贡献一致性测试。
- **Reversal plan:** 注入命令型 `EnvironmentDetector`。

## Deviations

### Deviation DEV-001

- **Original plan:** Local Snapshot create/restore。
- **Actual implementation:** 额外实现 discard 与 Manager 失败补偿；restore 采用 overlay，不执行删除。
- **Reason for deviation:** 文件系统与 Catalog 无法共享事务，且 P8 Permission 尚未提供破坏性删除授权。
- **User-visible effect:** Snapshot 提交失败不会遗留新快照；restore 会保留快照后新增文件。
- **Data / API effect:** `WorkspaceSnapshot::discard`、`WorkspaceCatalog::remove_snapshot` 成为扩展合同。
- **Risk introduced:** 自定义 Snapshot/Catalog 必须正确实现补偿删除。
- **Approval required:** No
- **Follow-up:** P6 Execution 提供跨步骤 rollback 后复用统一事务编排。

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| 超大仓库扫描内存占用 | 4 | max_depth/max_resources + ignore + 明确 LimitExceeded | P4 maintainer | P4.7 增量索引 |
| 系统临时目录 Snapshot 被外部清理 | 3 | Snapshotter 可注入；SQLite 记录状态 | Integrator | 需要跨进程长期 restore 时 |
| Remote Provider 安全边界未定义 | 5 | P4 不实现 Remote | Security | 新增首个 Remote Provider 前 |
| async trait 内部 Local 扫描/复制/SQLite 是同步阻塞 IO | 3 | 明确资源上限；P4 不启动后台 Agent Loop | P6/P4 maintainer | 引入并发执行或大型仓库前 |
| restore 完成文件覆盖后，后续 refresh/catalog 失败无法跨资源域原子回滚 | 4 | 目标 Snapshot 保留，可重试 reload/restore；不执行删除 | P6 Execution | 实现统一 rollback/checkpoint 时 |
| P4 Observer 只记录成功操作，不含失败阶段/artifact ID | 3 | 错误直接返回调用方 | P10 Observation | Trace/Audit Runtime 接入时 |

## Tests Added or Updated

| Test | Purpose | Result |
|---|---|---|
| P4 unit suite | Domain、Local、Snapshot、Index、SQLite（14 个） | Pass |
| `workspace_runtime_e2e` | 完整 Workspace workflow、补偿与恢复（13 个） | Pass |
| root integration test | Workspace → Context adapter（1 个） | Pass |
| full workspace regression | 防止 P0~P3 回归 | Pass |

## Rollback Notes

- Code rollback: 删除 `core-agent-workspace` workspace member/dependency、根导出与新增 crate。
- Data rollback: P4 仅新增独立五张表；删除这些表不会影响 P0~P3 数据。
- Configuration rollback: 移除自定义 Provider/Scanner/Detector/Snapshotter 注入即可回默认。
- External-system rollback: 无外部系统变更。
- Recovery validation: 回滚后执行全工作区测试；保留用户 Workspace 文件不变。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
