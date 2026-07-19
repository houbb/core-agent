<script setup lang="ts">
import { Check, CircleDashed, Clock3, X } from "lucide-vue-next";
import type { TraceStep } from "../types";

defineProps<{ steps: TraceStep[] }>();

function stateIcon(state: string) {
  const normalized = state.toUpperCase();
  if (["COMPLETED", "SUCCESS"].includes(normalized)) return Check;
  if (["FAILED", "ERROR"].includes(normalized)) return X;
  if (["RUNNING", "STREAMING"].includes(normalized)) return CircleDashed;
  return Clock3;
}
</script>

<template>
  <ol class="trace-list">
    <li v-for="step in steps" :key="step.id" class="trace-step">
      <component :is="stateIcon(step.state)" :size="15" class="trace-icon" />
      <div class="trace-copy">
        <div class="trace-title">
          <strong>{{ step.title }}</strong>
          <span class="badge badge-neutral">{{ step.kind }}</span>
        </div>
        <p v-if="step.summary">{{ step.summary }}</p>
        <div class="trace-meta">
          <span>{{ step.state }}</span>
          <span v-if="step.durationMs !== undefined">{{ step.durationMs }} ms</span>
          <span v-if="step.tokens !== undefined">{{ step.tokens }} tokens</span>
        </div>
      </div>
    </li>
  </ol>
</template>
