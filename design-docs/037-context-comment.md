# 目标

类似于 opencode，可以选中一段文字（具体到路径，多少行）评论，作为上下文的补充。

甚至 chatGPT 可以对历史的会话选中，进行评论，作为上下文的补充，

我们需要对于历史的上下文，从以前的 AT 文件路径内容进行增强。

可以是选中的内容段落、session 历史会话等等

## 操作

操作方面注重交互的简介、直观。所见即所得。

# 设计

你提出的 **Comment / Context Annotation 能力** 是 Agent 产品里非常关键的一层。

OpenCode、Claude Code、ChatGPT 的本质都是：

> 用户不仅通过聊天告诉 Agent 信息，还可以主动“标注上下文”。

但是目前这些产品的能力都比较碎片化：

* OpenCode：代码选中 → comment → 加入上下文
* ChatGPT：历史消息引用 → 继续讨论
* Claude Code：引用文件片段
* IDE Agent：选中代码 → Ask AI

你的设计可以进一步抽象为：

> **Context Annotation Runtime（上下文标注运行时）**

它不是简单 comment，而是：

```
用户意图
    +
指定上下文
    +
上下文关系
    +
Agent任务

形成 Context Package
```

---

# 一、重新定义 Comment

不要叫 Comment。

建议内部命名：

```
core-agent-context
```

用户层：

```
Comment
Mention
Reference
Attach Context
```

类似：

```
@file
@selection
@message
@session
@memory
@artifact
```

---

# 二、核心目标

## 用户角度

一句话：

> 我告诉 Agent “看这里”。

例如：

代码：

```java
UserService.java

100-130行
```

用户：

```
@comment

这里为什么会出现 NPE？
```

Agent 获得：

```
问题:

NPE

上下文:

UserService.java
line 100-130
```

---

# 三、能力模型

整体：

```
                 Context Annotation


                         |

              Context Reference


 -------------------------------------------------

 File              Message             Session


 Selection         Artifact            Memory


 Diff              Terminal            Tool Result


 -------------------------------------------------

                         |

                 Context Package


                         |

                    Agent
```

---

# 四、Context 类型设计（重点）

## 1. File Context ⭐⭐⭐⭐⭐

最基础。

来源：

```
文件
+
范围
+
代码片段
```

例如：

```
src/UserService.java

Line:
120-160
```

生成：

```json
{
"type":"file",
"path":"src/UserService.java",
"start":120,
"end":160
}
```

---

## 增强：

支持：

### Symbol 级别

不要只支持行。

例如：

```
UserService.createUser()
```

内部：

```
AST/LSP

class
method
field
```

定位。

这是超越 OpenCode 的地方。

---

# 2. Selection Context ⭐⭐⭐⭐⭐

用户鼠标选择。

例如：

Desktop:

```
----------------

return user.getName();

----------------
```

右键：

```
Ask Agent
Comment
Explain
Fix
```

生成：

```
SelectionContext
```

---

# 3. Message Context ⭐⭐⭐⭐⭐

针对聊天历史。

类似 ChatGPT：

选中历史消息。

例如：

历史：

```
用户:
设计 core-agent

AI:
建议 P0...
```

用户：

选择 AI 的回答：

```
为什么这里选择 SQLite？
```

生成：

```json
{
"type":"message",
"sessionId":"",
"messageId":"xxx"
}
```

---

# 4. Session Context ⭐⭐⭐⭐⭐

整个历史会话。

例如：

```
@session

分析之前关于 core-agent 的设计
```

Agent 获取：

```
Conversation Memory
```

---

但是注意：

不能直接塞全部历史。

需要：

```
Session

 |

Summary

 |

Relevant Context
```

---

# 5. Terminal Context ⭐⭐⭐⭐

非常重要。

例如：

用户看到：

```
mvn test失败
```

选择：

```
错误日志
```

comment:

```
帮我分析
```

上下文：

```
command

stdout

stderr

exit code
```

---

# 6. Tool Result Context ⭐⭐⭐⭐

Agent 自己产生的信息。

例如：

工具：

```
grep UserService
```

结果：

```
20个引用
```

用户：

选择：

```
第10个结果
```

继续：

```
分析这个调用链
```

---

# 7. Diff Context ⭐⭐⭐⭐⭐

类似 Cursor。

用户：

看到：

```diff
+ add cache
- remove logic
```

comment:

```
review这个修改
```

---

# 五、核心数据模型

建议：

```
core-agent-context
```

## ContextReference

```java
class ContextReference {


 id;


 type;


 source;


 locator;


 snapshot;


 metadata;


}
```

例如：

文件：

```json
{
"type":"FILE",
"source":"workspace",

"locator":{
"path":"User.java",
"start":100,
"end":150
}
}
```

---

# 六、Context Package

最终给 Agent：

不是散乱信息。

统一：

```
ContextPackage
```

