# P14 Runtime Kernel Unknowns Report

## Scope

`015-Kernel.md` 的新增内容是位于现有 Runtime 之下的统一 Runtime Kernel。P14 实现进程内 MVP：Runtime Registry、依赖拓扑、统一 init/start/stop/reload、类型安全 Service Registry、有界配置、Kernel Event、Health 聚合、同步 Hook、语义版本兼容与失败回滚。不会改写 P0～P13 各 Runtime 的内部生命周期，也不实现分布式控制面。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | Kernel 是否直接依赖全部业务 Runtime | Kernel crate 保持业务无关；根组合层提供具体适配器，避免形成中心化循环依赖 |
| P0 | 启动顺序与失败语义 | 注册前完成依赖存在性、版本和 DAG 校验；按拓扑顺序启动，失败时反向停止本次已启动 Runtime |
| P0 | reload 是否允许改变身份/依赖 | reload 只传递新配置快照，不允许改变 Descriptor、依赖或版本；身份变化必须重新注册 |
| P0 | Service Registry 类型擦除安全 | 只接受 `Arc<T: Any + Send + Sync>`，按稳定 key 注册并在 resolve 时强制类型匹配 |
| P0 | 配置是否可保存 Secret | Kernel 配置拒绝敏感 key、控制字符、过深/过大 JSON；Secret 只允许引用，不保存正文 |
| P1 | 生命周期并发 | Kernel 全局操作串行化；每个 Runtime 状态单独锁定，避免 start/stop/reload 交错 |
| P1 | Hook/Event 失败是否改变主流程 | Hook 是控制面，可拒绝 before 操作且 panic fail-closed；Event Sink 是观察面，失败不回滚已经完成的生命周期 |
| P1 | 版本兼容语义 | MVP 使用同 major 且实际版本不低于最低要求；不引入完整 SemVer range 解析器 |
| P1 | 持久化 | Kernel MVP 不新增数据库表；Runtime 自身仍拥有各自持久化，Kernel 冷启动重新注册并协调 |

## Acceptance

- 依赖顺序、缺失依赖、版本不兼容和循环依赖可确定性验证。
- init/start/stop/reload 状态机与启动失败反向恢复有断言测试。
- Service Registry 类型安全；配置拒绝敏感内容并具备单调 revision。
- Kernel Event、Health 与 Hook 扩展契约隔离清晰。
- 根组合层至少用真实 Platform Runtime 证明 Kernel 适配链路。

## Residual risks

- 进程崩溃后的控制面状态不持久化，重启需由应用组合层重新注册。
- Event/Health/Service 均为进程内契约；跨进程发现、租约、选主和滚动升级不在本 P 范围。
