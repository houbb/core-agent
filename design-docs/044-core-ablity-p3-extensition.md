# Core-Agent P3 设计

## P3：Agent Extension Runtime（能力生态层）

模块：

```text
core-agent-mcp
core-agent-plugin
core-agent-skill
core-agent-slash
```

---

# 一、P3 目标

P0：

```text
Agent 会运行
```

P1：

```text
Agent 会规划
```

P2：

```text
Agent 会协作
```

P3：

```text
Agent 会扩展能力
```

---

核心思想：

> Agent 本身不应该无限增加内置能力，而应该通过标准化扩展机制连接外部世界。

类似：

* Claude Code → MCP + Skills
* OpenCode → Plugin + Tool
* ChatGPT → GPTs + Actions
* VS Code → Extension

最终：

```text
                 core-agent


                     |

             Extension Runtime


 -------------------------------------------------


 MCP              Plugin          Skill        Slash


 |                  |              |             |


External          Runtime       Workflow      Command

Systems           Extension     Capability    UX Entry


 -------------------------------------------------


                     |

                    Agent

```

---

# 二、P3 模块关系

```text
                    Agent


                      |

                      |


              Capability Resolver


                      |


 ------------------------------------------------


 MCP Server      Plugin       Skill       Slash


                      |


                 Tool Runtime


                      |

                  Execution

```

---

# 三、core-agent-mcp ⭐⭐⭐⭐⭐

## 定位

外部能力连接协议。

MCP 是目前 Agent 生态最重要标准之一。

目标：

让 Agent 可以访问：

```text
数据库

GitHub

Jira

Grafana

CMDB

文件系统

浏览器

API

```

---

# 1. MCP Runtime

核心对象：

```java
McpServer {


 id;


 name;


 endpoint;


 tools[];


 resources[];


 status;


}
```

---

例如：

注册：

```yaml
mcp:

 servers:

  github:

    url:

    tools:

      - issue.search

      - repo.read

```

---

# 2. MCP Tool Discovery

启动：

```text
Agent

 |

MCP Registry

 |

发现能力


 |

加载 Tool


```

例如：

发现：

```text
github.search

github.create_issue

github.pull_request

```

---

# 3. MCP Resource

不仅 Tool。

支持：

```text
Resource


 |

文件

数据

文档

配置

```

例如：

```text
cmdb://service/order

grafana://dashboard/payment

```

---

# 4. MCP Permission

必须结合 P0：

```text
Agent

 |

MCP Tool

 |

Permission

 |

Execute

```

---

# UX

Desktop：

## MCP 管理

```text
MCP Servers


✓ GitHub


 Status:

 Connected


 Tools:

  repo.read

  issue.create



✓ Grafana


 Tools:

  dashboard.query


```

---

# 注意点

不要：

```text
Agent 直接连接 MCP
```

应该：

```text
Agent

 |

core-agent-mcp-runtime

 |

MCP Server

```

方便：

* 权限
* 审计
* 统计
* 替换

---

---

# 四、core-agent-plugin ⭐⭐⭐⭐⭐

## 定位

Agent 平台插件系统。

类似：

* VS Code Extension
* IntelliJ Plugin
* OpenCode Plugin

---

# 为什么需要 Plugin？

MCP：

解决：

> 外部能力连接

Plugin：

解决：

> Agent 平台自身扩展

---

例如：

RCA Plugin：

```text
新增:

RCA Agent

RCA Tools

RCA Dashboard

RCA Workflow

```

---

Trading Plugin：

```text
新增:

Market Agent

Strategy Skill

Data Tool

```

---

# Plugin Model

```java
Plugin {


 id;


 name;


 version;


 author;


 permissions;


 tools[];


 skills[];


 agents[];


}
```

---

# Plugin 生命周期

```text
INSTALL

 |

LOAD

 |

ENABLE

 |

RUN

 |

DISABLE

 |

REMOVE

```

---

# Plugin Package

例如：

```text
rca-plugin.zip


├── manifest.json

├── tools

├── skills

├── agents

├── resources

```

---

manifest：

```json
{
"name":"RCA",

"version":"1.0",

"tools":[
"log.query"
],

"agents":[
"RCA-Agent"
]

}
```

---

# Plugin Registry

类似：

```text
npm registry

VS Marketplace

GPT Store

```

---

# UX

Desktop：

插件中心：

```text
Marketplace


Installed:


✓ RCA Assistant

✓ Java Developer


Available:


+ Kubernetes Expert

+ Security Scanner


```

---

# 注意点

插件必须隔离：

```text
Plugin

 |

Sandbox

 |

Runtime
```

不要：

```text
plugin.jar

直接运行
```

---

---

# 五、core-agent-skill ⭐⭐⭐⭐⭐

## 定位

技能层。

这是很多 Agent 平台缺少的。

