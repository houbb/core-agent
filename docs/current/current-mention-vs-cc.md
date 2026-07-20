# `@` (AT Mention) 上下文模块对标分析

> 对标 Claude Code 的 `@` 上下文引用能力，梳理当前现状与差距，按优先级排序补充。

---

## 一、Claude Code 的 `@` 上下文能力全景

### 1.1 输入侧 (`@` in prompt)

| 能力 | 描述 | 交互方式 |
|------|------|----------|
| `@file` | 引用工作区文件 | 输入 `@` 触发补全列表，模糊搜索文件名 |
| `@file:line` | 引用文件 + 行范围 | `@file:42` 或 `@file:10-20` |
| `@directory/` | 引用整个目录 | `@src/` 展开目录下所有文件 |
| 补全 UI | 高亮匹配 + 路径预览 | 下拉列表，文件名优先，含文件类型图标 |
| 引用显示 | 输入框上方显示已引用文件 | Chip 标签，含文件名、行号、删除按钮 |
| 多文件引用 | 同时引用多个文件 | 16 个引用上限 |

### 1.2 输出侧 (Agent 回复中的引用)

| 能力 | 描述 | 交互方式 |
|------|------|----------|
| 文件路径可点击 | Agent 回复中 `@file` 路径可点击 | 点击跳转到文件对应位置 |
| 行号跳转 | `src/main.rs:42` 可点击跳转 | 点击打开文件到指定行 |
| 引用高亮 | 引用的文件路径以不同颜色/样式显示 | 视觉上区分于普通文本 |
| 代码块引用关联 | 代码块上方标注来源文件路径 | 关联文件路径可点击 |

### 1.3 引用侧 (上下文引用)

| 能力 | 描述 | 交互方式 |
|------|------|----------|
| 选中代码引用 | 选中代码段 → 加入上下文 | 右键菜单 / 浮动菜单 |
| 消息历史引用 | 选中历史消息 → 继续讨论 | 消息 hover 菜单 |
| Context Chip | 当前上下文可视化 | 输入框上方 Chip 标签 |

---

## 二、本项目现状 (Current Implementation)

### 2.1 已实现能力 ✅

```
输入侧 (@ 补全)          ✅ 完整
ContextReference 模型     ✅ 完整 (File/Selection/Message)
ContextReference 持久化   ✅ 完整 (Sqlite)
Reference 接入 Context    ✅ 完整 (Composer + Provider)
模糊搜索                  ✅ 完整 (ripgrep 优先 + fuzzy)
```

#### 2.1.1 输入补全 (`src/interaction.rs`)

```rust
// ContextCandidateIndex — 工作区文件索引 + 模糊搜索
// 支持: ripgrep 优先索引, 目录展开, 模糊匹配 (文件名优先)
// 限制: 16 个 mention / 128 文件 / 256 KiB 单文件 / 1 MiB 总计
```

#### 2.1.2 引用模型 (`core-agent-context/src/domain/context_reference.rs`)

```rust
pub enum ReferenceType { File, Selection, Message }
pub enum ReferenceLocator { File { path, start_line, end_line },
                            Selection { content, source_path, start_line, end_line },
                            Message { session_id, conversation_id, message_id } }
pub struct ContextReference { id, reference_type, locator, snapshot, metadata, created_at }
pub struct ContextPackage { user_question, references, metadata }
```

#### 2.1.3 Desktop 补全 (`agent-desktop/src/prompt-completion.ts`)

```typescript
// mentionQueryAtCursor — 解析 @ 位置
// contextCompletions — 将匹配路径转为补全项
// 补全列表显示在输入框上方
```

### 2.2 未实现能力 ❌

```
输出侧文件路径可点击     ❌ 完全缺失
行号跳转                 ❌ 完全缺失
Context Chip 组件         ❌ 完全缺失
选中代码引用 UI           ❌ 完全缺失
历史消息引用 UI           ❌ 完全缺失
文件打开/跳转机制         ❌ 完全缺失
```

---

## 三、逐项对标差距分析

### 3.1 输出侧可点击文件链接 🔴 P0

| 维度 | Claude Code | 本项目 | 差距 |
|------|-------------|--------|------|
| 文件路径渲染 | 蓝色/下划线，可点击 | 纯文本 `<p>{{ item.content }}</p>` | ❌ |
| 行号跳转 | `src/main.rs:42` 点击跳转 | 无 | ❌ |
| 视觉区分 | 高亮 + 特殊样式 | 无 | ❌ |
| 代码块来源标注 | 代码块上方显示文件路径 | 无 | ❌ |

