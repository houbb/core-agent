# P22 AgentOS Ecosystem 实现说明

## 范围

将最终产品阶段从“另一个 AgentOS”校正为 AgentOS Ecosystem：Marketplace、Capability Marketplace、Template Center、Developer SDK、Publishing、Community Signals 和 Cloud 能力边界。

## Ecosystem Runtime

- 新增独立 `core-agent-ecosystem`，管理 Tenant-scoped Publisher、Agent/Capability/Template/SDK Package、Publication Review、Rating 和 Installation Plan。
- 每个生态写操作和 install plan 都先通过 P13 Platform default-deny Policy，并生成允许/拒绝 Audit。
- Package 使用精确 semantic version、SHA-256 checksum、外部 signing key id、稳定 capability key；不保存制品正文、私钥或凭据。
- Draft → InReview → Listed 需要 Publisher owner 提交和非 Owner 独立审批；支持 Suspend/Resume/Retire。
- Dependency 使用已 Listed 精确版本，DFS 生成依赖优先、确定性去重的安装计划；缺失、自依赖和环 fail-closed。
- Rating 每个 subject/Package 只能提交一次，限制 1～5 并保存整数聚合。

## Runtime Integration

- `EcosystemExtensionInventory` 把 Marketplace `required_capabilities` 与真实 P12 Extension Runtime 对比。
- Catalog 只生成受治理安装计划，不绕过 P12 Extension Policy、checksum 校验、生命周期和 Host 隔离。
- P15 产品合同最终阶段更名为 `AgentEcosystem`，旧 `AGENT_OPERATING_SYSTEM` 序列化值作为 alias 保持兼容；readiness 要求 Marketplace/SDK/Publishing/Template 四项能力。

## Ecosystem Workspace

- Desktop 新增 Marketplace/My Agents/Capabilities/Templates/Developer/Publishing/Community/Cloud 八中心。
- Marketplace 和 Publishing 提供真实 Install/Submit API 动作，失败保留快照并显示错误；响应限制 2 MiB。
- Cloud Center 明确未配置远端同步，不伪造 Session/Memory/Trace/Build 云能力。
- UI 使用组件化黑金 Apple 层级、pill、三级按钮、自适应卡片与真实空态。

## 测试覆盖

- Runtime E2E：Publisher → Capability → Agent → 独立审核 → Listed → 依赖拓扑安装 → Rating。
- 安全测试：自审、缺失依赖、自依赖、重复评分与 Platform 默认拒绝。
- 跨 Runtime：Marketplace capability requirement → P12 Extension inventory 缺口。
- Vue：Marketplace 加载、安装后刷新、发布拒绝和全局 Ecosystem 导航。
- 统一验证在最后一个协议 P 完成后执行。

## 已知边界

- Catalog 当前进程内；生产需要 durable metadata registry、artifact object store 和真实 signature verifier。
- Discussion/Issue、收费分成、Billing、联邦 Marketplace、多云同步不在本 P 范围。
