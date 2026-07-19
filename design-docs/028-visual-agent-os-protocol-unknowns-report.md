# P23 AgentOS Protocol Unknowns Report

## Scope

从已实现 Runtime/Studio/CLI/Ecosystem 的真实合同中提取 `AgentOS Internal Contract 0.1`：版本化 Protocol Document、Runtime/Capability/Agent/Workflow/Memory/Event/Trace/UI/Marketplace/SDK/Command typed spec、Discovery Registry、兼容性报告和最小 CTK。不会把尚未稳定的内部合同宣布为公共标准 v1.0。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | 现在是否发布 AgentOS Specification v1.0 | 不发布；命名为 Internal Contract 0.1，只有同 major/不高于当前 minor 才兼容，待多语言实现和真实第三方 Runtime 验证后再开放 v1 |
| P0 | 是否重新定义已有 Runtime 类型 | 不复制业务状态；Protocol 使用可序列化描述符与 stable resource ref，根组合层从 Kernel/Visual/Ecosystem 等真实类型投影 |
| P0 | UI Protocol 能否携带前端代码 | 禁止；只允许有界 panel/field/safe relative API endpoint，不允许脚本、远程组件或任意 URL |
| P0 | Schema 安全边界 | JSON Schema 仅作有界声明；限制 256 KiB、32 层、敏感 key，暂不声称完整 JSON Schema dialect validator |
| P0 | 注册更新语义 | 同 kind/key/version 内容不可变；相同 hash 幂等，内容变化必须发布新 semantic version，避免协议漂移 |
| P1 | 引用解析 | 注册时所有 resource ref 必须已存在并版本精确匹配；依赖先注册，Discovery 返回确定性顺序 |
| P1 | CTK 范围 | 校验版本、类型匹配、标识符、schema/endpoint/图/引用/内容 hash；不做网络互操作或 Java/Python SDK 生成 |
| P1 | OpenAPI | 提供 discovery/register/schema/health/event endpoint descriptor，不启动第二个 Server 或伪造已部署 API |

## Acceptance

- 十类以上 typed spec 能 round-trip、注册、discover，并对同版本内容漂移 fail-closed。
- CTK 能拒绝未来版本、跨类型 spec、不安全 endpoint/schema、缺失引用和无效 Workflow/UI。
- 根组合测试从真实 Kernel Runtime/Visual Panel/Ecosystem Package 生成并发现 Protocol Document。
- 最终文档明确 Internal 0.1 与 Public Specification v1.0 的成熟门槛。

## Residual risks

- 当前 CTK 是结构兼容，不验证跨进程 wire semantics、重试/幂等、流控或不同语言 SDK 行为。
- 公开协议需要至少 Rust/Java/Python 独立实现、版本演进样本和第三方互操作测试后再冻结。
