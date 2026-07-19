# P21 Enterprise AgentOS 实现后审查

## 当前结论

**IMPLEMENTED — UNIFIED VERIFICATION PENDING**

## 第一轮：企业边界审查

- P21 复用 P13 Tenant/Organization/Policy/Audit，不建立第二套治理真相源。
- Identity 是已验证外部主体的绑定引用，代码不接收密码、access token 或 IdP Secret。
- Cost Ledger 是整数用量事实，不冒充 Billing、Invoice 或支付系统。

## 第二轮：治理一致性审查

- 每个写入在本地状态变更前通过 Platform default-deny Policy，拒绝也进入不可变 Audit。
- Asset 状态迁移白名单、1～8 审批要求、Active principal 和 self-approval 拒绝均 fail-closed。
- Cost event key 幂等，Dashboard 使用整数 micros 聚合，未引入浮点财务误差。

## 第三轮：企业 UX 审查

- 企业首页聚焦 Governance Queue、风险资产、身份、成本和审计，不复制 Studio/Team 的创建与协作界面。
- Approve/Promote/Suspend 都调用真实 API，并在失败时保留当前快照和可见错误。
- UI 遵循 sites-building 的组件化、Apple 层级、100% 自适应、响应式、可访问导航和真实空态原则。

## 遗留风险

- 进程内 Registry/Ledger 重启后丢失，不能作为生产账本；需后续 durable store。
- actor 仍需部署认证层从已验证 IdP claim 注入；本层无法独立证明 external subject 的真实性。
