# P14 Runtime Kernel 实现说明

## 范围

实现进程内 Runtime Kernel：注册与发现、依赖与版本校验、统一 init/start/stop/reload、类型安全 Service Registry、有界 Configuration、Kernel Event、Health 聚合和 Hook。Kernel 不参与 LLM、Tool 或 Workflow 业务，也不替换现有 Runtime 的内部状态机。

## 架构

- 新增独立 `core-agent-kernel`，保持业务 Runtime 无关。
- `ManagedRuntime` 是统一生命周期和健康契约；`RuntimeDescriptor` 声明稳定 ID、语义版本、配置 schema 版本和依赖。
- `RuntimeKernel` 在启动前完成必需依赖、同 major 最低版本和 DAG 校验，随后按确定性拓扑顺序启动、反向停止。
- start 失败时，Kernel 会先尝试停止失败 Runtime，再反向停止本次已启动 Runtime，并把控制面标记为 Failed。
- 生命周期操作全局串行化；Runtime 状态独立记录，避免 register/start/stop/reload 交错。
- 根组合层新增 `PlatformKernelRuntime`，证明真实 Platform Runtime 可由 Kernel 管理而不产生反向依赖。

## 配置与服务

- 配置按 Runtime 保存单调 revision 快照；reload 成功后才替换当前快照。
- 配置限制条目、深度和 256 KiB 体积，拒绝 password/secret/token/private key 等敏感正文。
- `ServiceRegistry` 只注册 `Arc<T: Any + Send + Sync>`，重复 key 和类型不匹配显式失败。

## Hook、Event 与 Health

- before Hook 可拒绝生命周期操作，panic fail-closed；after Hook 与 Event Sink 属于完成后观察面，失败不篡改已完成结果。
- Kernel Event 只携带 Runtime 身份、事件类型和有界摘要，不携带配置正文。
- Health 对非 Running Runtime 或单 Runtime 检查错误生成独立 unhealthy 结果，不遮蔽其他 Runtime。

## 测试覆盖

- 单元断言：版本兼容、Descriptor 自依赖、敏感配置、Service 类型安全。
- Runtime E2E：拓扑启动/反向停止、reload revision、事件与健康、启动失败回滚、缺失依赖、循环依赖、版本不兼容、配置注册原子性。
- 跨 Runtime E2E：Kernel → 真实 Platform Runtime 的 start/reload/health/stop。

测试命令按用户要求，在所有剩余 P 实现完成后统一运行。

## 已知边界

- Kernel 控制面状态不持久化；进程重启由应用组合层重新注册 Runtime。
- 不包含跨进程发现、租约、选主、滚动升级、分布式配置或 Service Mesh。
