# P17 Professional CLI Unknowns Report

## Scope

在 P16 官方 CLI 上增加 Project Snapshot/Index、统一 slash Command Registry、Profile、Review/Plan/Explain/Tasks/History/Tools/Memory API、有限终端命令历史与专业状态头。核心智能仍在 core-agent 服务端，CLI 只采集项目 manifest/Git 元数据和呈现结果。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | Project Intelligence 放在哪里 | CLI 只做有界 marker/manifest/Git HEAD 采集；项目图、Review、History、Memory 分析通过 `ProfessionalAgentClient` 委托服务端 |
| P0 | 当前无 Professional API Server | 实现明确 HTTP 合同和可替换 trait；E2E 用 Mock Client，连接失败不降级为伪分析 |
| P0 | 命令插件扩展 | `CommandRegistry` 使用稳定 definition/parser/completion/help；内置命令与未来扩展共享同一注册冲突规则 |
| P0 | Profile 如何影响请求 | Profile 是项目级本地选择，作为显式字段随 Professional request 发送，不隐式改写用户 Prompt |
| P1 | 命令历史隐私 | 默认只记录 slash command，不记录普通 Prompt；上限 500，原子写入，拒绝控制字符 |
| P1 | Git 采集是否执行 shell | MVP 只安全读取 `.git/HEAD`，不执行 Git 命令、不读取 diff 正文；Review 由服务端 Workspace/Tool 执行 |
| P1 | Project 扫描范围 | 只检查根级受支持 marker 和最多 128 个直接子目录，不递归读取源码，不上传文件内容 |
| P1 | Task Runtime 范围 | 本 P 只提供任务查询/呈现 Client，不在 CLI 新建第二套 Task 状态机 |

## Acceptance

- Project Snapshot 可确定识别主要语言、框架、构建工具、Git 分支、模块候选且有界。
- Slash/top-level 命令使用同一 Registry；重复注册、未知命令和不安全参数显式失败。
- Profile 持久化、命令历史隐私边界、Professional API 请求与输出有测试。
- Review/Plan/History/Tasks/Tools/Memory 均通过 Client，不在 CLI 伪造结果。

## Residual risks

- 服务端 Professional API 尚待后续应用层实现。
- 仅 marker 的本地扫描不能替代 AST/Symbol/Reference Index；那些能力属于服务端 Project Intelligence。