**影响**：用户无法通过对话结果直接定位到代码位置，需要手动搜索文件。

**实现路径**：前端正则解析 `@path:line` 或文件路径模式 → 转为可点击链接 → 点击触发文件打开。

### 3.2 Context Chip 组件 🔴 P0

| 维度 | Claude Code | 本项目 | 差距 |
|------|-------------|--------|------|
| 输入框上方显示引用 | Chip 标签，含文件名/行号 | 无 | ❌ |
| 删除引用 | 点击 Chip 的 `x` 按钮 | 无 | ❌ |
| 引用数量指示 | 显示已引用 N 个文件 | 无 | ❌ |
| Token 使用量 | 显示引用消耗的 Token | 无 | ❌ |

**影响**：用户无法直观看到当前已经引用了哪些上下文，容易重复引用或漏引用。

### 3.3 文件跳转机制 🟡 P1

| 维度 | Claude Code | 本项目 | 差距 |
|------|-------------|--------|------|
| Desktop | 点击文件链接打开编辑器 | 无 | ❌ |
| CLI | 点击文件链接打开 VS Code | 无 | ❌ |
| 回退 | 文件不存在时提示 | 无 | ❌ |

**影响**：用户无法从对话结果直接跳转到代码位置。

**实现路径**：
- Desktop: Tauri `shell.open()` / `tauri-plugin-shell`
- CLI: `code --goto <path>:<line>` 或 `cursor --goto <path>:<line>`

### 3.4 选中代码引用 UI 🟡 P1

| 维度 | Claude Code | 本项目 | 差距 |
|------|-------------|--------|------|
| 代码选中 | 选中后浮动菜单 | 无 | ❌ |
| 右键菜单 | 右键 → "Ask Claude" | 无 | ❌ |
| 行号获取 | 自动获取选中区域行号 | 无 | ❌ |

**影响**：用户需要手动输入 `@file` 并拼写行号，体验不够直观。

### 3.5 历史消息引用 UI 🟡 P1

| 维度 | Claude Code | 本项目 | 差距 |
|------|-------------|--------|------|
| 消息 hover | 历史消息 hover 显示引用按钮 | 无 | ❌ |
| 引用到上下文 | 点击后消息加入当前上下文 | 无 | ❌ |

**影响**：用户无法引用历史对话内容作为当前上下文的补充。

---

## 四、优先级总结

```
优先级    能力                 对标 Claude Code      当前状态    预估工作量
─────────────────────────────────────────────────────────────────────────
P0 🔴     输出侧文件路径可点击    @file 可点击跳转      ✅ 已完成     2-3 天
P0 🔴     Context Chip 组件     输入框上方引用标签     ✅ 已完成     2-3 天
P0 🔴     行号跳转              @file:line 跳转       ✅ 已完成     1-2 天

P1 🟡     文件跳转机制          桌面/终端打开文件      ✅ 已完成     1-2 天
P1 🟡     选中代码引用 UI        代码选中→浮动菜单     ✅ 已完成     2-3 天
P1 🟡     历史消息引用 UI        消息 hover→引用       ✅ 已完成     2-3 天

P2 🟢     代码块来源标注         代码块上方文件路径    ✅ 已完成     1 天
P2 🟢     引用样式优化          高亮 + 视觉区分        ✅ 已完成     1 天
P2 🟢     引用 Token 统计        显示引用消耗          ✅ 已完成     0.5 天
```

---

## 五、架构关系

```
┌─────────────────────────────────────────────────────────────┐
│                    用户输入 (User Input)                      │
│  @file        @file:line       @directory/       @message     │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│                  ContextMentionResolver                      │
│  (src/interaction.rs)                                        │
│  · 解析 @ 语法 → 返回文件路径                                │
│  · 读取文件内容 → 生成 JSON context                          │
│  · 限制: 16 mention / 128 files / 1 MiB total               │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│                  ContextReference                            │
│  (core-agent-context)                                        │
│  · File / Selection / Message 三种引用类型                    │
│  · ContextPackage 聚合引用                                    │
│  · Sqlite 持久化                                              │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│                  Context Runtime                             │
│  · Collect → Reduce → Compose → Snapshot                     │
│  · 引用注入到 Context Pipeline                                │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│                  Agent Reply                                  │
│  · 输出中包含 @path 引用                                      │
│  · 需要前端渲染为可点击链接 ← ❌ 当前缺失                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 六、实施路线

### 第一步：输出侧文件路径可点击 (P0)

**Desktop 端** (`agent-desktop/src/App.vue`):

```vue
<!-- 消息渲染：将文件路径转为可点击链接 -->
<article v-for="item in controller.state.conversation" :key="item.id" class="message" :class="item.role">
  <header class="message-header">...</header>
  <div class="message-content" v-html="renderMessageContent(item.content)" />
