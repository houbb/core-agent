# P5 Planning Runtime — Post-Implementation Review

> 日期：2026-07-18  
> 结论：通过

## 目标达成

- 独立 `core-agent-plan` 已实现 Goal → Plan → Task → Step → Action，Intent 以共享 ID + 嵌入值进入 Goal，未突破 SQLite 五表边界。
- PlanningManager 提供 Goal/Plan 创建、更新、取消、恢复、快照与查询；Rule Builder 可无 Model 端到端运行，Builder/Reviewer/Policy/Interceptor/Observer 等均可替换。
- Planning Graph 严格覆盖完整层级、依赖引用、无环和精确边集合；P5 不包含 Scheduler 或 Tool 执行。
- SQLite 对 Goal/Plan 使用事务内 CAS，对 Plan 变更原子保存旧版本快照；五表均有审计列、注释、索引且无外键。
- 根组合层只把可用 Workspace 与启用 Tool 转为有界 PlanningContext，不反向污染底层 Runtime 依赖。

## 三轮 Review

1. 架构 review：删除抢跑 P6 的依赖排序实现；固定根组合层依赖方向、双阶段 Review 生命周期、Intent/五表边界和精确 Graph 合同。
2. 正确性与安全 review：补事务 CAS、Snapshot 不可变、Plan 身份锁定、递归敏感键/深度/体积限制、URI 凭据拒绝、生成后 Policy 与 Tool Context 交叉校验。
3. QA/Testability review：修复 Goal Interceptor 身份重定向、Goal/Context Workspace/Session 串用、错误 capability、不可用 Workspace adapter，并将高风险发现转换为 E2E 断言。

## 验证证据

- P5：11 个单元断言、10 个 Runtime E2E、1 个根跨 Runtime E2E。
- 静态检查：`cargo clippy -p core-agent-plan --all-targets -- -D warnings` 通过。
- 格式与差异：`cargo fmt --all -- --check`、`git diff --check` 通过。
- 全工作区：统一回归通过；根 crate 仍只有此前存在的 8 个 ambiguous glob re-export warnings。

## 剩余风险

- 同步 SQLite 调用可能阻塞 async worker；Observer 内部阶段粒度仍偏粗。
- 恢复计划不携带最新 Tool Catalog；P6 执行前必须重新完成 Tool 存在性、能力和权限检查。
- LLM Planning、调度、并行、自动重规划与人类审批不属于 P5。

## Verdict

P5 满足 `006-planning.md` 的 MVP 边界，可作为 P6 Execution Runtime 的稳定输入合同。
