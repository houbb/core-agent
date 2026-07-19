# P10 Workflow Runtime — Unknowns Report

## Metadata

- **Task / Feature:** Sequential Workflow Runtime
- **Mode:** Standard
- **Date:** 2026-07-18
- **Prepared by:** Codex
- **Scope:** `core-agent-workflow`, root Execution integration, SQLite MVP

## Intent

### User-visible problem

现有 Agent、Planning、Execution、Memory 与 Event Runtime 可以独立工作，但缺少一个业务可读、可暂停和可恢复的长期流程层来持续协调多个 Execution。

### Desired behavior change

Runtime 使用者可以注册版本化的 `Workflow → Stage → Activity → Action` 定义，启动顺序实例，查看阶段进度，安全暂停、恢复、取消、快照和冷恢复，并把 Action 委托给 Execution Runtime，而不是由 Workflow 直接执行 Tool。

### Affected users and workflows

- 定义 RCA、Release、Approval、CI/CD 等流程的 Runtime 作者。
- 通过 Manager 启动、暂停、恢复、取消实例的调用方。
- 检查 Stage/Activity/Action 状态、Timeline 与 Snapshot 的运维人员。
- 在根组合层把 Workflow Action 解析成已批准 Plan 并交给 Execution Runtime 的集成方。

### Success criteria

- Workflow crate 不依赖 Tool、Model、Agent 或具体 Execution 实现。
- Definition 是强类型四层对象、不可变版本；Instance 固定引用并快照一个 Definition 版本。
- P10.0 只做确定性顺序调度，任何时刻最多一个 Action active。
- Action 在副作用前先持久化稳定 dispatch/binding；冷恢复绝不重新 prepare 一个新的 Execution。
- start/pause/resume/cancel、Waiting、Snapshot/Restore、CAS 与 Timeline 可验证。
- SQLite 严格使用设计中的五张表，具备审计字段、注释、索引、冷读交叉校验且无外键。
- 根组合 E2E 证明 Workflow 通过端口驱动真实 Execution，而不是绕过它执行 Tool。

### Non-goals

