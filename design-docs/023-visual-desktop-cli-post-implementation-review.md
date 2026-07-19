# P18 Desktop Workspace 实现后审查

## 当前结论

**IMPLEMENTED — UNIFIED VERIFICATION PENDING**

## 第一轮：职责与数据所有权审查

- Desktop 只持有 UI 偏好；Memory/Session/Trace/Tool/Project 全部通过 API 获取。
- Rust Bridge 未复制 core-agent 业务状态机。
- 不存在 API 时明确 Offline，不使用 demo 数据冒充真实 Runtime。

## 第二轮：交互与视觉审查

- 默认第一视口以 Agent 工作过程为中心，不是通用 dashboard 或聊天全屏。
- 八 Workspace 共用 Panel 模型，未来可增量加入 Studio 面板。
- 采用 sites-building 的组件化、具体产品文案、可访问标签、响应式与无无关图片原则；桌面产品保持本地，不发布为网站。

## 第三轮：安全与持久化审查

- API 响应有界，SSE 非法事件隔离；Chat 输入有界。
- SQLite 偏好具备审计字段、索引、无外键、乐观版本与篡改检测。
- CSP 只允许本地应用和 localhost Agent API；偏好拒绝敏感数据。

## 遗留风险

- Tauri Window/Shortcut 权限和拖拽布局尚未启用，因此对应 Preference Kind 只预留持久化合同。
- 真实 Desktop E2E 仍需要 API Server 和桌面 WebView 环境。
