# 目标

我们的 tools 内置对齐 opencode

## 实现方式

全部插件化实现

接口实现+方便拓展替换

# opencode 能力

OpenCode 的核心定位是 **Terminal 里的 Coding Agent Runtime**。它不像传统 IDE 插件只是“补全代码”，而是给 LLM 提供一组 **可以操作真实开发环境的 Tools（工具能力）**。LLM 通过 Tool Calling 调用这些能力完成“理解代码 → 修改代码 → 执行验证”的闭环。([OpenCode][1])

目前 OpenCode 内置工具大致可以分成几类：

```
                    OpenCode Agent

                         |
                         v

              +--------------------+
              |    Tool Runtime    |
              +--------------------+

       文件理解       文件修改       环境执行       智能分析

          |              |             |             |
       read           edit           bash          lsp
       grep           write
       glob           patch

       webfetch       question
       websearch      todo
```

([OpenCode][2])

---

# 1. bash —— Shell 执行能力 ⭐⭐⭐⭐⭐

## 能力

让 Agent 可以执行真实命令：

例如：

```bash
npm install

mvn test

cargo build

git status

docker ps

kubectl get pods

python train.py
```

对应能力：

```
AI
 |
 | 调用 bash
 |
 v

真实操作系统环境
```

用途：

* 安装依赖
* 编译项目
* 运行测试
* 查看日志
* Git 操作
* 部署操作

例如：

用户：

> 帮我修复这个 Spring Boot 项目

Agent：

```
1. read pom.xml

2. bash:
   mvn test

3. 发现错误

4. edit Controller.java

5. bash:
   mvn test
```

---

本质：

> 给 AI 一台终端。

这是 Agent 和 ChatGPT 最大区别之一。

---

# 2. read —— 文件读取 ⭐⭐⭐⭐⭐

## 能力

读取代码文件：

例如：

```
src/main/java/UserService.java
```

返回：

```
class UserService {

}
```

支持：

* 大文件
* 指定行范围

例如：

```
read UserService.java

line 100-200
```

([OpenCode][1])

---

用途：

理解项目。

典型流程：

```
glob
 |
找到文件

read
 |
读取内容

LLM分析
 |
决定修改
```

---

# 3. glob —— 文件发现 ⭐⭐⭐⭐⭐

类似：

```
find
```

能力：

根据模式找文件。

例如：

```
**/*.java

src/**/*.vue

*.yaml
```

返回：

```
src/
 ├── UserController.java
 ├── UserService.java
 └── UserMapper.java
```

---

用途：

项目扫描。

例如：

用户：

> 给这个项目增加 OAuth 登录

Agent：

```
glob:

**/*Security*

找到:

SecurityConfig.java

read

分析认证体系
```

---

# 4. grep —— 内容搜索 ⭐⭐⭐⭐⭐

类似：

```
grep
ripgrep
```

能力：

全文搜索。

例如：

搜索：

```
UserService
```

结果：

```
UserController.java

UserService.java

UserMapper.xml
```

用途：

代码关系分析。

例如：

寻找：

```
谁调用这个方法？
```

```
grep:

calculatePrice(
```

得到：

```
OrderService

PaymentService
```

---

这其实是 Coding Agent 最重要能力之一。

很多 Agent 的代码理解能力，本质依赖：

```
glob + grep + read
```

形成：

```
Codebase Retrieval System
```

---

# 5. edit —— 精确修改 ⭐⭐⭐⭐⭐

核心修改工具。

不是让 AI 重新生成整个文件，而是：

```
old text

替换

new text
```

例如：

原：

```java
return user;
```

改：

```java
return Optional.of(user);
```

优势：

* 修改范围小
* 不容易破坏代码
* diff 清晰

---

# 6. write —— 创建文件 ⭐⭐⭐⭐

创建：

```
UserController.java

Dockerfile

README.md

config.yaml
```

例如：

Agent：

```
write:

src/api/UserApi.java
```

---

和 edit 区别：

| 工具    | 作用      |
| ----- | ------- |
| edit  | 修改已有内容  |
| write | 创建/覆盖文件 |

([OpenCode][2])

---

