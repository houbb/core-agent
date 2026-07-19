# Post-Implementation Review

## Metadata

- **Task / Feature:** P4 Workspace Runtime
- **Date completed:** 2026-07-18
- **Reviewer:** Codex + QA Testability Review
- **Related Unknowns Report:** `design-docs/005-worksapce-unknowns-report.md`
- **Related implementation notes:** `design-docs/005-worksapce-implementation-notes.md`
- **Related PR / commit:** Working tree（未提交）

## Behavior Changes

### Before

- Agent Runtime 没有统一的外部世界模型；P1 WorkspaceContext 只是占位。
- P3 Tool 可以描述能力，但真实文件、项目、环境、索引和快照没有所属 Runtime。

### After

- 调用方可通过 WorkspaceManager 完成 open/list/find/reload/mark_modified/search/snapshot/restore/close。
- Local Workspace 自动发现资源、项目和环境，构建可持久化、可搜索的基础 Graph。
- Workspace 元数据可跨 SQLite Runtime 冷恢复，并通过根 adapter 安全进入 Context。
- Snapshot restore 为非破坏性 overlay；提交失败执行 Catalog + 文件补偿清理。

## Files and Systems Affected

| Area | Change | Why it changed |
|---|---|---|
| `core-agent-workspace/domain` | 新增 Workspace/Project/Resource/Environment/Snapshot/Graph | 冻结 P4 世界模型和不变量 |
| `application` | 新增 WorkspaceManager 及三个子 Manager | 提供唯一生命周期入口和编排 |
| `infrastructure/providers` | 新增扩展 traits、Registry、Local 实现 | 支持未来 Provider/Scanner/Detector/Indexer 替换 |
| `persistence` | 新增 SQLite 五表和严格 aggregate 恢复 | 持久化 Workspace 世界模型且符合 DB 规范 |
| 根 `core-agent` | 新增 P4 导出和 Context adapter | 打通 P1，但保持 P4 独立 |
| tests | 新增单元、Runtime E2E、跨 Runtime E2E | 验证行为、失败边界和恢复 |

## Assumptions Review

| Assumption | Status | Evidence | Action |
|---|---|---|---|
| 忽略 `.git/target/node_modules/.core-agent` | Confirmed | 扫描测试与 Git 环境检测测试 | Keep，ScanOptions 可覆盖 |
| 环境检测不执行外部命令 | Confirmed | Detector 只读 OS/Shell、marker、扩展名 | Keep，未来注入命令型 Detector |
| 默认 Snapshot 位于系统临时目录 | Confirmed | 默认实现与可注入 snapshot root E2E | Monitor，长期恢复应注入持久实现 |
| search 使用确定性子串匹配 | Confirmed | Graph search E2E | Keep，可替换 Indexer |

## Unknowns Review

### Resolved

| Unknown | Resolution | Evidence |
|---|---|---|
| Workspace 身份 | UUID + provider key + canonical credential-free URI | Local cold reopen 保留 ID |
| Graph 首版粒度 | Workspace/Project/Environment/Resource Node/Edge | Graph 数量、类型、搜索与严格恢复测试 |
| P1/P4 依赖方向 | 根组合层 adapter，P4 零 Runtime 依赖 | cargo tree + 跨 Runtime E2E |
| Snapshot 删除语义 | overlay restore，保留新增文件 | Snapshot E2E |
| 扫描逃逸/无界 | 不跟随符号链接；数量/深度明确失败 | limit 与安全测试 |

### Remaining

| Unknown | Risk | Follow-up |
|---|---|---|
| 同步扫描/复制/SQLite 在高并发 Agent Loop 中的延迟 | 中 | P4.x/P6 使用 blocking executor 或专用 IO service |
| Restore 后 refresh/catalog 失败的跨资源域原子回滚 | 中高 | P6 Execution checkpoint/rollback；当前可重试 reload/restore |
| Observer 失败事件、阶段和 artifact correlation | 中 | P10 Observation Runtime |
| 长期 Snapshot 加密、保留、清理 | 中 | 持久 Snapshot Provider/企业策略 |

### Newly discovered

