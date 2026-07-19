# P032 桌面端统一工作区体验 — Implementation Notes

## Implemented

- 核心 `ContextCandidateIndex` 优先通过 ripgrep 建立有界、git-aware 的文件/文件夹索引，失败时回退安全文件系统遍历；搜索阈值和模糊排序由核心统一实现。
- Tauri 暴露共享候选查询，并用可替换的 `DesktopRuntime` 状态支持进程内工作区切换；旧审批默认拒绝。
- Vue Console 增加 `/`/`@` 候选、键盘补全、项目树 `Add @`、用户/Agent 消息复制和系统目录选择器。
- 输入补全拆为纯函数模块，避免把命令语义或文件扫描复制到组件。

## Verification evidence

- Vue/Vitest：8 个文件、20 个测试通过。
- `vue-tsc --noEmit` + Vite production build：通过；npm audit：0 vulnerability。
- Tauri Rust 单元/E2E、完整 Cargo workspace 和零警告 Clippy：通过。
- 真实 DeepSeek 两个核心 E2E + 全局配置 Terminal E2E：3/3 通过。
- 最新 Desktop 可执行文件已启动为 `AgentOS Workspace` 原生窗口并保持响应。

## Three-pass review

1. 架构：Desktop 只查询核心索引/命令，工作区切换替换单个嵌入式 Runtime。
2. 并发/安全：工作区切换与请求串行；切换窗口暂停并清空旧审批，敏感索引路径 fail-closed。
3. UX/回归：补全不发送、发送原文可见、复制、`Add @`、离线/切换失败保留旧连接状态均完成断言或构建验证。

## Remaining boundary

- 首版索引在打开工作区时建立，不监听文件系统增量变化；新建文件在重新打开/刷新 Runtime 后进入候选，但最终显式路径仍可直接使用。