# 7. patch —— Patch 修改 ⭐⭐⭐⭐

类似：

```
git apply
```

一次提交 diff。

例如：

```diff
+ add UserService
- remove old method
```

适合：

大范围修改。

---

# 8. lsp —— 代码智能分析 ⭐⭐⭐⭐⭐（实验）

这是 IDE 级能力。

依赖：

Language Server Protocol。

提供：

## 跳转定义

例如：

```
UserService
      |
      |
      v

UserService.java
```

## 查找引用

```
谁调用这个方法？
```

---

## Hover

查看：

```
类型
参数
注释
```

---

## Call Hierarchy

调用链：

```
Controller

 ↓

Service

 ↓

DAO

 ↓

Database
```

([OpenCode][1])

---

这部分如果成熟：

OpenCode ≈ VSCode + AI。

---

# 9. webfetch —— 网络读取 ⭐⭐⭐⭐

访问网页。

例如：

用户：

> 根据 Spring 官方文档升级版本

Agent：

```
webfetch:

spring.io/docs
```

读取：

```
Migration Guide
```

然后修改代码。

([OpenCode][2])

---

# 10. websearch —— 搜索 ⭐⭐⭐⭐

用于：

寻找资料：

例如：

```
Spring Boot 4 migration issue
```

然后：

```
webfetch
```

获取详情。

---

# 11. question —— 主动询问 ⭐⭐⭐⭐

Agent 可以暂停并问用户：

例如：

```
发现两个方案：

A 使用 Redis

B 使用 SQLite

请选择：
```

类似：

Human-in-the-loop。

---

# 12. todowrite / todoread —— 任务规划 ⭐⭐⭐⭐

让 Agent 管理任务。

例如：

```
TODO:

[x] 分析项目结构

[x] 修改数据库

[ ] 增加测试

[ ] 部署
```

适合：

大型任务。

---

# 综合能力矩阵

| Tool      | 能力   | 对应人类能力 |
| --------- | ---- | ------ |
| glob      | 发现文件 | 浏览项目   |
| grep      | 搜索关系 | 查资料    |
| read      | 阅读代码 | 理解     |
| lsp       | 代码智能 | IDE能力  |
| edit      | 修改代码 | 编程     |
| write     | 创建文件 | 开发     |
| patch     | 批量修改 | 重构     |
| bash      | 执行环境 | 操作电脑   |
| websearch | 查资料  | 搜索     |
| webfetch  | 阅读文档 | 学习     |
| todo      | 规划任务 | 项目管理   |
| question  | 沟通决策 | 询问用户   |

---

# 如果抽象成 Agent Architecture

OpenCode 实际上就是：

```
                 LLM Brain

                    |
                    |
              Tool Calling Layer

                    |
 ------------------------------------------------

 文件世界          代码世界        系统世界

 read             lsp             bash
 grep             edit            git
 glob             write           docker

 ------------------------------------------------

                    |
              Developer Machine
```

---

# 对你之前设计的 core-agent 来说

OpenCode 的 Tool 体系非常值得参考。

你之前规划：

```
core-agent
    |
    +-- tool runtime
    |
    +-- workflow
    |
    +-- memory
    |
    +-- plugin
```

其实可以直接参考：

```
core-agent-tool

    file.read
    file.write
    file.search

    shell.exec

    code.lsp

    http.fetch

    database.query

    docker.exec

    kubernetes.exec

    monitoring.query

    cmdb.lookup

    alarm.query
```

也就是说：

OpenCode 是：

> Coding Agent Tool OS

而你的方向更像：

> Enterprise Agent Tool OS

扩展以后：

```
OpenCode Tool
      |
      v
Developer Environment


你的 Core-Agent Tool
      |
      v
Enterprise Environment
```

这两个架构思想非常接近。你之前规划的 **core-plugin / core-workflow / core-openapi / core-ai**，本质上就是把 OpenCode 的 Tool Runtime 企业化。

[1]: https://opencode.ai/docs/zh-cn/tools/?utm_source=chatgpt.com "工具 | OpenCode"
[2]: https://dev.opencode.ai/docs/tools/?utm_source=chatgpt.com "Tools | OpenCode"
