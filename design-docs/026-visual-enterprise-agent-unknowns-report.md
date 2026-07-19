# P21 Enterprise AgentOS Unknowns Report

## Scope

在 P13 Platform 之上实现 Enterprise AI Governance MVP：外部 Identity Binding、统一 AI Asset Registry、风险/数据分类、多人审查证据、Production/Suspended 生命周期、不可变 Cost Ledger、治理 Dashboard 与 Operation/Audit 视图。不会实现密码认证、OIDC 协议栈、Secret Vault 或账单结算。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | 企业层是否重写 Tenant/Policy/Audit/Quota | 不重写；`core-agent-governance` 依赖 P13 `PlatformManager`，每个写操作先调用默认拒绝 GovernanceRequest |
| P0 | Identity 安全边界 | 只保存 IdP external subject/provider/display name/role/group 引用，不保存密码、token、LDAP/OIDC Secret；身份真实性由部署层 IdP adapter 保证 |
| P0 | AI Asset 如何进入生产 | Draft → Reviewed → Approved → Production；所需审批数 1～8，创建者不得自审，审批主体必须绑定且 active |
| P0 | Cost 精度/幂等 | 使用整数 micros + ISO 3 字符 currency + event_key 幂等；不使用浮点金额，不做发票/支付/税务 |
| P1 | 风险与数据分类 | Public/Internal/Confidential/Restricted + 0～100 risk score；Production 前必须有批准证据 |
| P1 | 企业 Approval 与 P20 Review | P20 是项目任务 Review；P21 Approval 是 AI 资产生产治理证据，两者不共享状态机 |
| P1 | 持久化 | P21 Manager 先提供进程内原子 Registry/Ledger；P13 Policy/Audit durable。生产部署仍需企业 Store |
| P1 | UI | Enterprise Workspace 只展示治理数据和发起受控 API action，不读取 Secret 正文或原始 Prompt |

## Acceptance

- Identity bind、Asset register/review/multi-approval/production/suspend、自审拒绝、Cost 幂等/整数汇总有测试。
- 未配置 P13 Allow Policy 时所有企业写操作默认拒绝并审计。
- Enterprise Dashboard/Organization/Identity/Governance/Policy/Cost/Audit/Operation 可视化有真实 API 状态和空态。
- 企业层不宣称 SSO、Vault、Billing、Compliance certification 已实现。

## Residual risks

- 外部 subject 尚需部署层把已验证 IdP token 映射到调用 actor。
- Registry/Cost Ledger 进程重启丢失，生产需要 durable enterprise store。
