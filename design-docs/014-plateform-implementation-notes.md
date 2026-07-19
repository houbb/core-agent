# P13 Platform Runtime 实现说明

## 范围

实现 P13.0 企业治理最小闭环：Tenant、Organization、默认拒绝 Policy、原子 Quota、不可变 Audit，以及 Health/Metrics 扩展契约。Billing、Secret Vault、Kubernetes、HA、Cluster 与多 Region 留给后续阶段。

## 架构

- 新增独立 `core-agent-platform`，不反向依赖业务 Runtime。
- `PlatformManager` 统一管理运行状态、租户目录、组织目录、策略、配额、审计、健康检查和指标上报。
- `GovernanceRequest` 必须显式携带 `tenant_id`；组织归属、策略范围、配额范围和审计范围均在读写时校验。
- 确定性 Policy Engine 按优先级降序匹配；同优先级 Deny 优先；未命中 Allow 时默认拒绝。
- Quota 通过版本 CAS、请求 ID 有界幂等账本和 Audit 单事务提交避免重复扣量。
- 根组合层提供 `PlatformToolPolicy` 与 `ToolGovernanceResolver`，把真实 Tool 执行接入企业治理，治理故障一律 fail-closed。

## SQLite

严格使用五张表：`tenant`、`organization`、`policy`、`audit`、`quota`。所有表包含审计字段、注释、索引且无外键；配额通过表达式唯一索引覆盖 `organization_id IS NULL` 的租户级范围。冷读会交叉校验结构化列、JSON 内容、归属、版本、时间和操作人。

## 安全与恢复

- Suspended/Archived Tenant 禁止新的治理操作。
- Audit 不持久化 Prompt、Tool 参数或 Secret，只记录有界身份、决策、命中规则、配额与允许列表属性。
- Observer panic 与业务决策隔离；Interceptor 不得改变请求身份。
- 相同请求 ID 直接返回既有 Audit 决策，不重复扣减配额。
- 单节点 SQLite 采用事务和乐观版本控制；分布式配额不在 P13.0 范围。

## 测试覆盖

- 单元断言：策略显式匹配、伪造配额账本、敏感元数据拒绝。
- Runtime E2E：默认拒绝审计、允许与幂等扣量、越限不扣量、租户隔离、Suspended fail-closed、Observer 隔离、SQLite 五表/重开/篡改。
- 跨 Runtime E2E：Platform Policy/Quota → 真实 Tool Runtime，第二次调用被配额拦截且 Tool 不执行。

测试命令按用户要求，在所有剩余 P 实现完成后统一运行。

## 已知边界

- P13.0 不提供身份认证、密钥管理、KMS、法律留存或分布式一致性配额。
- Organization 是 Platform 治理目录，与 P11 协作组织保持独立，通过组合层映射。