区别：

## Tool

能力：

```text
查询数据库
```

---

## Skill

经验：

```text
如何排查数据库慢查询
```

---

关系：

```text
Skill

 |

多个 Tool

 |

完成任务

```

---

# Skill Model

```java
Skill {


 id;


 name;


 description;


 instructions;


 tools[];


 examples;


}
```

---

# 示例

## RCA Skill

```yaml
name:

database-slow-query-analysis


steps:

1. 查询慢SQL

2. 分析执行计划

3. 检查指标

4. 输出结论


tools:

- sql.query

- metric.query

```

---

# Skill Runtime

执行：

```text
Agent

 |

Skill Resolver


 |

Load Skill


 |

Execute Workflow


```

---

# Skill 分类

```text
Built-in Skill

Community Skill

Enterprise Skill

Personal Skill

```

---

# UX

Agent 输入：

```
分析订单接口慢
```

自动：

```text
Detected Skill:


Database Performance Analysis


Load?

[Yes]

```

---

# 注意点

Skill 不要写死。

应该：

```text
Markdown

YAML

JSON

DSL

```

---

---

# 六、core-agent-slash ⭐⭐⭐⭐

## 定位

用户快捷操作入口。

类似：

Claude Code：

```text
/review

/explain

/test
```

---

# Slash Command Model

```java
SlashCommand {


name;


description;


prompt;


skill;


permission;


}
```

---

例如：

```text
/review


调用:

Code Review Skill

```

---

内置：

```text
/explain

/review

/refactor

/test

/debug

/plan

/commit

```

---

# Slash 执行

```text
User:

/review UserService


       |


Command Parser


       |


Skill


       |


Agent


```

---

# UX

输入框：

```text
/


显示：

/review

/debug

/test

/explain


```

类似 IDE Command Palette。

---

# 七、四者关系

非常重要：

```text
                 User


                  |


              Slash


                  |


                 Skill


                  |


                 Tool


                  |


                 MCP


                  |


              External World

```

---

例如：

用户：

```
/analyze payment timeout
```

流程：

```text
Slash:

/analyze


↓

Skill:

RCA Analysis


↓

Tools:

log.query

trace.query

metric.query


↓

MCP:

Grafana

ELK


↓

Agent Result

```

---

# 八、P3 数据模型

## Extension

```java
Extension {


id;


type;


name;


version;


status;


}
```

---

type：

```text
MCP

PLUGIN

SKILL

SLASH

```

---

# 九、Repo 设计

保持：

```text
core-agent


├── core-agent-runtime

├── core-agent-planner

├── core-agent-task

├── core-agent-question

├── core-agent-todo

├── core-agent-reflection


├── core-agent-subagent

├── core-agent-message

├── core-agent-orchestrator


├── core-agent-mcp

├── core-agent-plugin

├── core-agent-skill

├── core-agent-slash

```

---

# 十、P3 MVP 范围

不要一开始做 Marketplace。

第一阶段：

## core-agent-mcp

实现：

```text
MCP Client

Tool Discovery

Permission

Execution

```

---

## core-agent-plugin

实现：

```text
Plugin Manifest

Install

Enable

Disable

```

---

## core-agent-skill

实现：

```text
Skill Definition

Skill Loader

Skill Executor

```

---

## core-agent-slash

实现：

```text
Command Registry

Parser

Executor

```

---

# 十一、P3 完成后的能力

你的 Agent 体系：

P0：

```text
Agent Runtime
```

↓

P1：

```text
Planning Agent
```

↓

P2：

```text
Multi-Agent System
```

↓

P3：

```text
Agent Ecosystem Runtime
```

达到：

```text
任何人可以:

开发 Plugin

提供 MCP

发布 Skill

创建 Slash Command


扩展 Agent 能力

```

---

# 十二、和 Claude Code / OpenCode / Codex 对比

| 能力                   | Claude Code | OpenCode | Codex | 你的设计  |
| -------------------- | ----------- | -------- | ----- | ----- |
| MCP                  | ✅           | ✅        | 部分    | ✅     |
| Plugin               | 弱           | ✅        | 弱     | ⭐⭐⭐⭐⭐ |
| Skill                | ✅           | 弱        | 弱     | ⭐⭐⭐⭐⭐ |
| Slash                | ✅           | ✅        | 弱     | ⭐⭐⭐⭐  |
| Marketplace          | 弱           | 弱        | 弱     | 后续    |
| Enterprise Extension | 弱           | 弱        | 弱     | ⭐⭐⭐⭐⭐ |

---

P3 完成后，下一阶段 P4 应该进入：

```text
core-agent-workflow
core-agent-approval
core-agent-audit
core-agent-observability
core-agent-cost
```

也就是：

**Agent Enterprise Governance Runtime（企业治理层）**。
