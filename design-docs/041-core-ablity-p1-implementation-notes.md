# P1 Implementation Notes

## Changes

### New crates (3)
- **core-agent-question** — Human-in-the-loop interaction: CHOICE/CONFIRM/INPUT/APPROVAL/REVIEW types, async channel-based ask/answer
- **core-agent-todo** — User-visible progress tracking: Todo items with PENDING/IN_PROGRESS/COMPLETED/CANCELLED status
- **core-agent-reflection** — Self-evaluation: score 0-100, issues, suggestions, retry limits, threshold checks

### Enhanced existing crate
- **core-agent-plan** — Added `LLMPlanBuilder`: accepts PlanDraft (from LLM JSON output), implements PlanBuilder trait, key="llm"

### Integration
- **enterprise.rs** — Added question/todo/reflection to EnterpriseRuntimes, initialized in with_model_and_telemetry()
- **renderer.rs** — Enhanced event renderer to display `todo_list` and `reflection_completed` events with formatting

### Tests
- 3 new unit tests per crate (9 total)
- 4 E2E tests: todo lifecycle, question lifecycle, reflection evaluation, full P1 workflow simulation

## Key Decisions

1. **Planner**: LLM generates PlanDraft JSON → LLMPlanBuilder wraps it → PlanningManager::create_plan_from_draft() validates and persists
2. **Task**: Reuses core-agent-execution's ExecutionManager (no new crate)
3. **Question**: Independent QuestionRuntime with oneshot channels for ask/answer pattern
4. **Todo**: Lightweight, user-facing only, does not participate in execution
5. **Reflection**: Whole-plan-level, rule-based evaluator for MVP

## Data flow

```
User input
  ↓ (plan_mode=true)
LLM generates PlanDraft JSON
  ↓
LLMPlanBuilder → PlanningManager::create_plan_from_draft()
  ↓
Plan validated (Review → Ready)
  ↓
TodoManager: from_task_names() creates todo items
  ↓
ExecutionManager: execute plan steps
  ↓ (if uncertain)
QuestionManager: ask() → user answer()
  ↓ (all steps done)
ReflectionManager: evaluate() → score + suggestions
```

## Remaining unknowns

- LLM's PlanDraft JSON generation quality (depends on model)
- Desktop GUI for Plan/Todo/Question (P2)
- Reflection using LLM instead of rule-based (future enhancement)