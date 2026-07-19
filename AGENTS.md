# 称呼

每次和我沟通，叫我【帅哥】

---

# 沟通规范 ⚠️ 强制遵守

**需要你向我提问时，必须使用选项列表，禁止用散文/段落罗列问题。**

- ✅ 每个选项有 label（≤15 字）+ description（解释影响）
- ✅ 使用 `multiSelect: true` 当选项互不冲突
- ✅ 推荐选项标注 `（推荐）`
- ❌ 禁止 "Q1: xxx / Q2: xxx" 这种手工编号段落式提问
- ❌ 禁止把 5 个问题写成一整段散文让我读

---

# 编码原则

1. **先想再写** — 不假设、不隐藏困惑。不确定就问，多种解读就摆出来。
2. **极简优先** — 只写解决问题的最少代码。200 行能改成 50 行就改。
3. **手术式改动** — 只改必须改的。不"顺手优化"旁边代码，不删不相关的死代码。
4. **目标驱动** — 每步定义可验证的成功标准，循环直到达标。
5. **中文编码** - 统一为 UTF-8
---

# DB 规范

每个表必须有：`id` `create_time` `update_time` `create_user` `update_user` + 注释 + 合适索引。禁止外键。

## Apple UI 风格模式

- **Pill badge** — 小圆角(10px)、半透明背景+对应文字色：`.badge { border-radius: 10px; padding: 2px 8px; }`
- **三级按钮** — 普通(`--bg-secondary`+`--border`) → 强调(`--accent-bg`+`--accent` 边框) → 主要(`--accent` 背景+`--color-on-accent`)
- **页面 100% 宽度**，自适应容器，不硬编码 `width`
- **留白充足**，padding ≥ 12px，紧凑区域 ≥ 8px
- **层级清晰** — 标题 17px/700、正文 13px/400、辅助 11px/次要色

# 代码风格

- 前端组件化，方便拓展
- 遵循已有代码风格，保持一致
- 只阅读核心相关文件，减少上下文

---

# 测试验证 ⚠️ 强制执行

单元断言测试+端到端测试

# 执行原则

- **最少干扰** — 文件变更+命令执行全部自动确认，不给用户确认
- **最短耗时** — 脑暴确认后并行实现，准确+快速
- **实现优先，统一验证** — 先实现全部功能（后端+前端+注册+i18n），最后统一跑四步验证。实现过程中不插桩运行 cargo check / vue-tsc / vite build 等验证命令，避免中断流程
- **完成后 review 3 次+小幅度优化**

# Unknowns Management

## 强制触发规则 (Hard Trigger)

**当用户提出以下任一类型的任务时，你必须先执行 Unknowns Discovery 流程，然后才能开始实现：**

- 新功能开发 / 新模块创建
- 架构设计 / 数据模型设计
- 数据库表设计或变更
- 认证、授权、安全相关代码
- 跨模块 / 跨系统改动
- 多文件、大范围改动
- 不可逆操作（如数据库迁移、删除数据）
- 用户需求中有主观描述词（"简单""好看""智能""自然"）

**这是硬性要求，不是建议。**

**执行方式（按优先级尝试）：**

1. **优先** — 调用 Skill 工具：`Skill("unknowns-discovery", "standard")`
2. **Fallback** — 如果 Skill 不可用（返回 "Unknown skill"），则手动执行 `.agents/skills/unknowns-discovery/SKILL.md` 中定义的完整流程，产出 Unknowns Report（模板在 `templates/unknowns-report.md`）。将报告内容直接输出给用户确认，确认后再进入实现。

**只有以下情况可以跳过：**
- 单行修复（typo、注释修正）
- 单文件简单 bug fix（已有明确根因）
- 纯代码解释类问题

---

Do not treat the initial request, specification, or implementation plan as a complete description of reality.

Before implementing any non-trivial change, identify the important unknowns that could alter the architecture, data model, user-visible behavior, security, compatibility, or scope of the work.

Distinguish between:

* **Known knowns**: facts confirmed by the user, codebase, tests, or documentation.
* **Known unknowns**: unresolved decisions or missing information already visible.
* **Unknown knowns**: implicit product, design, or domain expectations that have not yet been made explicit.
* **Unknown unknowns**: overlooked constraints, dependencies, edge cases, failure modes, or alternative problem definitions.

Follow these rules:

1. Do not silently convert uncertainty into an assumption.
2. Verify codebase facts by inspecting the relevant code, tests, schema, history, and adjacent modules.
3. Prioritize unknowns that are high-impact, difficult to reverse, or expensive to discover later.
4. For reversible local decisions, choose the most conservative option and record the assumption.
5. For irreversible or cross-system decisions, surface the issue before committing to an implementation direction.
6. During implementation, record material discoveries, deviations, assumptions, and unresolved risks.
7. After implementation, explain what changed, what remains uncertain, and what evidence verifies the result.
8. Convert recurring discoveries into tests, documentation, conventions, or reusable project knowledge.

For substantial features, architecture changes, ambiguous product work, migrations, or cross-module changes, run the **Unknowns Discovery** skill before implementation.


# 要求

每一次新的功能点加入，都必须真实的测试验证，尽可能的和已有的模块打通。

1）最基础的 junit5 断言测试验证，保障基本功能正确性

2）尽可能的端到端测试验证，保障整体功能正确性

3）简明扼要的使用+变更内容

# 文档更新

## 变更日志

每一次功能全部完成后，将变更压缩更新到 CHANGELOG.md 中，版本号主动询问一下用户

## README

README.md 也进行同步的更新，保持文档最新

要面向用户去写，保留启动方式（方便快速体验）。