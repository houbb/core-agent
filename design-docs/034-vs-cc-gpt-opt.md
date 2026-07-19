# 目标

对标一下 chatGPT 的命令行/桌面端

你认为有哪些差距？

学习 chatGPT 比较优秀的地方，列举出来一起特性+实现的优先级+原因

我确定之后你开始实现

## 内置 TOOLS 能力

对标一下 chatGPT/claude-code

将其中常见的比较核心的 tools 能力，逐个实现（按照优先级）


## 内置 MEMORY

对标一下 chatGPT/claude-code

看一下全局 AGENTS.md（规范支持），对应的 skills 支持

对应的项目级别、session 级别，所有的会话等信息，memory 是如何处理的。

memory 非常重要，只有处理好，才有可能更好的实现功能。

## 核心能力

```
Agent

↓

Planning

↓

Context

↓

Memory

↓

Tool

↓

Workspace

↓

Execution

↓

Permission

↓

Observation

↓

Plugin
```

看一下完整的这个链路，还有哪些和企业级 claude-code/chatGPT 存在差距。

真实的实现+打通+端到端测试验证。

## 实现状态（2026-07-19）

已按“核心闭环 → Hooks/MCP/Web 扩展 → sandbox/background/sub-agent/集中策略”分阶段实现：

- `AGENTS.md`/`AGENTS.override.md` 全局与项目指令链，带优先级、SHA-256、UTF-8/体积/符号链接边界。
- system/user/project Skills 发现与 metadata 常驻，完整 `SKILL.md` 通过 `load_skill` 按需加载并做 TOCTOU 校验。
- 独立 SQLite Memory：project/session namespace、自动相关召回、`remember_memory/recall_memory/forget_memory`、secret-like 内容拒绝。
- 本地 `find_files/search_files/apply_patch`，精确替换、current SHA-256、歧义拒绝并接入现有 Checkpoint。
- `run_command` 结构化 stdout/stderr/exit code、流式观察、超时、取消、输出上限、敏感环境剥离和进程树终止；`start/poll/cancel_command` 支持后台执行。
- 可配置 OpenAI Responses Web Search provider，以及带来源 URL、域名策略、逐跳重定向/DNS 私网校验的 `web_search/web_fetch`。
- 显式启用的生命周期 Hooks 与 MCP stdio initialize/tools/list/tools/call；均进入统一 Tool Permission，MCP server 受 allowlist 和环境变量名白名单约束。
- bubblewrap 可用性探测、best-effort/required/fail-closed 策略；Windows 当前无原生 backend，不把路径检查描述成 OS sandbox。
- `delegate_task` 使用独立模型上下文和最多四轮只读 Tool loop；managed policy 可集中禁用工具类别、Web、Memory、MCP、Hooks 或强制 sandbox。

实现记录、未决风险和验证证据见：

- `design-docs/034-vs-cc-gpt-opt-unknowns-report.md`
- `design-docs/034-vs-cc-gpt-opt-implementation-notes.md`

