# CHANGELOG

## [0.1.0] - 2026-07-17

### Phase 0: Session Runtime MVP

Session Runtime — Agent 生命周期管理器。负责 Agent 从出生到结束的整个生命周期。

**不做 AI，只做基础设施。** 后续所有 Runtime（Context / Model / Tool / Workspace / Planning / Execution / Memory / Permission / Plugin / Observation / Multi-Agent）全部依赖此层。

#### 架构

```
core-agent (workspace root)
└── core-agent-session (Session Runtime)
    ├── api/          — 公开 API (SessionRuntime)
    ├── application/  — 用例编排 (SessionApplicationService)
    ├── domain/       — 5+1 核心实体
    ├── infrastructure/ — 扩展点 trait (SessionStore)
    ├── persistence/  — SQLite 实现 (5 张表)
    ├── dto/          — 输入输出 DTO
    ├── event/        — EventBus (tokio::broadcast)
    └── error/        — 统一错误类型
```

#### 核心实体

| 实体 | 描述 |
|------|------|
| Session | Agent 生命周期载体，状态机：CREATED → READY → RUNNING → PAUSED → ARCHIVED → DELETED |
| Conversation | 属于 Session，类型：MAIN / PLAN / REVIEW / SYSTEM / DEBUG（MVP 只用 MAIN） |
| Message | 消息实体，状态：PENDING / STREAMING / DONE / FAILED |
| Attachment | 附件统一模型（图片/文件/日志/Diff/Terminal/PDF） |
| Manifest | Session 概要快照（名称/模型/workspace/标签/统计），左侧列表用 |
| Metadata | JSON 扩展容器，避免不断加字段 |

#### EventBus

基于 `tokio::sync::broadcast`，事件类型：
- `SessionCreated` / `SessionUpdated` / `SessionStateChanged` / `SessionDeleted`
- `ConversationCreated`
- `MessageAdded` / `MessageUpdated` / `MessageDeleted`
- `ManifestUpdated`

#### 持久化

- SQLite（rusqlite + r2d2 连接池）
- 5 张表：`session` / `conversation` / `message` / `attachment` / `manifest`
- 全部软删除，禁止外键

#### 测试

- 27 个单元测试全部通过
- 覆盖 domain / dto / event / persistence 层

#### 依赖

- Rust 1.94.0
- tokio (async runtime)
- rusqlite 0.32 (bundled SQLite)
- serde / serde_json
- uuid v4
- chrono
- async-trait
- thiserror 2
