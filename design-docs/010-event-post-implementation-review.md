# P9 Event Runtime — 实现后复核

## 结论

**PASS**。P9 已按本地 typed Event Runtime 边界完成，并通过单元、Runtime E2E、跨 Runtime E2E、严格 Clippy 与全 workspace 回归。

## Review 1：架构与边界

- Event crate 只包含通信机制，不依赖 Memory、Tool、Execution 等业务 Runtime。
- typed payload、namespace、Registry 与 schema version 形成稳定公开合同。
- durable subscription declaration 与 process-local handler binding 分离，符合本地 Bus 边界。
- 五表模型覆盖 Event、Subscription、Replay、Policy 与 Dead Letter，没有引入设计外的 Broker/CQRS/Event Sourcing。

发现并修复：发布原先可能在路由失败前持久化 `Published`。现已改为先完成路由、扩展输出校验及策略授权，再进入持久状态。

## Review 2：一致性与故障恢复

- Event lifecycle 与 per-subscription delivery state 增加组合不变量。
- Event 与直接 Dead Letter、Replay 与 Replay Dead Letter 均采用事务原子提交。
- handler 调用前持久化 `Delivered`；未知结果通过稳定 delivery ID/attempt 续投。
- duplicate publish 校验不可变内容，避免相同 ID 静默接管另一事件。
- SQLite 冷读取交叉校验结构列、JSON aggregate、payload hash 与 owner 关系。

发现并修复：重复发布原先只返回非终态记录，无法推进崩溃后遗留投递；Replay 也缺少同等恢复能力。现已增加 `resume`、`resume_replay` 与持久化 Pending 计划。

## Review 3：测试证据

- 单元测试覆盖 typed round-trip、namespace 隔离、嵌套敏感键拒绝。
- Runtime E2E 覆盖确定性优先级、幂等/冲突、有限重试、死信隔离、Replay 原事件不变、发布/Replay 恢复、注销、策略、panic 隔离、CAS、SQLite 五表/冷恢复/篡改检测。
- 跨 Runtime E2E 验证 typed Event 真实写入并召回 Memory。
- 严格 Clippy、全 workspace tests、格式与 diff 检查均通过。

## 剩余风险与后续边界

- At-least-once 不能保证外部副作用 Exactly Once；handler 仍必须按 delivery ID 幂等。
- live handler 需要进程启动时重新绑定；缺失 handler 会按有限重试进入 Dead Letter。
- 分布式 lease、延迟调度、Permission Runtime 对接和 Timeline UI 留待后续阶段。