| Unknown | Impact | Recommended action |
|---|---|---|
| 结构列与 JSON aggregate 可能发生双写漂移 | 高 | 已在冷读时严格交叉校验并测试 |
| Snapshot metadata 失败会遗留外部文件 | 高 | 已加入 discard/remove_snapshot 补偿并测试 |
| max_depth 原实现会静默截断 | 中 | 已改为明确 LimitExceeded 并测试 |

## Deviations

| Deviation | Reason | User-visible effect | Risk | Approved |
|---|---|---|---|---|
| Restore 默认 overlay | P8 尚无删除授权 | 新增文件不会被删除 | 低 | Yes |
| Snapshot 增加补偿删除合同 | 文件系统与 SQLite 无共享事务 | 提交失败不遗留 Snapshot | 低 | Yes |
| Graph MVP 不含 Module/Symbol/Git | 设计明确排除 AST/Symbol/Git Diff | 首版只提供基础节点 | 低 | Yes |

## Verification Evidence

### Automated checks

- [x] Unit tests：P4 14 passed
- [x] Integration tests：P4 13 passed + root adapter 1 passed
- [x] Migration tests：新 schema 幂等建表、审计列/索引/无外键检查
- [x] Contract tests：Policy、Interceptor、Observer、Provider/Catalog/Snapshot 注入
- [x] Static analysis：P4 Clippy `-D warnings`
- [x] Build：全工作区测试构建
- [x] Lint：`cargo fmt --check`、`git diff --check`
- [x] Type check：Rust 编译与 Clippy

### Manual checks

- [x] Happy path：Rust Workspace → Ready → search
- [x] Empty state：空目录 → Generic Project
- [x] Failure path：无效凭据 URI、Policy deny、数量/深度上限、损坏 DB
- [x] Recovery path：SQLite 冷读、close/reopen、Snapshot overlay
- [x] Permission boundaries：Policy 扩展；不跟随符号链接；不保存变量值
- [x] Mobile / responsive：不适用（无 UI）
- [x] Accessibility：不适用（无 UI）
- [x] Performance：边界已验证；高并发 blocking 行为延期

### Production or runtime evidence

- Logs: P4 不绑定日志框架。
- Metrics: WorkspaceObservation 提供操作、状态和资源/项目计数。
- Screenshots: 不适用。
- Traces: 延期到 P10。
- User validation: 等后续 Planning/Execution Runtime 消费。

## Rollback and Recovery

- **Rollback trigger:** P4 导致 P0–P3 回归、SQLite 严格恢复错误或 Local Workspace 越界写入。
- **Code rollback steps:** 移除 workspace member/dependency、根导出/adapter 与 `core-agent-workspace`。
- **Data rollback steps:** 删除 P4 独立五表；不影响 P0–P3 表。
- **Configuration rollback steps:** 移除自定义 Workspace 扩展注入。
- **Recovery verification:** 运行全工作区测试；用户 Workspace 文件不因代码回滚删除。

## Maintainer Notes

- Workspace identity 永远是 URI，不要把 public path 重新变成 aggregate identity。
- READY 以后 Graph 必须与 Project/Environment/Resource identity + kind 完全一致。
- Local restore 必须继续拒绝符号链接目标，且默认不得删除新增文件。
- SQLite 的结构列和 JSON content 是双写合同；新增字段必须同步保存、恢复交叉校验和损坏数据测试。
- 自定义 Snapshot/Catalog 必须实现补偿删除；不要在状态校验前执行外部 IO。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill

## Understanding Check

1. 用户现在可以打开本地 Workspace，发现/搜索/快照/恢复/关闭并持久化。
2. 根 Runtime adapter 是旧 Context 路径进入 P4 的入口；P3 Tool 尚不直接执行 Resource。
3. 最可能失败：超大扫描达到限制、Snapshot 存储被清理、Restore 后 refresh/catalog 失败需重试。
4. 本地生命周期/恢复/边界均有测试；高并发 blocking 性能仍是明确假设和后续项。
5. 代码和五张独立表可移除；overlay 不删除用户新增文件；Snapshot 提交失败执行补偿。
6. 六个月后先查 `application/manager.rs`、`providers/local.rs`、`persistence/store.rs` 及 E2E。
7. canonical URI、Graph Node ID、SQLite 双写和 Snapshot provider 是未来最需兼容的合同。
