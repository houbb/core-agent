# 目标

我们的 tools 内置对齐 cc

## 实现方式

全部插件化实现

接口实现+方便拓展替换

# cc

Claude Code 和 OpenCode 的设计理念非常接近：**给 LLM 一个受控的开发环境操作能力（Agent Tool Runtime）**。不过 Claude Code 在工具体系上更偏向“企业级 Agent 平台”，增加了 **Agent、多代理、计划模式、Worktree、安全控制、MCP 扩展** 等能力。([Claude][1])

整体可以抽象：

```
                 Claude Code Agent

                         |
                         |
                Tool Calling Runtime

                         |
 --------------------------------------------------

   理解代码       修改代码       执行环境       协作能力

      |              |              |              |

    Read           Edit           Bash           Agent
    Glob           Write          Web            Plan
    Grep           Notebook       Git            Worktree
    LSP                            Cron
                                   MCP

 --------------------------------------------------

                  Developer Machine
```

([Claude][1])

---

# 1. Bash ⭐⭐⭐⭐⭐

## 定位

Claude Code 最核心工具。

能力：

> 操作真实操作系统。

例如：

```bash
npm test

mvn clean package

cargo build

docker compose up

git diff

kubectl logs
```

对应：

```
AI
 |
Bash Tool
 |
Shell
 |
OS
```

用途：

* 编译
* 测试
* 部署
* 查看日志
* Git 操作
* 环境诊断

和 OpenCode 类似：

```
Claude Code Bash
        ≈
OpenCode bash
```

([Claude][1])

---

# 2. Read ⭐⭐⭐⭐⭐

代码阅读。

例如：

```
Read:

src/UserService.java
```

返回：

```java
class UserService {

}
```

用途：

* 理解代码
* 分析架构
* 找 Bug

---

典型：

```
Read
 |
理解
 |
Edit
```

---

# 3. Write ⭐⭐⭐⭐⭐

创建文件。

例如：

```
Write:

UserController.java
```

生成：

```java
@RestController
class UserController {

}
```

用途：

* 新建模块
* 创建测试
* 生成配置

([Claude Platform][2])

---

# 4. Edit ⭐⭐⭐⭐⭐

精确修改。

Claude Code 的 Edit 是：

```
old_string

替换

new_string
```

不是：

```
重新生成整个文件
```

所以安全性更高。

例如：

原：

```java
return user;
```

修改：

```java
return Optional.of(user);
```

([Claude][3])

---

# 5. Glob ⭐⭐⭐⭐

文件发现。

类似：

```
find
```

例如：

寻找：

```
**/*.java
```

返回：

```
src/
 ├ User.java
 ├ Order.java
 └ Payment.java
```

用途：

项目扫描。

([Claude][3])

---

# 6. Grep ⭐⭐⭐⭐⭐

代码搜索。

类似：

```
ripgrep
```

例如：

寻找：

```
calculatePrice
```

返回：

```
OrderService.java

PaymentService.java
```

用途：

建立：

```
代码知识图谱
```

([Claude][3])

---

# 7. LSP ⭐⭐⭐⭐⭐

这个是 Claude Code 比很多 Agent 强的地方。

LSP = Language Server Protocol

能力：

## 跳转定义

```
UserService
      |
      v
UserService.java
```

## 查找引用

```
谁调用这个方法？
```

## 类型分析

例如：

```
user.getName()

user 类型?
```

---

本质：

把 IDE 的能力暴露给 Agent。

([Claude][3])

---

# 8. Agent ⭐⭐⭐⭐⭐（非常关键）

这是 Claude Code 一个重要区别。

Claude 可以创建子 Agent：

```
Main Agent

      |
      |
 -----------------

 |       |        |

代码分析  测试    文档

Agent    Agent   Agent
```

例如：

用户：

> 重构支付系统

主 Agent：

```
Agent 1:
分析数据库

Agent 2:
分析接口

Agent 3:
写测试
```

最后汇总。

([Claude][1])

---

这个能力对应你之前设计：

```
core-agent

    |
    +-- agent-runtime

    +-- agent-worker

    +-- agent-orchestrator
```

---

# 9. Plan Mode ⭐⭐⭐⭐⭐

非常重要。

普通模式：

```
需求

↓

直接改代码
```

Plan Mode：

```
需求

↓

分析

↓

生成方案

↓

用户确认

↓

执行
```

类似：

软件架构师模式。

例如：

```
我要把 MySQL 换 PostgreSQL
```

Agent：

```
Step1:
分析 ORM

Step2:
修改 schema

Step3:
迁移数据

Step4:
测试

等待确认
```

---

对应：

企业 Agent 必备。

([Claude][1])

---

# 10. Worktree ⭐⭐⭐⭐

Git 隔离开发。

传统：

```
main branch

修改代码
```

风险：

污染当前环境。

Worktree:

