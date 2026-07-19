# core-agent 能力可追溯矩阵

> 日期：2026-07-19  
> 范围：设计文档 000～015、020～028，23 个 Rust package、CLI、Tauri/Vue Desktop  
> 判定：类型存在为“实现”；相邻 Runtime 有真实断言为“集成”；从用户入口贯穿持久化主链为“产品闭环”。

## 当前结论

- 统一入口已完成：Terminal 和 Desktop 默认直接加载同一个进程内 `EnterpriseAgent`。
- `core-agent-*` 是内部 Rust crate，不是需要单独启动的进程或子 Agent。
- 核心请求主链已打通：Session → Context → Model → 有界 Tool 循环 → HITL 审批 → Session/Event。
- 工作区具备受控列举、读取、乐观并发写入和命令执行；默认风险模式在编辑/风险命令前等待人的一次性批准。
- 全部 Runtime 均由组合根实例化；Planning→Execution→Tool、Agent→Planning/Execution、Kernel→Platform、Protocol→Kernel/Visual/Ecosystem 等既有适配器继续复用。
- 本轮产品端到端闭环覆盖单进程组合、持久化会话/上下文、可替换模型 Provider、真实 Tool Runtime、CLI bridge 和 Tauri bridge。

## Runtime 与产品矩阵

| 设计 | 实现载体 | 统一入口状态 | 证据/边界 |
|---|---|---|---|
| P0 Session | `core-agent-session` | 主链已接入 | SQLite 会话、消息终态、取消/恢复 E2E |
| P1 Context | `core-agent-context` | 主链已接入 | 从 Session 构建并持久化快照 |
| P2 Model | `core-agent-model` | 主链已接入 | OpenAI-compatible tool schema/correlation；确定性测试和真实 DeepSeek opt-in E2E |
| P3 Tool | `core-agent-tool` | 主链已接入 | 最多 8 轮 Model↔Tool 回填；失败保持 Agent 终态失败 |
| P4 Workspace | `core-agent-workspace` + 内置工具 | 产品闭环 | 持久化索引；受控 list/read/write/command，路径和体积 fail-closed |
| P5 Planning | `core-agent-plan` | 内部已连接 | Planning→Execution ToolActionExecutor |
| P6 Execution | `core-agent-execution` | 内部已连接 | Execution→Tool 跨 Runtime 测试 |
| P7 Agent | `core-agent-agent` | 内部已连接 | Agent→Planning/Execution coordinator |
| P8 Memory | `core-agent-memory` | 已实例化 | Memory→Context/Event 已有跨 Runtime 测试；主聊天自动记忆仍待增强 |
| P9 Event | `core-agent-event` | 已实例化 | Event→Memory 已有跨 Runtime 测试；统一主链同时发布产品事件 |
| P10 Workflow | `core-agent-workflow` | 内部已连接 | Workflow→Planning/Execution |
| P11 Multi-Agent | `core-agent-multi` | 已实例化 | Multi-Agent→Agent/Planning/Execution/Tool |
| P12 Extension | `core-agent-extension` | 已实例化 | Extension→Tool；默认权限 fail-closed |
| P13 Platform | `core-agent-platform` | Kernel 已启动 | 本地 tenant/org/policy seed；生产认证入口仍需外部身份提供方 |
| P14 Kernel | `core-agent-kernel` | 组合根启动 | 当前管理 Platform 生命周期；其余无生命周期 trait 的 Runtime 由组合根持有 |
| P15 Product Contract | `core-agent-app` | 已导出 | readiness 单元断言 |
| P16 CLI MVP | `agent-cli` | 默认 embedded | run/chat/status/sessions/resume/cancel 直连组合根；交互 HITL/非交互拒绝 |
| P17 Professional CLI | `agent-cli` | 默认 embedded | project/tools/tasks 读取 Runtime，其余智能命令进入统一主链 |
| P18 Desktop | `agent-desktop` | Tauri embedded | Console 直连组合根；原生审批事件/一次性决定/超时拒绝 |
| P19 Studio | `core-agent-visual` + Vue | 本地 bridge | 列表和 Agent 创建已接真实 Runtime；其余写操作继续收口 |
| P20 Collaboration | `core-agent-collaboration` + Vue | 本地 bridge | 本地项目/成员/任务/活动读取 |
| P21 Enterprise | `core-agent-governance` + Platform + Vue | 本地 bridge | 组织/策略/成本/审计读取；完整 IAM/durable governance 未宣称完成 |
| P22 Ecosystem | `core-agent-ecosystem` + Vue | 本地 bridge | Publisher/Package catalog 读取；artifact 分发不是本轮范围 |
| P23 Protocol | `core-agent-protocol` | 已注册 | Kernel/Visual/Ecosystem descriptors 可发现 |

## 用户可见运行形态

```text
Terminal ─┐
          ├─ EnterpriseAgent
Desktop ──┘   ├─ 持久化：Session / Context / Model / Workspace
              ├─ 主链：Context → Model ↔ Tool → HITL
              └─ 内部模块：Planning / Execution / Agent / Memory / Event /
                 Workflow / Multi / Extension / Platform / Kernel / Visual /
                 Collaboration / Governance / Ecosystem / Protocol
```

本地默认没有 `agent-server` 启动步骤。CLI 的 `remote` 模式仅是兼容边界；Desktop 的 HTTP 类也仅用于显式远程适配。

## 尚未过度声明的企业级边界

以下能力已有领域模型，但在宣称生产级企业闭环前仍需继续增强：

- 外部 IAM/OIDC 与可信 actor/tenant 注入。
- OS/容器级 Shell、File、Network 隔离；当前为工作区路径、敏感名、超时、输出和审批边界，不等同于 sandbox。
- Collaboration/Governance/Ecosystem 的完整 durable store 和迁移验证。
- Memory 自动写入/召回主聊天链。
- 集群、高可用、遥测后端、制品签名与远程协议兼容性测试。

这些边界不影响本地“一个入口、一个进程、全部 Runtime 可直接共同使用”的目标，但会影响对生产安全和高可用的等级声明。
