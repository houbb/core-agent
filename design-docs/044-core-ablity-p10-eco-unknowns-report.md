# Unknowns Report

## Metadata

- **Task / Feature:** P10 Agent Ecosystem Layer — 补齐 3 个缺失模块
- **Mode:** Standard
- **Date:** 2026-07-22
- **Prepared by:** Claude
- **Scope:** core-agent-sdk / core-agent-openapi / core-agent-developer

## Intent

### User-visible problem

P10 设计中定义了 5 个生态层模块（ecosystem / marketplace / developer / openapi / sdk），但当前只有 `ecosystem` 和 `marketplace` 两个模块有实现，`developer` / `openapi` / `sdk` 三个模块缺失。

### Desired behavior change

新建 3 个模块，补齐 P10 生态层的核心能力，让第三方开发者可以围绕 Core-Agent 构建生态。

### Affected users and workflows

- **SDK 使用者** — 第三方开发者使用 SDK 开发 Agent/Tool/Skill/Plugin
- **OpenAPI 调用者** — 外部系统通过 REST API 调用 Agent
- **Developer Portal 使用者** — 开发者管理 Agent、API Key、发布流程

### Success criteria

1. 每个模块有完整的 domain 模型定义
2. 每个模块有核心 Manager/Service 逻辑
3. 每个模块有单元测试覆盖
4. 编译通过、无 lint 警告

### Non-goals

