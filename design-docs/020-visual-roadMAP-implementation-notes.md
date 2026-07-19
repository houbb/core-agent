# P15 Visual Product Roadmap 实现说明

## 范围

把 020 的七阶段产品路线落成共享应用层合同，不提前实现 021～028 的具体 CLI/Desktop/Studio 页面。

## 实现

- 新增独立 `core-agent-app`，定义 `ProductPhase`、`ExperienceSurface`、`ProductCapability`。
- `PhaseDefinition::for_phase` 为 Phase 0～6 提供稳定表面和必需能力集合。
- `evaluate_readiness` 同时检查直接前置阶段与本阶段能力，确定性报告缺口。
- 根 crate 统一导出应用层合同，后续视觉 P 可直接复用。

## 验证

- 单元断言覆盖七阶段顺序、缺失前置阶段/能力报告和完整合同 ready。
- 本 P 不新增持久化，也不伪造 UI E2E；具体表面从 021 起逐 P 验证。
- 测试命令按约定在全部剩余 P 完成后统一运行。

## 已知边界

- readiness 是发布合同检查，不会自动探测代码功能；每项能力仍需由对应 P 的真实测试提供证据。
