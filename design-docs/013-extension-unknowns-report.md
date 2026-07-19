# P12 Extension Runtime Unknowns Report

## Scope decision

实现 P12.0 Local Extension，并同时提供稳定的 Capability/Provider 契约。Extension Runtime 只管理外部能力接入、声明、生命周期和调用路由，不知道 Agent、Workflow 或 Planner。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | MVP 是否直接动态加载 Native/WASM/Process | P12.0 只支持注入式 Local Loader/Host；默认 Loader 校验本地 URI 和 checksum，不执行任意代码。WASM/Process/Remote 留后续 |
| P0 | Extension 与 Capability/Provider 关系 | Manifest 是不可变版本快照；安装时原子创建 Extension + Manifest + Capability + Provider，Agent 等上层只按 Capability 查找 |
| P0 | Execute 的职责边界 | Manager 解析 enabled Capability/Provider，调用 `CapabilityProviderExecutor` 端口；具体 Tool/MCP/HTTP 实现位于组合层或扩展实现 |
| P0 | 崩溃/副作用语义 | invocation 使用调用方 request ID 幂等；结果未知返回 OutcomeUnknown，不自动换 Provider 或重放 |
| P1 | 升级 | 仅 Disabled 状态支持离线升级，新 Manifest revision 原子切换；不实现热更新和依赖解析 |
| P1 | Manifest 格式 | 领域对象支持 YAML/JSON 反序列化，持久化统一 JSON；敏感值禁止进入 Manifest/metadata |
| P1 | 安全策略 | 默认 deny network/write/process 等权限，由 ExtensionPolicy 显式允许；P12.0 不宣称 VM 沙箱 |
| P1 | 五表 | 严格 `extension/extension_manifest/extension_state/capability/provider`，状态 timeline 与聚合写入同事务，无外键 |

## Acceptance

- install/load/enable/execute/disable/upgrade/uninstall 生命周期可恢复且可审计。
- Capability 可按稳定 key/version 查询；Provider 确定性选择并严格属于 Extension/Capability。
- Observer/Interceptor/Policy/Loader/Host/ProviderExecutor 可替换。
- SQLite 五表满足审计字段、注释、索引、无外键和冷读取交叉校验。
- 单元、Runtime E2E、根组合层 Tool capability E2E 覆盖。

## Residual risks

- P12.0 的“隔离”是 Host 边界而非操作系统/VM 沙箱；不应加载不可信原生代码。
- Provider 外部副作用完成后结果提交失败仍需 request ID 幂等或结果查询。
