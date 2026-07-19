# P20 Collaborative Agent Platform Unknowns Report

## Scope

实现团队共享 Project、Member、Agent Registry 引用、Task、Review/Approval、Knowledge 与统一 Activity/Notification 视图，并在 Desktop 增加 Collaboration Workspace。复用 P11 Multi-Agent 和 P19 Studio 资产，不创建第二套 Agent 执行系统。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | 团队身份与授权来源 | MVP 使用 Project membership/role 做应用层授权；actor 必须显式，后续 P5 由企业 IAM/Platform 证明身份 |
| P0 | Review 与 Approval 是否独立状态机 | Approval 是 Review 的不可变决策；批准/拒绝与 Task 状态在同一锁内原子变更并生成 Activity |
| P0 | 自我审批 | 禁止 Task 创建者批准自己的 Review；审批者必须为 Owner/Maintainer/Reviewer |
| P0 | Activity 是否只是 UI 日志 | 否；Activity 是 Collaboration 层不可变、事件 key 幂等的产品事实，Notification 是按 audience 过滤的视图 |
| P1 | Agent Registry 数据所有权 | Project 只引用 Studio/P11 Agent ID，不复制 Agent 配置或 Runtime 状态 |
| P1 | Shared Workspace | 共享 Project/Task/Review/Knowledge/Activity 引用，源码和 Memory 仍由 Workspace/Memory Runtime 管理 |
| P1 | 持久化 | 本 P 先提供进程内原子 Collaboration Manager 和 API 合同；服务端 durable store 留给后续 API/部署层，不在 Desktop SQLite 保存团队数据 |
| P1 | 企业权限 | 不实现多租户/IAM/Compliance；Project role 是协作权限，不宣称企业安全边界 |

## Acceptance

- Project membership、Task 转移/进度、Review submit/approve/reject、自我审批拒绝和 Knowledge version 有测试。
- 每次业务变更原子生成一条幂等 Activity，Notification 可按 audience 查询。
- P11 Multi-Agent outcome 可通过根组合层写入项目 Activity。
- Collaboration Workspace 可呈现 Projects/Agents/Team/Tasks/Reviews/Approvals/Knowledge/Trace/Notifications。

## Residual risks

- 进程内 Collaboration 状态重启丢失，真实服务端需要 durable store。
- actor 字符串尚未绑定认证主体，不能作为企业 IAM 证据。
