看一下下面的 tools 增强，进行对应的补充+更多在 claude-code/codex/opencode 之外的 tools 增强（按照价值排序）  # 目标

内置更多更加强大的内置 tools

这些 tools 能力是 terminal 和 desktop 可以公用的，底层是同一套实现。

# 已有的 tools

将已有的 tools 进行梳理，梳理放在 D:\_ai_core_plateform\core-agent\docs

## desktop 页面查看/管理

tools 的配置是否启用，从配置文件加载，内置默认全部启用。

支持 desktop 设置页面-tools 管理

底层使用配置文件（我们 core-agent 全局的配置文件，以前放大模型配置等信息的），保持 desktop 和 terminal 可以一致性。

# 新增 tools

看一下，尽量使用开源成熟方案。

## AST

支持 ast 语法树的查询+替换

ast-grep-replace
ast-grep-search

## 查找

grep + grep 增强 ripgrep

glob 增强+文件路径匹配等

## 编辑

hashlie-edit 内容哈希的编辑，避免行号变化带来的定位苦难问题

## 反编译

maven-dependency-inspect

本地依赖包的类+方法

这个是针对 java 的，可以多实现一些常用的。

# 详细设计

你的方向是正确的：不要简单复制 Claude Code / Codex / OpenCode 的 tools，而是建设一个 **通用 Agent Tool Runtime**。

核心原则：

> Terminal Agent 和 Desktop Agent 不应该各自实现 tools，而应该共享一个 `core-agent-tool-runtime`。

架构：

```text
                 core-agent

                     |
          core-agent-tool-runtime

                     |

 -------------------------------------------------

 File Tools       Code Tools       System Tools

 Knowledge        Dev Tools        Enterprise Tools


                     |

 -------------------------------------------------

 Terminal UI                 Desktop UI

```

Desktop 只是：

```
Tool Registry
      |
      |
UI 展示 / 开关 / 参数配置
```

真正执行：

```
Tool Runtime
```

---

# Tool 总体分级

按照价值排序：

```
P0 基础开发工具（必须）
P1 代码智能工具（核心差异）
P2 工程理解工具
P3 系统运维工具
P4 企业 Agent 工具
P5 高级 AI 工具
```

---

# P0：基础工具 Runtime（已有 + 补充）

## 1. file.read ⭐⭐⭐⭐⭐

已有 Claude/Codex 基础能力。

增强：

支持：

```
read file

read range

read symbol

read class

read method
```

例如：

```
UserService.java

method:
createUser()
```

直接返回方法。

---

## 2. file.write ⭐⭐⭐⭐⭐

增强：

支持：

* create
* overwrite
* append
* template

---

## 3. file.edit ⭐⭐⭐⭐⭐

你提出：

## hashline-edit

非常值得加入。

传统：

```
line 100-120 replace
```

问题：

代码变化后：

```
line offset
```

失效。

hash edit：

例如：

```
目标:

hash:
8f92aa


replace:

old

new
```

类似：

```
Aider
```

采用的思想。

建议：

名称：

```
core-tool-file-patch
```

支持：

```
line patch

hash patch

ast patch
```

---

## 4. grep ⭐⭐⭐⭐⭐

采用：

```
ripgrep
```

增强：

支持：

```
regex

language filter

ignore gitignore

symbol search
```

例如：

```
grep:

method:
login

language:
java
```

---

## 5. glob ⭐⭐⭐⭐⭐

采用：

```
fast-glob
```

增强：

支持：

```
**

exclude

.gitignore

file size

modified time
```

---

## 6. shell.exec ⭐⭐⭐⭐⭐

Claude/Codex 核心。

增强：

不要直接执行。

设计：

```
command executor


        |

Permission


        |

Sandbox


        |

Execute
```

支持：

```
bash

powershell

python

java

docker

kubectl
```

---

# P1：代码智能工具（超过 Claude Code）

这里是你的核心竞争力。

---

# 7. AST Search ⭐⭐⭐⭐⭐

你提出：

```
ast-grep-search
```

非常好。

基于：

```
ast-grep
```

能力：

不是文本搜索：

```
find:

UserService
```

而是：

语义：

```
找到所有 Controller 中调用 Service 的地方
```

例如：

搜索：

```java
@Autowired
$SERVICE
```

返回：

所有注入。

---

# 8. AST Replace ⭐⭐⭐⭐⭐

基于：

```
ast-grep-replace
```

能力：

安全重构。

例如：

统一：

```java
logger.info()
```

替换：

```java
log.info()
```

比 grep/edit 强。

---

# 9. LSP Runtime ⭐⭐⭐⭐⭐

Claude Code 有。

必须加入。

能力：

```
goto definition

find reference

rename symbol

hover

call hierarchy
```

支持：

```
Java

Rust

Go

Python

TS

Vue
```

实现：

Language Server Protocol。

---

# 10. Symbol Index ⭐⭐⭐⭐⭐

这个很多 Agent 没有。

建立：

```
代码索引
```

类似：

IDE Index。

存储：

```
class

method

field

dependency

call graph
```

技术：

可以：

```
tree-sitter
+
sqlite
```

形成：

```
core-code-index
```

价值巨大。

---

# 11. Dependency Analyze ⭐⭐⭐⭐⭐

你提出：

maven-dependency-inspect。

扩展：

## Java

```
maven

gradle
```

能力：

