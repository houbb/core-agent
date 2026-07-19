# P18 Desktop Workspace 实现说明

## 范围

实现 Tauri2 + Vue3 Developer Workspace 壳，围绕 Agent Runtime 可视化提供 Console、Project、Changes、Trace、Tools、Memory、Sessions、Settings 八个工作区。不实现代码编辑器、Workflow Canvas、多窗口或企业后台。

## Desktop 架构

- 新增 `agent-desktop` Vue/Vite 前端与 `agent-desktop/src-tauri` Rust 应用。
- Vue `DesktopController` 集中管理 Workspace Snapshot、Chat、SSE Trace、连接和面板错误；组件不散落业务 fetch。
- `HttpDesktopApi` 使用 `/api/project/tree|changes`、`/api/trace`、`/api/memory/list`、`/api/tool/status`、`/api/session/list`、`/api/chat` 和 SSE。
- 数据响应限制 2 MiB；单 API 缺失产生面板空态，全部离线才显示全局可恢复错误。

## Workspace UI

- 默认同屏展示 Project Explorer、Agent Console、Trace、Changes、Execution/Tools 与底部 Runtime 状态。
- 八个 Sidebar 入口切换 focused workspace；移动窄屏收敛为底部导航和 Console/Changes 主视图。
- 视觉采用深色黑金、17/13/11 层级、10px pill、三级按钮、充足留白、100% 自适应 Grid；图标使用现有 Lucide 组件，无装饰图片。
- 无服务端时展示真实 Offline/Empty state，不填充伪业务数据。

## 本地 UI 状态

- Rust Bridge 只持久化 `WINDOW/LAYOUT/RECENT_PROJECT/THEME/SHORTCUT` 偏好。
- SQLite 单表 `ui_preference` 具备五个审计字段、注释、索引、无外键、CAS 与结构列/JSON 冷读篡改检测。
- 偏好限制 64 KiB、16 层、256 项，拒绝 Secret/Token/Password 等敏感键。

## 测试覆盖

- Rust E2E：create/update/CAS/reopen、审计 schema、无外键、篡改和 Secret 拒绝。
- Vue：load/send/SSE trace/offline 隔离、八工作区切换、Sidebar 可访问性。
- 统一验证阶段运行 Cargo、Vitest、Vue typecheck 与 Vite production build。

## 已知边界

- 当前尚无兼容 Desktop API Server，真实业务数据需服务启动后显示。
- 拖拽停靠、窗口几何恢复、全局快捷键、Diff 语法渲染和 Memory 编辑操作尚未实现。
