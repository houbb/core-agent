# CHANGELOG

## [0.2.0] - 2026-07-17

### Phase 1: Context Runtime

Context Runtime — Agent 上下文生命周期管理器。负责构建 Agent 每一次推理所需要的完整上下文。

**不做 LLM 调用，只做上下文组装。** Context ≠ Prompt。Context 是结构化的上下文数据，由 Provider 收集、Reducer 裁剪、Composer 组装后交给后续的 Model Runtime。

#### 架构

```
core-agent (workspace root)
├── core-agent-session (Session Runtime)
└── core-agent-context  (Context Runtime) ← 新增
    ├── api/          — 公开 API (ContextRuntime)
    ├── application/  — 用例编排 + ContextPipeline + SummaryReducer + DefaultComposer
    ├── domain/       — Context + ContextSegment + ContextSlot + 7 个子 Context
    ├── infrastructure/ — 4 个扩展点 trait (ContextProvider / ContextReducer / ContextComposer / ContextSnapshotStore)
    ├── persistence/  — SQLite 实现 + 4 个内置 Provider
    ├── dto/          — 输入输出 DTO
    └── error/        — 统一错误类型
```

#### 核心组件

| 组件 | 描述 |
|------|------|
| ContextBuilder | 流程编排（Pipeline Builder 模式），Collect → Reduce → Compose → Snapshot |
| ContextProvider | 4 个内置 Provider：System / Conversation / Environment / User |
| ContextReducer | SummaryReducer：摘要 + 保留最近 N 条（默认 20），超出预算时生成摘要 |
| ContextComposer | DefaultComposer：将 segments 分配到 8 个 Slot，组装完整 Context |
| ContextSnapshot | 每次 build() 后保存完整 Context JSON 到 SQLite |
| ContextPipeline | 不可变管道，链式执行各阶段，支持自定义扩展 |

#### ContextSlot 机制

8 个槽位，每个独立：Token 估算 / 优先级排序 / 启用禁用 / 预算控制。

```
System(100) > Environment(90) > Workspace(80) > Memory(70)
> Conversation(60) > Tool(50) > Plugin(40) > User(30)
```

#### Context 对象

7 个独立子结构：System / Conversation / Workspace / Memory / Environment / Plugin / User，含 TokenDistribution 和 SHA-256 哈希。

#### 持久化

- `context_snapshot` 表：id/session_id/conversation_id/created_at/content/token_count/hash/build_duration_ms
- 3 个索引：session_id / created_at DESC / hash

#### 与 Session Runtime 集成

- 依赖 `core-agent-session`（只读），通过 `Arc<dyn SessionStore>` 读取消息历史
- `ContextRuntime<S: SessionStore>` 接收 Session Store 作为依赖

#### 测试

- 33 个单元测试全部通过
- 覆盖 domain / application / dto / persistence / api 层
- 集成测试：Session → Messages → build_context → 验证裁剪

---

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