查看：

```
依赖树

class来源

method来源

版本冲突
```

例如：

Agent:

```
这个 UserService 来自哪个 jar?
```

返回：

```
xxx-user-sdk.jar
```

---

扩展其他语言：

## Node

```
npm dependency inspect
```

## Rust

```
cargo tree
```

## Python

```
pipdeptree
```

---

# 12. Decompiler Tool ⭐⭐⭐⭐

Java 特别重要。

工具：

```
CFR

FernFlower

JD-Core
```

能力：

```
class

↓

source
```

Agent 可以：

```
分析闭源 jar
```

---

# P2：工程理解工具

---

# 13. Project Analyzer ⭐⭐⭐⭐⭐

非常建议。

输入：

```
项目目录
```

输出：

```
项目地图
```

例如：

```
Spring Boot

modules:

controller

service

dao

entity


入口:

Application.java
```

类似：

```
Repo Map
```

Aider 有类似思想。

---

# 14. Architecture Graph ⭐⭐⭐⭐

生成：

```
模块关系图
```

例如：

```
Controller

 |

Service

 |

Repository

 |

Database
```

输出：

```
graph json
```

---

# 15. Call Graph Tool ⭐⭐⭐⭐⭐

调用链分析。

例如：

```
login()

↓

AuthService

↓

UserDAO

↓

Mysql
```

用于：

RCA 非常重要。

---

# 16. API Analyzer ⭐⭐⭐⭐

扫描：

```
REST API

OpenAPI

Controller
```

输出：

```
接口列表

参数

权限

调用关系
```

---

# P3：Runtime / 运维工具

这部分是你优势领域。

---

# 17. Log Query Tool ⭐⭐⭐⭐⭐

连接：

```
ELK

Loki

ClickHouse
```

能力：

Agent:

```
查询订单失败日志
```

---

# 18. Metrics Tool ⭐⭐⭐⭐⭐

连接：

```
Prometheus
```

能力：

```
CPU

Memory

QPS

Latency
```

---

# 19. Trace Tool ⭐⭐⭐⭐⭐

连接：

```
Jaeger

SkyWalking

OpenTelemetry
```

能力：

链路：

```
API

↓

Service

↓

DB
```

---

# 20. CMDB Tool ⭐⭐⭐⭐⭐

你的场景必须有。

能力：

查询：

```
服务

机器

负责人

依赖
```

---

# 21. Kubernetes Tool ⭐⭐⭐⭐

封装：

```
kubectl
```

能力：

```
pod

deployment

service

logs

events
```

---

# P4：企业 Agent Tools

---

# 22. Knowledge Tool ⭐⭐⭐⭐⭐

连接：

```
Vector DB

Wiki

Markdown

Confluence
```

能力：

RAG。

---

# 23. Ticket Tool ⭐⭐⭐⭐

连接：

```
Jira

ServiceNow
```

能力：

创建：

```
bug

incident

task
```

---

# 24. Notification Tool ⭐⭐⭐⭐

连接：

```
Email

Slack

DingTalk

企业微信
```

---

# 25. Browser Tool ⭐⭐⭐⭐

类似：

Playwright。

能力：

```
打开网页

点击

截图

获取内容
```

---

# P5：高级 AI Tools

---

# 26. Vision Tool ⭐⭐⭐⭐

输入：

```
截图

架构图

日志截图
```

---

# 27. Code Review Tool ⭐⭐⭐⭐⭐

自动：

```
diff

review

security

quality
```

---

# 28. Test Generator Tool ⭐⭐⭐⭐⭐

生成：

```
unit test

integration test
```

---

# 29. Security Scan Tool ⭐⭐⭐⭐

集成：

```
Semgrep

SonarQube

Snyk
```

---

# 30. Data Tool ⭐⭐⭐⭐

连接：

```
SQL

CSV

Excel

DataFrame
```

---

# 最终推荐 Tool 清单

按照实现优先级：

## Phase 0

```
file.read
file.write
file.edit(hash)
grep(ripgrep)
glob
shell.exec
```

---

## Phase 1

```
ast-search
ast-replace
lsp
symbol-index
dependency-inspect
decompiler
```

---

## Phase 2

```
project-analyzer
architecture-graph
call-graph
api-analyzer
```

---

## Phase 3

```
log-query
metric-query
trace-query
cmdb-query
k8s-query
```

---

## Phase 4

```
knowledge-search
ticket
notification
browser
```

---

## Phase 5

```
code-review
test-generator
security-scan
data-analysis
vision
```

---

# 最终 Tool Runtime 设计

建议目录：

```
core-agent-tools

├── filesystem
│
├── shell
│
├── search
│
├── ast
│
├── lsp
│
├── code-index
│
├── dependency
│
├── decompiler
│
├── project
│
├── runtime
│
├── observability
│
├── enterprise
│
└── ai
```

---

我认为你这个方向相比 Claude Code / Codex 最大的差异点应该不是再造一个 Coding Agent，而是：

> **把 Coding Agent 的 Tool Runtime 扩展成 Enterprise Agent Tool OS。**

尤其是：

```
AST
+
Code Index
+
Dependency
+
CMDB
+
Log
+
Metric
+
Trace
```

这一套组合，正好对应你之前的 **NOC + RCA + 企业 Agent 平台**方向，这是 Claude Code / Codex 目前没有深入覆盖的领域。



