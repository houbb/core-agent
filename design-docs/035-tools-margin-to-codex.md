# 目标

我们的 tools 内置对齐 codex

## 实现方式

全部插件化实现

接口实现+方便拓展替换

# codex

ChatGPT 的 Codex（这里指 OpenAI 新一代 **Codex coding agent**，不是早期 GPT-3 Codex 模型）和 Claude Code / OpenCode 的思路类似，但架构取向略不同：

* **Claude Code：工具丰富型 Agent**
* **OpenCode：开放式 Tool Runtime**
* **Codex：安全执行 + Patch 驱动 + Agent 协作**

Codex 的核心目标是：

> 给模型一个代码工作空间，让它能够读取仓库、修改代码、运行命令、测试验证，并通过审批机制安全执行。([OpenAI Help Center][1])

整体架构：

```text
                 Codex Agent

                      |
                      |
              Tool Runtime

                      |
 -------------------------------------------------

     文件          修改          执行          协作

      |             |             |             |

   shell        apply_patch     exec        update_plan
   rg           git diff        test        spawn_agent
   cat          commit          build       review

 -------------------------------------------------

                Local Workspace
```

---

# 1. Shell / Exec Tool ⭐⭐⭐⭐⭐

这是 Codex 最核心工具。

能力：

> 执行真实命令。

例如：

```bash
npm test

mvn clean package

cargo build

git status

docker compose up
```

对应：

```text
LLM
 |
 exec
 |
 Operating System
```

用途：

* 编译
* 测试
* 调试
* 部署
* 环境检查

Codex 支持不同权限模式：

| 模式        | 能力          |
| --------- | ----------- |
| Suggest   | 只建议命令       |
| Auto Edit | 自动修改文件      |
| Full Auto | 自动修改+执行（沙箱） |

([OpenAI Help Center][1])

---

# 2. apply_patch ⭐⭐⭐⭐⭐

这是 Codex 和 Claude/OpenCode 最大区别之一。

Claude：

```text
Edit Tool

old text
 ↓
new text
```

Codex：

```text
apply_patch

diff patch
```

例如：

```diff
*** Begin Patch
*** Update File: User.java

- return user;

+ return Optional.of(user);

*** End Patch
```

优势：

* 修改精确
* 天然支持 diff
* Git 友好
* 容易审查

OpenAI Codex 内部明确使用这种 patch 工作流。([GitHub][2])

---

# 3. File Search / rg ⭐⭐⭐⭐⭐

Codex 更偏向：

```bash
rg
rg --files
```

搜索代码。

例如：

用户：

> 找一下用户登录流程

Agent：

```bash
rg "login"

```

得到：

```text
AuthController

AuthService

UserRepository
```

---

本质：

代码检索能力：

```text
Repository

   |
   |
 Search Index

   |
   |
LLM Context
```

---

# 4. Read File ⭐⭐⭐⭐⭐

读取代码。

例如：

```text
read:

src/service/UserService.java
```

用途：

* 理解代码
* 分析架构
* Debug

不过 Codex CLI 设计上更倾向：

```bash
cat
sed
rg
```

通过 shell 完成读取。

---

# 5. update_plan ⭐⭐⭐⭐⭐

这是 Codex 非常重要的 Agent 能力。

用于复杂任务。

例如：

需求：

> 把单体 Spring Boot 拆微服务

Codex：

```text
Plan:

[x] 分析模块

[ ] 拆分 domain

[ ] 修改 pom

[ ] 添加 gateway

[ ] 测试
```

对应：

```text
Planner Agent
       |
       |
 Execution Agent
```

([GitHub][2])

---

# 6. apply_patch + git diff ⭐⭐⭐⭐⭐

Codex 非常强调：

```text
修改

↓

diff

↓

review

↓

commit
```

不像一些 Agent：

```text
直接覆盖文件
```

更加接近：

```text
Software Engineer Workflow
```

---

# 7. Git Tool ⭐⭐⭐⭐

虽然很多 Git 操作通过 shell：

```bash
git status

git diff

git commit
```

但是能力层面包括：

* 查看修改
* 创建 patch
* 回滚
* 提交

目标：

保证代码变化可追踪。

---

# 8. Test / Build Execution ⭐⭐⭐⭐⭐

不是独立工具，而是通过 exec 实现。

例如：

