# P15 Visual Product Roadmap Unknowns Report

## Scope

`020-visual-roadMAP.md` 是产品演进总路线，不是单一 UI 页面规格。P15 不提前实现后续 CLI/Desktop/Studio，而是落地一个共享、可测试的 Experience Roadmap Contract：阶段、产品表面、必需能力、阶段依赖和 readiness 评估。后续 021～028 逐份实现具体体验层。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | 是否在本 P 一次实现全部 CLI/Desktop/Studio | 否；020 只定义演进契约，具体功能严格留给对应文档 |
| P0 | 路线图如何避免成为无约束文档 | 用强类型阶段/能力清单与确定性 readiness evaluator，使后续实现和测试可追踪 |
| P1 | 是否依赖 Runtime 实现 | `core-agent-app` 保持应用层元数据无关，不反向依赖业务 Runtime |
| P1 | 阶段能否跳级 | readiness 同时检查当前阶段能力和前置阶段；允许并行开发，但不能宣称跳级完成 |
| P1 | 是否新增持久化 | 路线图是静态产品合同，不新增数据库表 |

## Acceptance

- Phase 0～6、CLI/Desktop/Web/IDE 表面和每阶段核心能力均有强类型表达。
- readiness 能稳定报告缺失能力和未满足前置阶段。
- 后续 P 可复用同一合同而无需修改 Kernel 或业务 Runtime。

## Residual risks

- 产品合同验证“能力是否声明为已实现”，不替代具体功能的 E2E 测试。
- 能力粒度会随后续详细规格演进，但已有枚举语义不得静默改变。
