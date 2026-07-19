# P12 Extension Runtime 实现说明

## 范围

实现 P12.0 Local Extension：Discover/Install（声明输入）、Load、Enable、Execute、Disable、Offline Upgrade、Uninstall，并提供稳定 Capability/Provider 抽象。未实现在线 Marketplace、依赖解析、热更新、WASM/Process/Remote Host 或 VM 沙箱。

## 架构

- 新增独立 `core-agent-extension`，不知道 Agent、Workflow、Planner。
- `ExtensionManager` 是生命周期、Capability 查询、Provider 解析和 invocation 的统一入口。
- Manifest 是不可变 revision；安装/升级原子注册 Extension、Manifest、Capability、Provider 与 State timeline。
- `ExtensionLoader` 验证本地 `file:` artifact 和 SHA-256；`ExtensionHost` 隔离 start/stop/execute 边界。
- Capability 调用按 enabled 当前 Manifest、Provider priority/key/id 确定性解析。
- 根组合层 `ToolExtensionHost`/`ExtensionToolResolver` 可把 Capability 委托给既有 Tool Runtime。

## 安全与恢复

- 默认 Policy 拒绝声明 Network/File/Process/Environment 权限的安装、启用与执行；企业策略必须显式替换。
- Manifest/config/input/output/metadata 有体积、深度、条目和敏感 key 边界。
- invocation 在调用 Host 前持久化 request/capability/provider/input hash；Host 结果未知时保持 Running，`resume` 只能使用完全相同请求恢复。
- 生命周期与 timeline、Capability/Provider enabled 状态使用单事务 CAS。
- 每个 Extension 的生命周期操作和执行由不覆盖的 live guard 串行化。

## SQLite

严格五张表：`extension`、`extension_manifest`、`extension_state`、`capability`、`provider`。全部含审计字段、注释、索引且无外键；冷读取交叉验证结构化列、Manifest 当前归属和 Capability/Provider 声明归属。

## 测试覆盖

- 单元：YAML Manifest、未知 Capability 引用、嵌套敏感值。
- Runtime E2E：完整生命周期、离线升级、未知结果恢复、默认权限拒绝、Observer 隔离、SQLite 五表/重开/篡改。
- 跨 Runtime E2E：Extension Capability → Tool Runtime。

测试命令按用户要求在全部剩余 P 实现后统一运行。

## 已知边界

- P12.0 Host 是隔离接口，不是 OS/VM 安全沙箱；不得将不可信原生实现注册到同进程 Host。
- 外部 Provider 副作用依赖 request ID 幂等或查询协议。
