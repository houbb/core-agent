# P21 Enterprise AgentOS 实现说明

## 范围

在 P13 Platform Runtime 之上实现企业 AI 治理层：外部身份引用、统一 AI 资产注册、独立审批、生产生命周期、精确成本账本，以及 Desktop Enterprise Control Plane。

## Enterprise Governance Runtime

- 新增独立 `core-agent-governance`，由 `EnterpriseGovernanceManager` 组合真实 `PlatformManager`。
- 所有 Identity/Asset/Cost 写操作先提交 P13 `GovernanceRequest`；未配置允许策略时默认拒绝并审计。
- Identity 只保存 IdP provider、external subject、组织/角色/组引用和状态，不保存密码、token 或 Secret。
- Asset Registry 统一 Agent/Model/Prompt/Workflow/Knowledge/Policy/Capability，带版本、owner、数据分类、环境、风险分和审批证据。
- 生命周期为 Draft → Reviewed → Approved → Production，可 Suspension/Resume/Retire；创建者不得自审，审批者必须是 Active 绑定主体。
- Cost Ledger 使用 `event_key` 幂等、`u64` micros、三字符货币和整数 token，不使用浮点金额，也不宣称结算或账单能力。

## Enterprise Control Plane

- Desktop 新增 Dashboard/Organization/Identity/AI Assets/Governance/Policies/Cost/Audit/Operation/Settings 十个企业中心。
- Governance Queue 支持真实 Approve/Promote/Suspend API 动作；服务端继续拥有 actor、Policy、身份和生命周期校验。
- Cost UI 使用十进制字符串转 `BigInt` 汇总，避免 JavaScript Number 损失整数精度。
- API 响应限制 2 MiB；空态不填充伪企业数据，Identity/Settings 明示 IdP、Secret、Billing 的部署边界。

## 测试覆盖

- Runtime E2E：Identity bind、资产 review、自审拒绝、独立批准、Production/Suspend、Cost 幂等和精确汇总。
- Platform 集成：无 Allow Policy 的企业写入默认拒绝并生成 Audit。
- Vue：企业数据加载、批准后刷新、被拒操作错误保留，以及全局 Enterprise 导航。
- 统一验证在所有剩余 P 完成后运行。

## 已知边界

- Governance Registry/Cost Ledger 当前为进程内实现；P13 Policy/Audit 已持久化，生产环境仍需 durable enterprise store。
- 外部 token 校验、SSO 协议栈、Secret Vault、账单结算和合规认证不在本 P 范围。
