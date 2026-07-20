<script setup lang="ts">
import type { PlanSnapshot, TaskSnapshot } from "../types";

const props = defineProps<{
  plan: PlanSnapshot | null;
  plans: PlanSnapshot[];
  loading: boolean;
}>();

const emit = defineEmits<{
  selectPlan: [planId: string];
  approvePlan: [planId: string];
  cancelPlan: [planId: string];
  refresh: [];
}>();

function statusIcon(status: string): string {
  switch (status) {
    case "COMPLETED": return "✓";
    case "RUNNING":
    case "EXECUTING": return "⏳";
    case "READY": return "○";
    case "CANCELLED": return "✗";
    case "FAILED": return "⚠";
    default: return "○";
  }
}

function statusClass(status: string): string {
  switch (status) {
    case "COMPLETED": return "status-done";
    case "RUNNING":
    case "EXECUTING": return "status-running";
    case "CANCELLED":
    case "FAILED": return "status-error";
    default: return "status-pending";
  }
}
</script>

<template>
  <section class="plan-panel">
    <header class="plan-panel-header">
      <h2>Plan Mode</h2>
      <button class="button button-small" :disabled="loading" @click="$emit('refresh')">↻</button>
    </header>

    <div v-if="loading" class="plan-loading">Loading...</div>

    <div v-else-if="!plan" class="plan-empty">
      <p>No active plan. Use <code>/plan &lt;goal&gt;</code> to create one.</p>
    </div>

    <template v-else>
      <div class="plan-info">
        <span class="plan-status" :class="statusClass(plan.status)">{{ statusIcon(plan.status) }} {{ plan.status }}</span>
        <h3>{{ plan.goalTitle }}</h3>
        <p class="plan-meta">ID: {{ plan.id.slice(0, 8) }}… | v{{ plan.version }}</p>
      </div>

      <div class="plan-tasks">
        <div v-for="task in plan.tasks" :key="task.id" class="plan-task">
          <div class="task-header">
            <span class="task-status" :class="statusClass(task.status)">{{ statusIcon(task.status) }}</span>
            <span class="task-name">{{ task.name }}</span>
          </div>
          <div v-if="task.steps.length" class="task-steps">
            <div v-for="step in task.steps" :key="step.id" class="step-item">
              <span class="step-status" :class="statusClass(step.status)">{{ statusIcon(step.status) }}</span>
              <span class="step-name">{{ step.name }}</span>
            </div>
          </div>
        </div>
      </div>

      <div class="plan-actions">
        <button
          v-if="plan.status === 'READY' || plan.status === 'REVIEWING'"
          class="button button-primary"
          :disabled="loading"
          @click="$emit('approvePlan', plan.id)"
        >
          Approve & Execute
        </button>
        <button
          v-if="plan.status !== 'COMPLETED' && plan.status !== 'CANCELLED'"
          class="button"
          :disabled="loading"
          @click="$emit('cancelPlan', plan.id)"
        >
          Cancel
        </button>
      </div>
    </template>

    <div v-if="plans.length > 1" class="plan-history">
      <h4>History</h4>
      <div v-for="p in plans" :key="p.id" class="history-item" :class="{ active: plan?.id === p.id }" @click="$emit('selectPlan', p.id)">
        <span class="history-status" :class="statusClass(p.status)">{{ statusIcon(p.status) }}</span>
        <span class="history-title">{{ p.goalTitle }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.plan-panel {
  display: grid;
  gap: 12px;
  padding: 12px;
  height: 100%;
  overflow-y: auto;
}
.plan-panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.plan-panel-header h2 { font-size: 17px; font-weight: 700; margin: 0; }
.plan-loading, .plan-empty { color: var(--text-secondary); padding: 20px; text-align: center; }
.plan-info { background: var(--bg-secondary); border-radius: 10px; padding: 12px; }
.plan-info h3 { margin: 8px 0 4px; font-size: 15px; }
.plan-meta { font-size: 11px; color: var(--text-secondary); margin: 0; }
.plan-status { font-size: 12px; font-weight: 600; }
.plan-tasks { display: grid; gap: 8px; }
.plan-task { background: var(--bg-secondary); border-radius: 8px; padding: 8px 12px; }
.task-header { display: flex; align-items: center; gap: 8px; }
.task-name { font-size: 13px; font-weight: 500; }
.task-steps { margin-top: 6px; padding-left: 20px; display: grid; gap: 4px; }
.step-item { display: flex; align-items: center; gap: 6px; }
.step-name { font-size: 12px; color: var(--text-secondary); }
.task-status, .step-status { font-size: 12px; width: 16px; text-align: center; }
.plan-actions { display: flex; gap: 8px; }
.plan-history { border-top: 1px solid var(--border); padding-top: 8px; }
.plan-history h4 { font-size: 13px; font-weight: 600; margin: 0 0 8px; }
.history-item { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 6px; cursor: pointer; font-size: 12px; }
.history-item:hover { background: var(--bg-secondary); }
.history-item.active { background: var(--accent-bg); }
.history-status { font-size: 11px; width: 14px; text-align: center; }
.history-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.status-done { color: var(--green); }
.status-running { color: var(--accent); }
.status-error { color: var(--red); }
.status-pending { color: var(--text-secondary); }
</style>