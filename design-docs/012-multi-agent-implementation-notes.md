# P11 Multi-Agent Runtime 实现说明

## 范围

实现 P11.0 Team Runtime：`Organization → Team → Role → Agent Member → Collaboration`。支持多 Agent 编组、确定性能力路由、类型化通信信封、任务分派、等待恢复、未知结果保守恢复与显式 handover。

未实现 Swarm、投票、协商、动态组队、跨团队协作、自治组织、远程 transport、Marketplace、Agent Economy 或 UI；Shared Workspace/Memory 仅保留引用。

## 架构

- 新增独立 `core-agent-multi`，不依赖 Agent/Planning/Execution。
- `MultiAgentManager` 是组织、角色、团队、成员和 Collaboration 的统一入口。
- `AgentDirectory` 只读查询 live Agent；`AgentRouter` 确定性选择满足 Role、能力、Workspace 与可用状态的 member。
- `AgentDispatcher` 使用 `prepare → persist binding → execute` 两阶段协议；稳定 dispatch ID 支持冷恢复复用。
- 根组合层提供 `RuntimeAgentDirectory`、`RuntimeAgentDispatcher` 与 `AgentAssignmentResolver`，真实委托既有 Agent Runtime，Multi-Agent 不接管 Planning/Execution。
- typed `AgentMessage` 包含 source/target/correlation/intent/payload/context reference/priority/actor；P11.0 将有界 transcript 嵌入 Collaboration。

## 生命周期与一致性

- Team：`Created → Ready → Active → Ready → Completed → Archived`。
- Member：Joined/Available/Assigned/Working/Waiting/Completed/Left，并与 current Collaboration 强一致。
- Collaboration：Assigned/Working/Waiting/Completed/Failed/Cancelled/OutcomeUnknown。
- Team + Collaboration + affected Members 使用单 Store 事务 CAS 提交。
- 并发 Team 驱动使用不覆盖的 live ownership；同一 Team 的 P11.0 Collaboration 顺序执行。
- Handover 必须显式指定可用 member，重新生成 dispatch，保留同一 correlation 和 protocol transcript。

## SQLite

严格五张表：`organization`、`team`、`agent_member`、`role`、`collaboration`。全部包含审计字段、注释、索引且无外键；冷读取交叉校验结构化列、JSON 与 Organization/Team/Role/Member 所有权。

## 测试覆盖

- 单元断言：稳定 dispatch、敏感字段、Member 状态不变量。
- Runtime E2E：确定性路由、Waiting resume、显式 handover、OutcomeUnknown、Observer panic、SQLite 五表/重开/篡改。
- 跨 Runtime E2E：Team → Agent → Planning → Execution → Tool。

按用户要求，执行命令统一留到全部剩余 P 实现后运行。

## 已知边界

- Agent execute 后 Multi-Agent 提交失败时为 at-least-once；Dispatcher/Resolver 必须按 dispatch ID 幂等或可查询。
- P11.0 Router 不感知容量、SLA、租约或心跳；仅使用确定性可用性与能力匹配。
