# P20 Collaborative Agent Platform 实现说明

## 范围

实现 Agent 从个人资产到团队资产的协作层：Project membership、共享 Agent/Workflow 引用、Task、Review/Approval、Knowledge、Activity Stream/Notification，以及 Desktop Collaboration Workspace。

## Collaboration Runtime

- 新增独立 `core-agent-collaboration` 进程内原子 Manager。
- `TeamProject` 管理 Owner/Maintainer/Reviewer/Member/Viewer、共享 Agent/Workflow ID，不复制资产配置。
- Task 支持 Open/Running/Paused/Review/Completed/Failed、进度、Agent owner、assignee/reviewer 和显式状态迁移。
- Review request 会原子把 Task 进入 Review；Approve 原子完成 Task，Reject 返回 Running。
- 审批者必须为 Owner/Maintainer/Reviewer，Task 创建者不得自我审批。
- Knowledge 是项目级版本资产摘要；正文和索引仍归 Knowledge/Memory 服务端。

## Activity Stream

- 每次 Project/Member/Task/Review/Knowledge 变更在同一写锁内生成不可变 Activity。
- `event_key` 全局幂等，避免 Multi-Agent/Event 重投产生重复团队事实。
- Activity audience 来自项目成员；Notification 是按 actor membership/audience 过滤的视图。
- 根组合层 `MultiAgentProjectActivityObserver` 把 P11 Outcome 事件投影到共享项目 Activity。

## Collaboration UI

- Desktop 新增 Collaboration Workspace：Home、Projects、Agents、Team、Tasks、Reviews、Approvals、Knowledge、Activity、Notifications。
- Home 以“今天 Agent 团队发生什么”为入口，展示任务/Agent/Review/Knowledge 指标、Activity Stream 与待审批队列。
- Review/Approval 提供真实 API 决策动作；服务端继续负责 actor、角色、自我审批和 Policy 校验。
- API 响应限制 2 MiB；离线与各中心空态不使用伪团队数据。

## 测试覆盖

- Runtime E2E：完整 Project → Task → Review → Approve、Reject 恢复、自我审批拒绝、外部 Activity 幂等/成员校验。
- 跨 Runtime：P11 Multi-Agent Outcome → Collaboration Activity Stream。
- Vue：首项目选择、审批后刷新、失败保留快照与十个 Collaboration section。
- 统一验证在所有剩余 P 后运行。

## 已知边界

- Manager 当前进程内，不是 durable server store；Desktop SQLite 不保存团队数据。
- actor 尚未绑定认证主体；企业 IAM、多租户、Compliance 和外部通知留给 P5。
