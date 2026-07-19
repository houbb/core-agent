# P23 AgentOS Internal Protocol 0.1 实现说明

## 范围

从真实 Kernel/Runtime/Visual/Ecosystem 合同中沉淀 `AgentOS Internal Contract 0.1`，提供 typed descriptor、Discovery Registry 和 Compatibility Test Kit。当前版本明确不是公开 Specification v1.0。

## Internal Protocol

- 新增独立 `core-agent-protocol`，统一 `ProtocolVersion`、`ResourceCoordinate`、`ProtocolDocument` 和 tagged `ProtocolSpec`。
- 提供 Runtime、Capability、Agent、Workflow、Memory、Event、Trace、UI、Marketplace、SDK、Command 十一类 typed spec。
- Runtime 声明 lifecycle/health/event API 与 capability/event/UI refs；Capability/Event 携带有界 schema；Studio/Desktop 只消费声明式 UI Panel，不加载任意前端代码。
- Workflow 校验 node/edge 引用；Agent、Trace、Marketplace、Command 使用精确 kind/key/version ref；Marketplace 保留内容 SHA-256。
- 同 kind/key/version 内容不可变：相同 hash 注册幂等，内容漂移必须发布新版本。

## Discovery and CTK

- `ProtocolRegistry` 提供 register/find/schema/discover/revision；引用必须先注册且精确匹配，发现顺序由 `BTreeMap` 确定。
- Discovery 支持按 Protocol kind 和 advertised capability 查询，可对应 `/runtime/register`、`/runtime/discover`、`/runtime/schema` 等服务端 endpoint。
- `CompatibilityTestKit` 校验 Internal Contract 版本、document/spec kind、标识符、semantic version、大小、JSON schema、安全 `/api/` endpoint、Workflow/UI 结构和资源引用。
- Schema 限制 256 KiB/32 层并拒绝敏感 key；CTK 是结构兼容检查，不冒充完整 JSON Schema dialect 或网络互操作验证。

## 真实 Runtime 投影

- `kernel_runtime_protocol` 把 P14 `RuntimeDescriptor` 投影为 Runtime Protocol。
- `visual_descriptor_protocol` 把 P19 声明式 Panel/Field/DataSource 投影为 UI Protocol，使 Studio 无需认识业务 Runtime。
- `marketplace_package_protocol` 把 P22 Package/Dependency/Capability requirement 投影为 Marketplace Protocol，缺失 capability version 时拒绝猜测。
- 根集成测试让真实 Kernel + Visual + Marketplace descriptor 汇聚到同一 Discovery Registry。

## 测试覆盖

- Typed document YAML round-trip、按 capability discovery、schema、registry revision。
- 同版本幂等/内容漂移、未来 contract、不安全 endpoint、敏感 schema、缺失引用和无效 Workflow edge 拒绝。
- Kernel/Visual/Ecosystem 跨模块 Protocol discovery。
- 最终统一验证随全部 P 一次执行。

## 公开 v1.0 门槛

- 至少 Rust/Java/Python 三种独立 SDK 实现。
- 多个第三方 Runtime/Studio/CLI 对 wire semantics 的互操作测试。
- 协议版本演进、重试/幂等/流控、安全与签名规范成熟。
- 独立 CTK 能验证跨进程行为，而不只是结构。
