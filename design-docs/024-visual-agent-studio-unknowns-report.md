# P19 Agent Studio Unknowns Report

## Scope

实现 Agent Studio MVP 与长期 Visual Runtime 协议：Visual Descriptor/Registry/Panel Catalog，以及 Desktop 中的 Home、Agent Designer、Workflow、Prompt、Memory、Capability、Knowledge、Trace、Model Studio。业务资产通过 Studio API 保存，UI 不持有第二套 Runtime。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | Visual Descriptor 能否执行任意前端代码 | 不能；协议只允许受控 Panel/Field/Action 枚举和相对 `/api/` endpoint，不接受 HTML/JS/component URL |
| P0 | 危险 Visual Action | DELETE/危险 action 必须 `requires_approval=true`；Studio 只发起请求，最终 Policy/Approval 仍由服务端决定 |
| P0 | Agent/Workflow/Prompt 的资产所有权 | 全部通过 `/api/agent|workflow|prompt...`；Desktop 表单是编辑器，不在本地 SQLite 保存业务资产 |
| P0 | 发布语义 | MVP 提供创建/保存与版本字段，不实现公开 Marketplace 发布；“发布 Agent”只预留受策略 action |
| P1 | Workflow Designer | MVP 展示 Runtime 定义的有序节点并提交 DSL，不实现自由脚本节点或 n8n 式任意执行 |
| P1 | Prompt 查看 | UI 展示版本/变量/摘要，Prompt 正文是否可见由 API 权限决定，不在 Visual Descriptor 内嵌 Prompt |
| P1 | Visual Registry 更新 | 使用 runtime_id + 单调 revision CAS；同 Runtime panel key 唯一，更新原子替换整个 descriptor |
| P1 | 前端自动面板 | Generic renderer 支持 Summary/Table/Timeline/Form/Graph/Metrics/Inspector；未知协议版本显式拒绝 |

## Acceptance

- Visual Descriptor 有大小/身份/endpoint/action 安全校验、CAS Registry 与确定性 Catalog。
- Studio 首页和八个子 Studio 可导航；Agent Designer 可真实 POST 创建并刷新列表。
- Descriptor Panel 可从 API 自动装配，不需要在 Studio 为每个 Runtime 写专用页面。
- Runtime 协议单元/E2E、Studio Controller/表单测试和前端构建在统一验证阶段通过。

## Residual risks

- 当前无 Studio API Server，资产 CRUD 的真实跨 Runtime E2E 需后续服务端。
- A/B Test、Flame Graph、Provider Benchmark、Knowledge ingestion 和 Marketplace 发布未纳入 MVP。
