# P19 Agent Studio 实现说明

## 范围

实现从 Developer Tool 到 Developer Platform 的 Studio MVP：Home、Agent Designer、Workflow、Prompt、Memory、Capability、Knowledge、Trace、Model Studio，以及 Runtime 自描述的 Visual Protocol。

## Visual Runtime

- 新增独立 `core-agent-visual`，定义协议版本、Runtime Descriptor、Summary/Table/Timeline/Form/Graph/Metrics/Inspector Panel、Field、Data Source 与 Action。
- `VisualRegistry` 以 runtime_id + revision CAS 原子注册/更新，并按稳定 ID 生成 `StudioPanelCatalog`。
- endpoint 只允许无 traversal/query/fragment 的相对 `/api/` 路径；协议不接受 HTML、JS 或远程组件。
- DELETE/危险 Action 强制 `requires_approval`，Studio 发起后仍由服务端 Policy/Approval 决策。
- 根组合层新增 Platform Health/Audit Visual Descriptor，证明新 Runtime Panel 可自动出现在 Studio。

## Studio UI

- Desktop 新增 Agent Studio Workspace 与九个内部 Studio section。
- Home 展示真实资产计数、快捷创建入口和 descriptor 驱动的 Runtime Panel。
- Agent Designer 可填写 Name/Role/Model/Memory/Tools，真实 POST `/api/agent`，成功后更新版本化资产列表。
- Workflow 展示服务端 DSL 的有序节点；其他 Studio 通过统一资产卡片呈现 API 数据，不在本地伪造资产。
- Generic Visual Panel 只按声明字段渲染有界表格/状态，Action 执行前再次校验 endpoint，审批 action 显式确认。

## API 与安全

- `HttpStudioApi` 对 Agent/Workflow/Prompt/Memory/Capability/Knowledge/Trace/Model/Visual Catalog 并行加载，单项失败隔离、全离线明确报错。
- 响应限制 2 MiB；Agent 字段和 Tool key 有边界校验。
- Prompt 正文、Memory 正文、Secret 与 Provider 凭据不进入 Visual Descriptor。

## 测试覆盖

- Rust：Descriptor CAS、确定性 Catalog、远程/traversal endpoint 拒绝、危险 action 审批强制。
- 跨 Runtime：Platform Visual Descriptor → Registry → Studio Panel Catalog。
- Vue：Studio load/create Agent/失败保留快照/九 section 导航。
- Cargo/Vitest/typecheck/Vite build 在全部剩余 P 后统一运行。

## 已知边界

- Studio API Server 尚未实现，真实资产 CRUD 需服务端启动。
- A/B Test、Flame Graph、Model Benchmark、Knowledge ingestion、自由拖拽 Workflow 和 Marketplace 发布不在 MVP。
