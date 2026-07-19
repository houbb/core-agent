# Unknowns Report

## Metadata

- **Task / Feature:** P032 桌面端统一工作区体验
- **Mode:** Standard
- **Date:** 2026-07-19
- **Scope:** Core context index、Tauri Runtime state、Vue Console

## Confirmed Facts

- Desktop 已在 Tauri 进程内持有 `EnterpriseAgent`，无需本地服务。
- `/`、`@` 与权限都已有核心语义实现，UI 只应提供候选和交互适配。
- 原 Desktop 启动目录固定，输入框没有候选列表、补全状态或消息复制。

## Material Unknowns and Resolutions

| Unknown | Impact | Resolution |
|---|---:|---|
| 大项目每次按键扫描是否可接受 | High | 启动/切换时预索引，按键只搜索内存；最多 20,000 文件 |
| 何时开始过滤 | Medium | 至少 3 个 Unicode 字符；更短输入只显示提示 |
| Enter 应补全还是发送 | High | 有候选时只补全；无候选时发送；Shift+Enter 换行 |
| Desktop 如何切换工作区 | High | 系统目录对话框 + 进程内原子替换 Runtime；清理旧 UI session |
| 切换时存在审批怎么办 | High | 默认拒绝所有旧 Runtime pending approval |
| Terminal/Desktop 是否各自实现搜索 | High | 否；共用 `ContextCandidateIndex` 和序列化搜索结果 |

## Verification Plan

- Core：大结果集、Git/敏感路径排除、3 字符阈值、文件夹候选和模糊排序。
- Terminal：候选补全不发送、复制、极小/极大 resize。
- Desktop：纯函数补全测试、Controller 工作区切换测试、Tauri 配置/索引测试。
- E2E：完整 workspace 测试、前端 build、真实 DeepSeek 请求。
