# P9 Event Runtime — 实现说明

## 完成范围

- 新增独立 `core-agent-event` crate，提供 typed Event、Registry、Subscription、Router、Dispatcher、Policy、Lifecycle、Interceptor、Observer、Replay 与 Dead Letter 合同。
- Event Runtime 不依赖任何业务 Runtime；跨模块连接由根 crate 组合。
- 实现内存与 SQLite Store。SQLite 严格使用 `event`、`event_subscription`、`event_replay`、`event_policy`、`event_dead_letter` 五张表，全部含审计字段、注释和索引，且无外键。
- 根 crate 新增 typed `MemoryRememberPayload` 与 `MemoryRememberEventHandler`，验证 Event 可写入 Memory，且没有反向依赖。

## 核心语义

- 发布前校验类型、namespace、策略和完整路由结果；路由失败不会留下半成品事件。
- 发布先持久化 `Published`，再原子保存 `Dispatched + Pending deliveries`，处理器调用前持久化 `Delivered`。
- delivery ID 在重试和崩溃恢复中保持稳定，语义为 at-least-once；处理器必须按 delivery ID 幂等。
- 同 event ID、相同不可变内容返回幂等结果；同 ID、不同内容明确冲突。
- fan-out 按 subscription priority、key、ID 确定性串行执行；单个处理器失败不阻塞其他订阅者。
- 重试次数受 subscription 与 policy 双重限制并硬上限为 10；耗尽后 Event/Replay 状态与 Dead Letter 在同一事务提交。
- `resume` 与 `resume_replay` 可恢复未完成投递；未知处理结果复用原 delivery ID 和 attempt。
- Replay 必须显式提交 actor/reason，原 Archived Event 保持不变，Replay 及其 Dead Letter 使用独立审计记录。

## 最小使用方式

```rust
let manager = EventManager::builder().build();
manager.register_type(EventDefinition::for_payload::<MyPayload>("description"))?;
manager.subscribe(subscription, handler).await?;

let event = EventEnvelope::from_typed(
    "tenant-a",
    EventSourceKind::System,
    MyPayload { /* fields */ },
    "publisher",
)?;
let outcome = manager.publish(event).await?;
```

进程恢复后重新注册 type、通过 `bind_existing` 绑定持久化 subscription 的 live handler，再调用 `resume(event_id, actor)` 或 `resume_replay(replay_id, actor)`。

## 验证结果

- P9 单元断言：3 项通过。
- P9 Runtime E2E：13 项通过。
- Event → Memory 跨 Runtime E2E：1 项通过。
- `cargo clippy -p core-agent-event --all-targets -- -D warnings` 通过。
- `cargo test --workspace`、`cargo fmt --all -- --check`、`git diff --check` 通过。
- 根 crate 仍报告 8 条既有 ambiguous glob re-export warning，与 P9 无关。

## 明确未实现

- 分布式 Broker、Event Sourcing、CQRS、延迟调度、跨进程 handler discovery、Exactly Once 与 UI。