- BPMN、DAG、条件分支、并行 Stage、Cron、事件触发、人工审批、补偿/Saga、DSL、Visual Editor、分布式 Scheduler、Cluster 与 Multi Workflow。
- Workflow 内部的 Tool 执行、Execution retry/rollback 的重复实现。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | Runtime 必须独立、可插拔，并按职责组合 | High |
| Documentation | `design-docs/011-workflow.md` | 四层模型、Manager/Engine/Scheduler 等扩展点、生命周期、五表及 MVP 排除项 | High |
| Code | `core-agent-execution` public contracts | Execution 已提供 prepare/start/resume/pause/cancel、稳定 ID 与 outcome-unknown 防重放 | High |
| Code | root `integrations` module | 跨 Runtime 适配器应放在组合 crate，避免低层依赖环 | High |
| Schema pattern | 已实现 Runtime SQLite Stores | CAS、审计列、无外键、结构列/JSON 冷读交叉校验是当前工程约定 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Workflow 负责定义、调度、状态与等待，不执行 Tool | 当前 P 文档明确职责边界 | Engine 必须通过可注入端口委托副作用 |
| P10.0 是 Sequential Workflow | 企业演进表与 MVP 排除 DAG/Parallel | Scheduler 只允许定义顺序，拒绝隐藏并行 |
| Execution 支持副作用前 `prepare` | `ExecutionManager::prepare/start` | Workflow 可先持久化外部 execution ID 再启动 |
| Execution 对未知命令结果 fail-closed | `resume`/`cancel` 的 `OutcomeUnknown` | Workflow 不应盲目创建或重放另一个 Execution |
| 数据库必须正好覆盖五个设计表 | 当前 P 文档与 `AGENTS.md` | 不新增独立 action/stage 表，实例聚合存入 JSON 并以结构列校验 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| Workflow Action 如何委托 Execution 而不产生依赖环？ | Known unknown | Workflow 不应知道 Tool，根 crate 已是跨 Runtime 组合边界 | 5 | 5 | 4 | 5 | 500 | Decision | Workflow 定义 `WorkflowEngine` 两阶段端口；根组合 `ExecutionWorkflowEngine` 解析 Plan 并调用 ExecutionManager |
| 进程在 Execution 已准备或运行时崩溃，如何恢复？ | Unknown unknown candidate | 重新 prepare 会产生第二个副作用载体 | 5 | 4 | 5 | 5 | 500 | Decision | `prepare` 返回稳定 binding/execution ID，Workflow 在 `execute` 前提交；恢复只使用原 binding 调用 engine.execute/reconcile |
| Definition 更新是否影响运行中的 Instance？ | Known unknown | Marketplace/DSL 后续会频繁升级定义 | 5 | 4 | 4 | 4 | 320 | Decision | Definition 按 workflow_id + version 不可变；Instance 内嵌完整 Definition 快照 |
| Waiting 在 P10.0 表示什么？ | Known unknown | Event Trigger/Human Approval 均被延期，但生命周期要求 Waiting | 4 | 4 | 3 | 4 | 192 | Decision | Engine 可返回 Waiting；仅由显式 resume 继续，同一 binding 不变，不实现事件订阅或审批语义 |
| Workflow 是否重试或补偿失败 Action？ | Unknown known | Execution 已拥有 retry/rollback；Workflow Policy 列出 retry/compensation | 5 | 4 | 3 | 4 | 240 | Decision | P10.0 不重复实现；Execution 最终失败则 Workflow Failed，补偿留后续 |
| pause/cancel 能否中断正在执行的 Action？ | Known unknown | Manager API 要求 pause/cancel，外部副作用只能协作终止 | 5 | 4 | 4 | 4 | 320 | Decision | live control 向 Engine/Execution 传递；pause 在安全边界生效，cancel 协作传递并持久化，冷状态 fail-closed |
| P10.0 是否实现可变 Variables？ | Known unknown | 对象包含 Variables，但路线表将 Variables 放在 P10.1 | 3 | 4 | 2 | 3 | 72 | Decision | 支持启动时强类型 JSON map 快照与读取，不提供运行中表达式/写入/分支语义；VariableStore 仅预留合同 |
| Registry 和 SQLite Definition 谁是事实源？ | Unknown unknown candidate | live registry 重启会丢失，SQLite 必须可恢复 | 4 | 4 | 3 | 4 | 192 | Decision | SQLite Catalog 是 durable source，Registry 是进程内缓存；Manager 可显式 bind_existing |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Timeline 必须能解释当前停在哪个业务阶段 | UX 明确展示 Stage 状态和 Timeline | 状态记录与 Stage/Activity/Action progress E2E |
| 一个 Workflow 定义可安全升级 | Marketplace/DSL 是后续方向 | 版本不可变与运行实例快照测试 |
| 并发 start/resume 不会双执行 | 企业流程副作用昂贵 | live ownership + store CAS 冲突测试 |
| 扩展点不能篡改 identity/definition | Policy/Interceptor 为治理边界 | 拦截输出重校验和 panic 隔离测试 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| 空 Stage/Activity 造成 UI 与进度歧义 | 四层结构要求业务可读 | Domain validation unit test |
| 相同 Action ID/Key 跨层重复导致恢复错位 | 进度与 binding 按 ID 关联 | 全 Definition 唯一性校验 |
| prepare 成功但 Workflow binding 提交失败 | 已产生 durable Execution 但实例未引用 | Engine 返回 binding 后 CAS 失败 E2E；不启动副作用并报告 orphan ID |
| action 执行完成但 Workflow 结果提交失败 | 结果未知，不能换 binding | 冷恢复使用原 binding，由 Execution 查询终态 |
| Definition/Instance JSON 被单列篡改 | 五表使用 aggregate JSON | SQLite 冷读交叉校验与 tamper test |
| Snapshot 覆盖新版本实例 | 会回放副作用 | 仅允许 current-version restore，且 Running/终态不允许恢复 |

