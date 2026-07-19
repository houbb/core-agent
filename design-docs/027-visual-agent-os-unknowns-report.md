# P22 AgentOS Ecosystem Unknowns Report

## Scope

把已有 Runtime/Studio/Team/Enterprise 之上最后一个产品阶段落为开放生态 MVP：Publisher、Agent/Capability/Template Package、版本发布审核、依赖安装计划、评分聚合、Developer SDK 合同和 Desktop Ecosystem Workspace。不会实现远程制品上传、在线支付、商业分成、联邦市场或多云同步。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | Marketplace 是否等于 P12 Extension Registry | 不等于；P12 管本地 Extension 生命周期，P22 管可发现/审核/发布的软件包元数据与安装计划，通过稳定 capability key 衔接 |
| P0 | 发布信任与签名 | MVP 保存 SHA-256 checksum 与外部 signing key id；Review/Approve 后才可 Listed，不生成或保管私钥，不宣称供应链签名已验证 |
| P0 | 依赖安装如何避免错误版本 | 使用精确 package key + semantic version 字符串，发布时检查依赖存在且已 Listed，DFS 生成确定性拓扑安装计划并拒绝环 |
| P0 | 谁可发布/审核 | Publisher 必须 Active；包 Owner 才能提交，Reviewer 不能是提交者，审批后才进入 Marketplace |
| P1 | 数据与持久化 | MVP 使用进程内原子 Catalog，发布元数据不包含制品正文或 Secret；生产需 durable registry/object store/signature verifier |
| P1 | Community 范围 | 只实现有界 1～5 Rating 与聚合，不实现 Discussion/Issue 社区服务 |
| P1 | Cloud Center | 只呈现同步能力契约/空态，不伪造 Cloud Session/Memory/Trace 已上线 |
| P1 | ProductPhase 命名 | 将最终阶段从 AgentOperatingSystem 校正为 AgentEcosystem，并保留旧序列化 alias 兼容已有配置 |

## Acceptance

- Publisher → Package draft → submit → independent review → listed → dependency install plan 有断言测试。
- 未审核、缺失依赖、依赖环、自审、重复 rating 均 fail-closed。
- Marketplace/Capabilities/Templates/Developer/Publishing/Community/Cloud UI 使用真实 API、安装动作和空态。
- P22 不声称 Economy、Billing、Revenue Share、Federation 或 Cloud Sync 已实现。

## Residual risks

- Catalog 当前非 durable，checksum/signing key id 仅是可验证信息的载体，尚无远程 artifact verifier。
- 精确版本依赖先保证确定性；兼容版本范围和安全升级策略留给后续协议层。
