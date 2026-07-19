# Enterprise Agent 真实集成 Unknowns Report

> 日期：2026-07-19  
> 模式：Deep（跨 Runtime、CLI、Desktop、认证/治理、持久化）

## 目标

把当前 Cargo Workspace 中已经独立实现的 Runtime 组合成一个可直接启动的企业 Agent：统一 Server/Kernel 启动、真实 Session → Context → Model/Planning → Agent → Execution → Tool → Memory/Event 链路、CLI/Desktop 共用 REST/SSE，并让 Platform Policy/Audit/Governance 覆盖入口和危险动作。

成功标准不是“crate 能单独测试”，而是：

1. 一条命令启动统一服务。
2. CLI 和 Desktop 调用同一个真实 API。
3. 一个任务能跨越核心 Runtime 并产生可恢复 Session、Trace、Tool 结果、Memory/Event 和 Audit。
4. 单元断言、进程内集成、真实 HTTP/SSE E2E 全部通过。
5. README 能让新用户从零启动并完成一次任务。

## Known knowns

| 事实 | 证据 | 影响 |
|---|---|---|
| Workspace 有 23 个 Rust package、390 个 Rust `#[test]`/`#[tokio::test]` 声明和 6 个 Vue test 文件 | `cargo metadata`、测试扫描 | 独立模块与测试基础存在，不能据此证明在线闭环 |
| 根组合层已有 15 个跨 Runtime test 文件 | `tests/*.rs` | Tool/Execution、Memory/Event、Workflow、Multi-Agent 等局部链路可复用 |
| Kernel 的业务适配器目前只有 `PlatformKernelRuntime` | `src/lib.rs` 的唯一生产 `impl ManagedRuntime` | 不能统一启动全部 Runtime |
| CLI/Desktop 默认访问 `http://127.0.0.1:8080` | `agent-cli/src/config.rs`、Desktop API clients | 需要真实 Server；当前没有对应 binary/package |
| CLI 和 Desktop 的 Session 路由不完全一致 | CLI `/api/sessions`，Desktop `/api/session/list` | API 必须统一并保留兼容 alias 或同步客户端 |
| Studio/Collaboration/Enterprise/Ecosystem 都定义了 HTTP client | `agent-desktop/src/*-api.ts` | 当前 UI 只有 mock/controller 测试，真实服务端路由缺失 |
| Model Runtime 没有进入根 `RuntimeAgentCoordinator` 主链 | 根 integrations 未引用 `core_agent_model` | 当前 Agent 执行不是完整 LLM Agent loop |
| Governance、Collaboration、Ecosystem 的业务 Registry 是进程内 | P20～P22 实现说明与源码 | 重启丢失，不能宣称生产级 durable enterprise data |
| 当前统一 `cargo test` 已发现并修复 P12/P13 语法错误，但尚未得到全绿结果 | 2026-07-19 验证记录 | 所有“已实现”状态仍需重新编译验证 |

## Material unknowns and decisions

| 分类 | Priority | 未知项 | 当前决策 / 验证方式 |
|---|---:|---|---|
| Blocker | 625 | 统一进程如何拥有所有 Runtime | 新增 `core-agent-server`，由 composition root 构造共享 managers；Kernel 管生命周期，Server 管 HTTP/SSE |
| Blocker | 500 | 真实 Agent loop 的 Model 如何注入 | 定义可替换 `AgentInference` 边界；生产用 Model Runtime，E2E 用确定性 provider，不允许 Server 伪造 LLM 结果 |
| Blocker | 500 | actor 能否由客户端任意提交 | HTTP 层只从认证 middleware/external identity adapter 注入 actor；MVP local mode 使用显式受限 principal，不接受 body 覆盖 |
| Decision | 400 | 二十多个 crate 是否变成二十多个进程 | 不拆进程；保持 crate 模块化，默认单进程组合。未来 remote provider 通过 Protocol/Extension 扩展 |
| Decision | 320 | API 路径以 CLI 还是 Desktop 为准 | 建立版本化统一 API，暂时提供旧路由 alias；客户端随后收敛到同一 contract |
| Decision | 320 | 如何处理进程内企业数据 | 本轮先保证进程级 E2E并明确非生产边界；进入“企业级完成”前补 durable store，不能只在 README 标注后忽略 |
| Experiment | 240 | SSE 断线/恢复语义 | HTTP E2E 验证 event id、终态、断线重连和 session resume；未知结果不盲目重放 |
| Decision | 225 | Kernel 是否为每个 Manager 写专用 wrapper | 提供薄 `ManagedRuntime` adapter，只做生命周期/健康，不把业务逻辑搬进 Kernel |
| Monitor | 144 | Tauri/全 Workspace 首次构建耗时 | 分层执行 package tests，最后全 Workspace gate；保留统一最终命令 |

## Unknown knowns

- “企业级”隐含要求不是 UI 菜单多，而是认证主体可信、默认拒绝、审计完整、数据可恢复、故障可观测。
- “直接一起使用”隐含要求是一条启动命令和统一状态，而不是让用户理解 Cargo crate 拓扑。
- CLI/Desktop 需要业务等价，不应分别维护两套 Agent 行为。
- Mock controller test 只能验证 UI 状态管理，不能替代真实 Server E2E。

## Unknown unknown candidates

- 并发 Session 对 SQLite store/manager live guards 的竞争与恢复。
- SSE 在 terminal event 写入前断流时的终态一致性。
- Model 成功计费但 Session/Trace 持久化失败时的补偿与 Audit。
- Tool 已产生外部副作用但 Execution checkpoint 失败时的 OutcomeUnknown。
- 企业 actor、tenant、organization scope 在跨 Runtime adapter 中丢失。
- Desktop EventSource 无自定义 Authorization header 时的认证策略。

## Implementation handoff

### Invariants

- 所有业务状态由 Runtime 持有；Server 只编排与映射 DTO。
- CLI/Desktop 不执行 Planning、Policy、Cost 或 Tool 业务逻辑。
- Session ID、tenant、actor、request ID 在整条链路稳定传播。
- 危险 Tool 和企业写操作未命中 Allow Policy 时默认拒绝并审计。
- SSE terminal event 只有在持久化终态后发布。
- 同一 request/session 恢复必须幂等；外部副作用未知时不得自动重放。

### Required tests

- Server boot/shutdown + 全 Runtime health。
- 真实 HTTP 创建 Session、发送消息、SSE 终态、查询/恢复/取消。
- Agent loop 横跨 Context/Model/Planning/Execution/Tool/Memory/Event/Audit。
- CLI binary 对真实临时 Server E2E。
- Desktop API/controller 对真实临时 Server E2E；UI 单测与 build。
- 重启恢复、默认拒绝、断线恢复、并发 Session 与 OutcomeUnknown。

### Non-goals for the first integration slice

- 多节点 cluster、云同步、联邦 Marketplace、Billing settlement。
- 把所有 Runtime 拆成微服务。
- 在没有三语言互操作前发布 Protocol v1.0。