- 不修改已有 ecosystem/marketplace 模块
- 不做 Web UI 前端
- 不做 HTTP 服务端运行时（只定义接口和核心逻辑）
- 不做多语言 SDK（只做 Rust SDK）

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | `core-agent-ecosystem/src/lib.rs` | 现有生态层代码结构（domain/manager/error/validation） | High |
| Code | `core-agent-marketplace/` | Marketplace 完整实现模式（domain/manager/infrastructure/defaults/persistence + trait） | High |
| Tests | `core-agent-ecosystem/tests/` | 端到端测试模式 | High |
| Tests | `core-agent-marketplace/tests/` | 端到端测试模式 | High |
| Design doc | `design-docs/044-core-ablity-p10-eco.md` | P10 完整设计 | High |
| Workspace | `Cargo.toml` | 工作区成员列表，依赖管理 | High |
| Config | `CLAUDE.md` | 编码原则、DB 规范、测试要求 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| ecosystem 模块已经实现了 Publisher/MarketplacePackage 等核心域和完整的生命周期管理 | `core-agent-ecosystem/src/lib.rs` | 新模块不需要重复实现这些 |
| marketplace 模块已经实现了 AssetType/Store trait/SQLite 持久化 | `core-agent-marketplace/` | SDK 可以调用 marketplace 的接口 |
| 每个模块遵循 `domain.rs` + `error.rs` + `infrastructure.rs` + `manager.rs` + `lib.rs` 结构 | marketplace 代码 | 新模块应该遵循相同模式 |
| 项目使用 `thiserror` 定义错误类型，使用 `serde` 序列化 | 已有模块 | 新模块使用相同依赖 |
| 使用 `core-agent-platform` 进行授权/治理 | ecosystem 代码 | openapi 和 developer 需要集成 |
| 项目使用 `async-trait` 定义异步 trait | marketplace 代码 | SDK infra trait 需要使用 |
| 数据库表必须有 `id` `create_time` `update_time` `create_user` `update_user` | CLAUDE.md + marketplace schema | openapi 和 developer 的持久化要遵循 |
| 工作区使用 `uuid` + `chrono` 处理 ID 和时间 | 已有模块 | 新模块复用 |
| 每个模块需要 Cargo.toml 并注册到 workspace members | 已有模块 | 新建模块必须注册 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---|---|---|---|---|---|---|
| SDK 的 AgentClient 需要调用什么底层 Runtime？ | Known unknown | 设计文档说 "Core-Agent API → Runtime"，但当前没有明确的 Runtime client trait 可复用 | 4 | 3 | 2 | 4 | 96 | Decision | SDK 定义自己的 AgentClient trait，不依赖具体 Runtime 实现，通过 trait 抽象解耦 |
| OpenAPI 的 HTTP 框架选型 | Known unknown | 当前模块只定义核心逻辑，不启动 HTTP 服务。但 API 类型定义需要与框架无关 | 2 | 2 | 1 | 2 | 8 | Accept | 只定义纯 API 类型和 Gateway trait，不引入 HTTP 框架依赖 |
| Developer Portal 的 UI 形式 | Known unknown | 设计文档提到 "Agent Studio"，但本项目没有前端框架 | 3 | 4 | 1 | 3 | 36 | Monitor | 本次只定义后端领域模型和 Manager 逻辑，UI 后续实现 |
| SDK 如何与 marketplace 交互发布 | Known unknown | 设计文档说 SDK → Create Agent → Publish → Marketplace，但流程需要通过 Developer 模块协调 | 3 | 3 | 2 | 3 | 54 | Decision | SDK 定义 PublishRequest 类型，具体发布逻辑由 Developer Manager 处理 |
| OpenAPI 与 ecosystem 的授权关系 | Known unknown | OpenAPI 需要 API Key 认证，但 Key 的管理应该在 Developer 模块还是 OpenAPI 模块？ | 4 | 3 | 3 | 4 | 144 | Decision | API Key 的领域模型在 OpenAPI 模块，创建/管理在 Developer 模块 |
| SDK 是否是一个独立的 publishable crate？ | Known unknown | 作为生态对外入口，SDK 应该可以独立发布到 crates.io | 2 | 4 | 2 | 2 | 32 | Monitor | 先在 workspace 内作为成员开发，后续可独立发布 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 新模块应该复用 ecosystem 的 PackageKind/AssetType 定义 | 生态层应该有统一类型体系 | 对比设计文档中的 Asset 类型列表 |
| Developer Portal 的 API Key 管理应该与 OpenAPI 的认证打通 | 同一个 Key 既要能在 Portal 查看，也要能在 Gateway 验证 | 定义 shared API Key 类型 |
| SDK 的 Builder 模式应该与已有代码风格一致 | 已有 marketplace 使用 builder 模式 | 参考 MarketplaceManagerBuilder 实现 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| SDK 的 Tool/Skill/Plugin 定义可能与现有 extension/plugin 模块重复 | 已有 core-agent-extension 和 core-agent-plugin 模块 | 检查现有模块的接口定义 |
| OpenAPI 的 Rate Limit 需要与 governance 模块集成 | 已有 core-agent-governance 模块处理策略 | 集成时检查 governance 接口 |
| Developer 的 Agent 创建流程需要与已有 agent 模块交互 | 已有 core-agent-agent 模块 | 定义清晰的接口边界 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| API Key 模型归属 | ① 在 OpenAPI 模块定义 + Developer 模块管理 ② 全部在 Developer 模块 | ① 职责清晰，OpenAPI 直接验证 ② 更集中但耦合高 | Architecture | 实现前 |
| SDK 依赖哪些已有模块 | ① 只依赖核心类型（不依赖 marketplace） ② 依赖 marketplace 直接发布 | ① 轻量解耦 ② 功能完整但耦合 | Architecture | 实现前 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| SDK 使用 trait 抽象 AgentClient | trait 可以在不破环 API 的前提下替换实现 | 新增实现即可 |
| OpenAPI 定义纯数据模型 + trait | 不引入 HTTP 框架，可以随时加 axum/actix 封装 | 新增 infrastructure 层 |
| Developer 的 AgentManifest 使用 YAML 序列化 | 设计文档明确指定了 manifest.yaml 格式 | 支持 JSON 作为备选格式 |

## Recommended Implementation Boundary

### Implement now
- `core-agent-sdk` — AgentClient trait, AgentBuilder, Tool/Skill/Plugin 注解式 trait, 核心类型
- `core-agent-openapi` — API 请求/响应类型, APIKey 模型, Gateway trait, RateLimit 模型
- `core-agent-developer` — AgentManifest, Developer 模型, 创建流程, 发布流程

### Do not implement now
- 多语言 SDK（Java/Python/TypeScript）— 只做 Rust
- HTTP 服务端运行时 — 只定义接口和 trait
- Web UI / Agent Studio — 只做后端逻辑
- 与已有 ecosystem/marketplace 的集成 — 后续通过 workflow 打通

### Interfaces or data contracts to freeze
- SDK 的 AgentClient trait 签名
- OpenAPI 的 API 请求/响应类型
- Developer 的 AgentManifest 结构

### Areas that must remain reversible
- API Key 的存储方式（先 in-memory，后续可加 SQLite）
- Rate Limit 策略（先硬编码，后续可配置化）

## Verification Plan

### Automated
- 单元测试: 每个模块的 domain 模型 + manager 逻辑
- 集成测试: SQLite store 的读写验证

### Manual
- Happy path: 编译通过，测试通过
- Failure path: 错误类型覆盖所有边界情况