Java:

```bash
mvn test
```

Rust:

```bash
cargo test
```

Agent 可以：

```text
修改

↓

测试

↓

发现失败

↓

继续修复
```

形成闭环：

```text
Code
 |
Build
 |
Test
 |
Fix
 |
Repeat
```

---

# 9. Image Viewer ⭐⭐⭐

Codex 支持理解图片。

例如：

用户上传：

* UI 截图
* 架构图
* 错误截图

Agent：

```text
Image

↓

Vision Model

↓

Code Change
```

([OpenAI Help Center][1])

---

# 10. Agent Collaboration ⭐⭐⭐⭐⭐

新版 Codex 引入多 Agent 能力。

类似：

```text
                Main Agent

                    |
        -------------------------

        |          |           |

   Analyzer    Coder      Tester

```

能力：

* spawn agent
* send message
* resume agent
* close agent

([GitHub][3])

---

# 11. Sandbox ⭐⭐⭐⭐⭐

这是 Codex 很大的特色。

安全执行：

```text
                 Codex

                   |

              Sandbox

                   |

       --------------------

       File System

       Process

       Network

```

控制：

* 文件范围
* 命令权限
* 网络访问

尤其适合：

企业环境。

([OpenAI Help Center][1])

---

# 12. Approval System ⭐⭐⭐⭐⭐

Codex 强调：

Human Control。

例如：

执行：

```bash
rm -rf
```

或者：

```bash
npm install
```

之前：

询问用户。

类似：

```text
Agent

 |

Need permission

 |

Human approve

 |

Execute
```

---

# Codex Tool 总览

| Tool        | 能力     | 作用   |
| ----------- | ------ | ---- |
| exec/shell  | 执行命令   | 操作系统 |
| apply_patch | 修改代码   | 安全编辑 |
| rg/search   | 搜索代码   | 理解项目 |
| read file   | 读取文件   | 上下文  |
| update_plan | 任务规划   | 复杂开发 |
| git diff    | 版本控制   | 审查   |
| test/build  | 验证     | 闭环   |
| image view  | 视觉理解   | UI分析 |
| spawn_agent | 多Agent | 并行   |
| sandbox     | 隔离环境   | 安全   |
| approval    | 人工控制   | 治理   |

---

# 三者核心区别

## OpenCode

```text
Tool First

file
edit
bash
lsp
plugin
```

偏：

> 开源 Agent Runtime

---

## Claude Code

```text
Agent First

Tool
+
Planner
+
Sub Agent
+
MCP
```

偏：

> AI 软件工程师

---

## Codex

```text
Execution First

Patch
+
Sandbox
+
Approval
+
Agent
```

偏：

> 安全的软件工程自动化系统

---

# 如果映射到你的 core-agent 架构

你现在设计：

```
core-agent
|
+-- core-ai
|
+-- core-workflow
|
+-- core-plugin
|
+-- core-openapi
```

实际上可以吸收三家的最佳实践：

```text
                 core-agent

                     |

              Agent Runtime

                     |

 ------------------------------------------------

 Planning Layer

   update_plan
   task graph
   workflow


 Tool Runtime

   file.read
   file.search
   file.patch
   shell.exec
   git.diff


 Execution Runtime

   sandbox
   permission
   approval


 Multi Agent Runtime

   planner
   coder
   reviewer
   tester


 Enterprise MCP Layer

   CMDB
   Grafana
   Prometheus
   Jira
   Kubernetes

 ------------------------------------------------
```

如果目标是你之前说的 **AI RCA / NOC / 企业级 Agent OS**，其实 Codex 的：

> `Plan + Patch + Sandbox + Approval`

比单纯的 Coding Agent 更值得借鉴，因为它天然适合生产环境。

[1]: https://help.openai.com/en/articles/11096431?utm_source=chatgpt.com "OpenAI Codex CLI – Getting Started | OpenAI Help Center"
[2]: https://github.com/openai/codex/blob/main/codex-rs/core/prompt_with_apply_patch_instructions.md?utm_source=chatgpt.com "codex/codex-rs/core/prompt_with_apply_patch_instructions.md at main · openai/codex · GitHub"
[3]: https://github.com/openai/codex/issues/17899?utm_source=chatgpt.com "Failing to use apply_patch tool · Issue #17899 · openai/codex"

