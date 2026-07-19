# P18 Desktop Workspace Unknowns Report

## Scope

实现 Tauri2 + Vue3 Developer Workspace 第一版：Console、Project、Changes、Trace、Tools、Memory、Sessions、Settings 八类视图，Rust 本地偏好 Bridge，REST/SSE Agent API Controller。不是 CLI 套壳，也不是代码编辑器。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | Desktop 是否保存 Runtime 业务数据 | 否；本地 SQLite 只保存 window/layout/recent project/theme/shortcut UI 偏好，Session/Memory/Trace/Tool 仍归 core-agent |
| P0 | 当前 Desktop API Server 不存在 | 前端实现真实 REST/SSE Client、独立错误/空态；不内置假业务数据，Controller 可用 Mock 测试 |
| P0 | 是否做代码编辑器 | 不做 Monaco；Project Explorer 只浏览上下文，Changes 只显示服务端 diff 数据 |
| P0 | Trace 是否展示 Prompt 正文 | 默认展示步骤、耗时、token、工具与摘要；敏感 input/output 由服务端按权限脱敏，Desktop 不自行请求 Secret |
| P1 | Tauri Bridge 职责 | 只负责设备本地偏好 SQLite 和窗口壳；业务调用使用统一 Agent HTTP/SSE API |
| P1 | UI 信息架构 | 采用可扩展 Workspace/Panel 模型，默认 Console + Explorer + Trace + Changes + Bottom Tool 面板，无多浮窗 |
| P1 | 视觉 | 深色黑金、Apple 层级/留白/pill/三级按钮；100% 自适应，不依赖装饰图片 |
| P1 | 前端状态 | 单一 DesktopController 归一化 API 状态，失败按面板隔离；不在组件中散落 fetch |

## Acceptance

- Tauri 应用可启动 Vue workspace，Rust Bridge 可持久化并重开 UI 偏好。
- 八类工作区可导航，默认视图同时呈现 Chat/Project/Trace/Changes/Tool/Status 信息关系。
- REST/SSE Controller 可加载六类 API、消费事件并更新 Trace/状态，单面板失败不拖垮整体。
- Rust Store 单元/E2E、前端 Controller/状态测试与生产构建在统一验证阶段通过。

## Residual risks

- 缺少兼容 Desktop API Server 时只能展示明确离线/空态。
- 拖拽停靠、窗口几何恢复、快捷键全局注册和真实 Diff 渲染仍是后续增强。