## Decisions Required

无用户阻塞决策。上述选择均是 P10.0 边界内的保守、可演进实现。

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| 两阶段 Workflow→Execution binding 是否避免直接 Tool 执行？ | Cross-Runtime E2E | binding 先落库，真实 Execution 完成后 Workflow 才推进下一 Action | Medium | Runtime implementation |
| 冷恢复是否复用原 binding？ | Fault/recovery E2E | prepare 只调用一次，resume 使用相同 external ID | Low | Runtime implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 顺序由 Stage/Activity/Action Vec 的声明顺序决定 | P10.0 明确不做 DAG | 后续 Scheduler 可在不改定义层次的情况下解释更多策略 |
| 单实例并发度固定为 1 | Sequential MVP | P10.3 扩展 Scheduler 与 progress ownership |
| Registry 在启动时显式恢复绑定 | 与 Event live handler、Tool Registry 既有模式一致 | 后续增加自动 bootstrap |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| 条件与并行的状态合并语义 | P10.2/P10.3 | 扩展 Scheduler 前增加 ADR |
| Event Trigger 消费和去重 | P10.4 | 使用 P9 stable event/delivery ID |
| Human Approval 与 Permission/RBAC | P10.5/后续 Permission | Waiting token 保持通用，不提前定义审批模型 |
| DSL/Visual Designer 版本兼容 | P10.6/P10.7 | Definition 保持强类型和版本化 |
| 分布式 lease/fencing | P10.8 | 当前 live ownership 仅进程内，SQLite CAS 防丢更新但不充当分布式锁 |

## Recommended Implementation Boundary

### Implement now

- 四层 Definition、版本化 Registry/Catalog、Instance progress、Variables snapshot、Policy、Lifecycle、Observer、Interceptor。
- Sequential Scheduler 与两阶段 WorkflowEngine binding。
- start/pause/resume/cancel/archive、Waiting、Snapshot/current-version Restore、Timeline。
- 内存与严格 SQLite 五表 Store。
- 根组合 Execution adapter 与跨 Runtime E2E。

### Do not implement now

- DAG/branch/parallel/event trigger/cron/approval/compensation/DSL/UI/distributed execution。

### Interfaces or data contracts to freeze

- Workflow → Stage → Activity → Action 层次和不可变 Definition version。
- Instance 固定 Definition 快照、稳定 action dispatch ID 与 external binding。
- Workflow 不执行 Tool；Engine adapter 负责委托 Execution。
- 五表 schema 与 CAS/timeline/snapshot 语义。

### Areas that must remain reversible

- Scheduler 策略、Engine/Plan resolver、Policy、VariableStore、Observer 与 DSL。

## Verification Plan

### Automated

- Unit tests：层次/唯一性、生命周期、Variables/敏感键、Scheduler 顺序。
- Runtime E2E：注册/版本、顺序运行、Waiting/resume、pause/cancel、失败、CAS、snapshot/restore、panic/拦截器、冷恢复。
- Persistence tests：五表审计/索引/no-FK、reopen、结构列/JSON 篡改。
- Cross-Runtime：Workflow → approved Plan → Execution 完成，并验证 binding 先于副作用持久化。
- Static analysis：严格 Clippy、format、diff、workspace regression。

### Manual

- 后端阶段不涉及 UI、responsive 或 accessibility；业务进度通过断言验证。

### Observability

- Timeline 记录每次 instance lifecycle transition。
- Observer 包含 operation、instance/definition/action ID、actor、reason 与时间。
- Action progress 保留 attempts、binding、结果/错误和时间。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file planned
