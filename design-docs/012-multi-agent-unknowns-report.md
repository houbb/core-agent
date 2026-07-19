# P11 Multi-Agent Runtime Unknowns Report

## Scope

- **Known knowns**：当前已有可持久化的单 Agent Runtime；P11 只负责 Organization/Team/Role/Member、Agent 路由、协作分派、通信治理，不重新实现 Planning 或 Execution。
- **MVP 边界**：按演进表实现 P11.0 Team Runtime，并保留后续 Role/Collaboration/Capability Routing 的稳定契约；不实现 Swarm、投票、协商、动态组队、跨机器或自治组织。
- **持久化边界**：采用文档指定的五张表 `organization/team/agent_member/role/collaboration`，协议消息作为 Collaboration 的有界 transcript 持久化，不增加第六张表。

## Material unknowns and decisions

| 优先级 | 未知项 | 决策 | 依据/风险控制 |
|---|---|---|---|
| P0 | Multi-Agent 是否直接依赖 Agent Runtime | crate 仅定义 `AgentDirectory`/`AgentDispatcher` 端口，根组合层适配 `AgentManager` | 保持 Runtime 依赖方向，避免循环和职责泄漏 |
| P0 | 分派崩溃后是否重复创建 Agent 任务 | 使用稳定 dispatch ID + 可持久化 binding；先持久化 binding，再调用 dispatcher | 冷恢复复用 binding；未知结果不盲目改派 |
| P0 | Router 如何选择 Agent | P11.0 使用确定性规则：角色匹配、能力全包含、可用状态、member ID 稳定排序 | 可审计、可复现；AI/负载路由留后续 |
| P0 | Agent 生命周期由谁管理 | Agent Runtime 独占 Agent 生命周期；P11 只维护成员协作状态 | 防止两个 Runtime 竞争写 Agent |
| P1 | Organization/Role 是否属于 P11.0 | 实现最小强层级与版本化声明，因为 Team/Member 所有权和五表设计依赖它们 | 不实现企业组织树或跨组织协作 |
| P1 | 通信协议实现程度 | 实现有界、类型化 `AgentMessage` 信封（source/target/correlation/intent/payload/context refs/priority），嵌入 Collaboration | 支持审计与未来远程协议，不实现签名/网络传输 |
| P1 | Shared Workspace/Memory | 仅保存可选引用，不读取或写入 Workspace/Memory | 对应 P11.4/P11.5，避免越界 |
| P1 | 跨 Store 原子性 | 明确采用 at-least-once + dispatcher 幂等契约，不引入分布式事务 | 记录残余风险并以 OutcomeUnknown 保守恢复 |
| P2 | UI | 本 P 只实现 Runtime；可视化在 020+ 文档处理 | 保持本 P 单一职责 |

## Acceptance boundary

- 能创建 Organization、Role、Team，加入/离开 Agent member，并严格校验归属关系。
- 能按角色/能力确定性选择可用成员，创建 Collaboration，持久化 binding 后分派。
- Waiting/未知结果可冷恢复且复用同一 dispatch/binding；支持显式 handover，不静默改派。
- SQLite 恰好五张表，含审计字段、注释、索引、无外键，并严格校验结构化列与 JSON。
- 提供单元断言、Runtime E2E 和根组合层 Agent Runtime 集成 E2E。

## Residual risks

- Dispatcher 完成后 Collaboration 提交失败时仍是 at-least-once；下游必须按 dispatch ID 幂等或支持查询。
- P11.0 未实现容量感知、租约、心跳、远程 transport、消息签名和跨团队事务。