结构：

```
ContextPackage

|
├── user_question
|
├── references
|
|    ├── file
|    ├── message
|    ├── terminal
|
├── metadata
|
└── priority
```

---

# 七、交互设计 UX

重点：

> 所见即所得

---

# Desktop UX

## 场景1：代码选择

用户选中：

```
@Service
public class UserService {

}
```

浮动菜单：

```
---------------------

💬 Comment

🤖 Ask Agent

🔍 Explain

🛠 Fix

📋 Copy

---------------------
```

---

点击 Comment：

出现：

```
--------------------------------

你的问题:

[ 输入................ ]



Context:

✓ UserService.java

✓ Line 20-40


[Send]

--------------------------------
```

---

发送：

Agent：

```
我看到：

UserService.java
20-40行

问题:
为什么这里使用@Transactional?
```

---

# Terminal UX

类似：

```
> comment src/User.java:20-50


?
请输入问题:

```

---

或者：

```
cat User.java

选中

ctrl+c

@comment
```

---

# 八、历史会话 UX

ChatGPT 类似。

消息 hover：

出现：

```
-----------------

👍

引用

Comment

继续分析

-----------------
```

点击：

```
引用到当前上下文
```

生成：

```
Context Chip
```

---

# 九、Context Chip（非常重要）

类似 ChatGPT attachment。

显示：

```
当前上下文:

[ UserService.java L20-L50 ] x

[ Session #102 ] x

[ Error.log ] x


请输入问题...
```

---

用户随时：

添加：

```
+
```

删除：

```
x
```

排序。

---

# 十、Context 优先级

多个上下文：

需要权重。

例如：

```
Current Selection

      >

Current File

      >

Current Session

      >

Memory

      >

Knowledge
```

优先级：

```
P0:

User Explicit Context


P1:

Current Task


P2:

Project Context


P3:

Memory


P4:

Knowledge
```

---

# 十一、和 Memory 的区别

非常重要。

不要混。

## Context

短期：

```
我现在让你看这个
```

生命周期：

分钟

---

## Memory

长期：

```
我喜欢 Java
```

生命周期：

月/年

---

关系：

```
Context

    |

Task

    |

Memory
```

---

# 十二、权限设计

企业必须。

例如：

用户选择：

```
生产日志
```

但是：

没有权限。

返回：

```
无法添加该上下文

原因:

Production Log Permission Denied
```

---

# 十三、审计

记录：

```
User

什么时候

引用什么

发送给哪个 Agent

使用多少 token
```

---

# 十四、实现路线

## Phase 0 MVP ⭐⭐⭐⭐⭐

实现：

```
File Context

Selection Context

Message Context
```

支持：

```
Desktop

Terminal
```

---

## Phase 1

增加：

```
Session Context

Terminal Context

Diff Context
```

---

## Phase 2

代码增强：

```
AST Context

Symbol Context

LSP Context
```

---

## Phase 3

企业：

```
Log Context

Metric Context

Trace Context

CMDB Context
```

---

## Phase 4

智能：

```
Auto Context Retrieval


Agent 自动选择上下文
```

---

# 十五、最终架构

```
                 core-agent-context


                         |

              Context Provider


 -------------------------------------------------

 FileProvider

 MessageProvider

 SessionProvider

 TerminalProvider

 DiffProvider

 ASTProvider

 LSPProvider

 LogProvider

 MetricProvider


 -------------------------------------------------

                         |

               Context Package Builder


                         |

                    Agent Runtime


```

---

# 和 OpenCode / Claude Code / ChatGPT 对比

| 能力               | OpenCode | Claude | ChatGPT | 你的设计  |
| ---------------- | -------- | ------ | ------- | ----- |
| 代码选中             | ✅        | ✅      | 部分      | ✅     |
| 文件引用             | ✅        | ✅      | ✅       | ✅     |
| 历史消息引用           | 弱        | 弱      | ⭐⭐⭐⭐    | ⭐⭐⭐⭐⭐ |
| Session Context  | 弱        | 弱      | ⭐⭐⭐⭐    | ⭐⭐⭐⭐⭐ |
| Terminal Context | 弱        | 中      | 弱       | ⭐⭐⭐⭐⭐ |
| AST Context      | 无        | 无      | 无       | ⭐⭐⭐⭐⭐ |
| 企业数据 Context     | 无        | 无      | 无       | ⭐⭐⭐⭐⭐ |

---

我认为这个模块应该成为 `core-agent` 的一级能力：

```
core-agent

├── runtime
├── tools
├── memory
├── context   ⭐⭐⭐⭐⭐
├── workflow
├── permission
├── plugin
```

因为未来 Agent 的竞争不是：

> 谁模型更强

而是：

> 谁能更准确地给模型提供正确 Context。

这个 `Context Annotation Runtime` 会成为整个 Agent OS 的“输入操作系统”。
