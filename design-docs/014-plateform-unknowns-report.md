# P13 Platform Runtime Unknowns Report

## Scope

实现 P13.0 Tenant Runtime，并提供最小可用治理闭环：Tenant/Organization 隔离、默认拒绝 Policy、原子 Quota、不可变 Audit、Platform/Health/Metrics 扩展契约。不会实现 Billing、Secret Vault、Kubernetes、HA、Cluster 或多 Region。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | 如何避免“有 Tenant 但业务数据仍串租户” | 所有 GovernanceRequest 必须显式 tenant_id；Organization/Policy/Quota/Audit 冷读写双向校验 owner，不提供隐式默认租户 |
| P0 | Policy 语义 | P13 MVP 采用确定性规则：tenant + subject/action/resource + attributes，priority 降序，同优先级 Deny 优先，未命中默认 Deny |
| P0 | Quota 并发与重复扣减 | quota CAS + request ID 有界幂等 ledger + audit 在单事务提交；超限不修改 quota 但追加 Denied audit |
| P0 | 审计敏感数据 | Audit 仅保存稳定身份、决策、单位和有界 allowlisted attributes，不保存 Prompt/Tool 参数/Secret 正文 |
| P1 | Organization 与 P11 Organization 重复 | P13 Organization 是 Tenant 治理目录，独立 crate/type；组合层显式映射，不让 P11 反向依赖 Platform |
| P1 | 跨 Runtime 接入 | 定义通用 `GovernanceRequest`；根组合层先提供 ToolPolicy adapter，其余 Runtime 后续按同一契约接入 |
| P1 | Health/Metrics | P13.0 只定义可注入 Center 和快照对象，不持久化时序指标；五表保持文档范围 |
| P1 | Tenant 状态 | Active 可治理；Suspended fail-closed；Archived 只读审计，禁止新增治理动作 |

## Acceptance

- Tenant/Organization/Policy/Quota/Audit 五实体形成严格隔离闭环。
- allow/deny/default-deny/attribute/priority 可确定性解释。
- quota 并发 CAS、幂等 consume、超限和 audit 原子性有测试。
- SQLite 严格五表、审计字段、注释、索引、无外键与篡改检测。
- 根组合层 Platform ToolPolicy E2E 证明企业治理能阻止或允许真实 Tool。

## Residual risks

- P13.0 不提供用户认证、密钥管理、加密 KMS、法律留存策略或分布式 quota。
- 单 SQLite 节点适用于 MVP；SaaS 高并发需要集中式事务存储/租约。