</article>
```

```typescript
// 解析消息中的文件路径并转为链接
function renderMessageContent(text: string): string {
  return text.replace(
    /(@?[\w\/.-]+\.[a-z]+)(?::(\d+))?/gi,
    (match, path, line) => {
      const escaped = escapeHtml(match);
      return `<a class="file-link" href="file://${path}" data-line="${line || ''}" @click.prevent="openFile(path, line)">${escaped}</a>`;
    }
  );
}
```

**CLI 端** (`agent-cli/src/tui.rs`):

```rust
// 渲染消息时检测文件路径，转为可点击文本
fn render_messages(frame: &mut Frame<'_>, area: Rect, state: &mut TuiState) {
    for message in &state.messages {
        // 检测消息文本中的 @path:line 模式
        // 将文件路径以高亮颜色显示，并提示可点击
        let text = highlight_file_paths(&message.text);
        // ...
    }
}
```

### 第二步：Context Chip 组件 (P0)

**Desktop 端** (`agent-desktop/src/components/ContextChip.vue`):

```vue
<template>
  <div class="context-chip-bar">
    <div class="context-chip" v-for="ref in references" :key="ref.id">
      <span class="chip-icon">{{ refIcon(ref.type) }}</span>
      <span class="chip-label">{{ ref.path }}</span>
      <span class="chip-line" v-if="ref.startLine">L{{ ref.startLine }}-{{ ref.endLine }}</span>
      <button class="chip-remove" @click="remove(ref.id)">×</button>
    </div>
  </div>
</template>
```

### 第三步：文件跳转机制 (P1)

**Desktop**:
```typescript
import { open } from "@tauri-apps/plugin-shell";
async function openFile(path: string, line?: string) {
  // 方式1: 使用系统默认编辑器打开
  await open(path);
  // 方式2: 使用 IDE 打开（需额外配置）
  // code --goto path:line
}
```

**CLI**:
```rust
fn open_file(path: &str, line: Option<usize>) {
    let cmd = match line {
        Some(l) => format!("code --goto \"{}\":{}", path, l),
        None => format!("code \"{}\"", path),
    };
    std::process::Command::new("sh")
        .args(["-c", &cmd])
        .spawn()
        .ok();
}
```

---

## 七、现有代码关键位置

| 功能 | 位置 | 说明 |
|------|------|------|
| `@` 解析 | `src/interaction.rs:1196-1263` | `parse_mentions()` |
| 模糊搜索 | `src/interaction.rs:767-841` | `ContextCandidateIndex` |
| 文件内容解析 | `src/interaction.rs:1055-1193` | `ContextMentionResolver` |
| 引用模型 | `core-agent-context/src/domain/context_reference.rs` | 完整领域模型 |
| 引用持久化 | `core-agent-context/src/persistence/reference_store.rs` | Sqlite 存储 |
| 引用注入 Context | `core-agent-context/src/application/composer.rs:244-250` | Reference Slot 处理 |
| Desktop 补全 | `agent-desktop/src/prompt-completion.ts` | 前端补全逻辑 |
| Desktop 消息渲染 | `agent-desktop/src/App.vue:391-393` | 纯文本渲染 |
| CLI TUI 消息渲染 | `agent-cli/src/tui.rs:630-667` | 纯文本渲染 |
| 设计文档 | `design-docs/037-context-comment.md` | Context Annotation 设计 |

---

## 八、总结

**一句话**：`@` 上下文引用的**后端数据模型**和**输入补全**已就绪，但**输出侧渲染**和**交互体验**（可点击跳转、Context Chip、选中代码引用、历史消息引用）与 Claude Code 存在显著差距。

**核心差距**：
1. **输出侧文件路径不可点击** — 无法从对话结果直接跳转到代码位置
2. **缺少 Context Chip** — 用户无法直观看到当前引用
3. **缺少文件跳转机制** — 点击引用后没有打开文件的行为

**基础能力**（已就绪，无需改动）：
- `@` 解析与模糊搜索 ✅
- `ContextReference` 模型 (File/Selection/Message) ✅
- 引用持久化 (Sqlite) ✅
- 引用注入 Context Pipeline ✅