```
main

 |
 |
 +--- agent branch

       修改

       测试
```

适合：

多个 Agent 并行。

([Claude][1])

---

# 11. WebSearch ⭐⭐⭐⭐

互联网搜索。

例如：

```
Spring Boot 4 migration
```

获取：

* 官方文档
* issue
* 解决方案

---

# 12. WebFetch ⭐⭐⭐⭐

读取网页。

例如：

```
fetch:

spring.io/docs
```

然后：

分析文档。

([Claude Platform][2])

---

# 13. AskUserQuestion ⭐⭐⭐⭐

人机协作。

例如：

Agent：

```
发现设计冲突：

方案 A:
Redis Cache

方案 B:
SQLite

请选择
```

避免 AI 自己乱决策。

([Claude][1])

---

# 14. NotebookEdit ⭐⭐⭐

针对：

Jupyter Notebook。

能力：

修改：

```
.ipynb
```

例如：

数据分析：

```
读取数据

↓

修改 cell

↓

运行
```

([Claude][3])

---

# 15. Cron ⭐⭐⭐

定时任务。

例如：

```
每天 8 点：

检查项目依赖漏洞
```

Claude 可以创建 session 内计划任务。([Claude][1])

---

# 16. MCP（扩展能力）⭐⭐⭐⭐⭐

这是 Claude Code 生态核心。

MCP：

Model Context Protocol

可以接入：

```
Claude Code

      |
      |
      MCP Server

 ----------------------

 GitHub

 Jira

 PostgreSQL

 Kubernetes

 Slack

 内部系统
```

([Claude Help Center][4])

---

# Claude Code Tool 总表

| Tool            | 能力       | 定位         |
| --------------- | -------- | ---------- |
| Bash            | 执行命令     | 操作系统       |
| Read            | 读取文件     | 理解代码       |
| Write           | 创建文件     | 生成代码       |
| Edit            | 修改代码     | 精准重构       |
| Glob            | 文件发现     | 项目扫描       |
| Grep            | 全文搜索     | 代码检索       |
| LSP             | 代码智能     | IDE能力      |
| Agent           | 子代理      | 并行任务       |
| Plan Mode       | 规划       | 架构设计       |
| Worktree        | 隔离环境     | 安全开发       |
| WebSearch       | 搜索       | 知识获取       |
| WebFetch        | 网页读取     | 文档学习       |
| AskUserQuestion | 询问       | Human Loop |
| NotebookEdit    | Notebook | 数据科学       |
| Cron            | 调度       | 自动任务       |
| MCP             | 外部系统     | 生态扩展       |

([Claude][1])

---

# 和 OpenCode 对比

| 能力        | OpenCode | Claude Code |
| --------- | -------- | ----------- |
| Shell     | ✅        | ✅           |
| 文件读写      | ✅        | ✅           |
| 搜索        | ✅        | ✅           |
| LSP       | ✅        | ✅           |
| Web       | ✅        | ✅           |
| Plan      | 弱        | ⭐⭐⭐⭐⭐       |
| Sub Agent | 弱        | ⭐⭐⭐⭐⭐       |
| Worktree  | 弱        | ⭐⭐⭐⭐        |
| MCP       | ✅        | ⭐⭐⭐⭐⭐       |
| 企业扩展      | 一般       | 强           |

---

结合你之前设计的 **core-agent**，Claude Code 更像：

```
                    core-agent

                         |

                 Agent Runtime

                         |

 ------------------------------------------------

 Tool Runtime

 file.*
 shell.*
 code.*
 workflow.*
 browser.*
 database.*
 cmdb.*
 monitoring.*
 kubernetes.*
 incident.*

 ------------------------------------------------

 Agent Orchestrator

 Planner
 Worker
 Reviewer
 Executor

 ------------------------------------------------

 Enterprise MCP Layer

 GitHub
 Jira
 CMDB
 NOC
 Grafana
 Prometheus

```

所以如果你的目标是做 **企业级 AI 运维 Agent / RCA Agent 平台**，Claude Code 的 Tool + Agent + MCP + Worktree + Plan 这一套架构非常值得直接借鉴。你之前规划的：

```
core-agent
core-workflow
core-plugin
core-openapi
core-ai
```

实际上已经接近 Claude Code 的企业化版本。

[1]: https://code.claude.com/docs/en/tools-reference?utm_source=chatgpt.com "Tools reference - Claude Code Docs"
[2]: https://platform.claude.com/docs/en/managed-agents/tools?utm_source=chatgpt.com "Tools - Claude Platform Docs"
[3]: https://code.claude.com/docs/zh-CN/tools-reference?utm_source=chatgpt.com "工具参考 - Claude Code Docs"
[4]: https://support.claude.com/en/articles/14553413-claude-code-cheatsheet?utm_source=chatgpt.com "Claude Code cheatsheet | Claude Help Center"
