<script setup lang="ts">
import { X, FileCode, MessageSquare, Code, BarChart3 } from "lucide-vue-next";
import type { ContextReference } from "../types";

const props = defineProps<{
  references: ContextReference[];
  totalTokens?: number;
}>();
const emit = defineEmits<{
  remove: [id: string];
  open: [ref: ContextReference];
}>();

function refIcon(type: string) {
  switch (type) {
    case "FILE":
      return FileCode;
    case "SELECTION":
      return Code;
    case "MESSAGE":
      return MessageSquare;
    default:
      return FileCode;
  }
}

function formatTokens(tokens: number) {
  return tokens >= 1_000_000
    ? `${(tokens / 1_000_000).toFixed(1)}M`
    : tokens >= 1_000
      ? `${(tokens / 1_000).toFixed(1)}K`
      : `${tokens}`;
}
</script>

<template>
  <div v-if="references.length" class="context-chip-bar">
    <div
      v-for="ref in references"
      :key="ref.id"
      class="context-chip"
      :class="`chip-${ref.referenceType.toLowerCase()}`"
      @click="emit('open', ref)"
    >
      <component :is="refIcon(ref.referenceType)" :size="12" />
      <span class="chip-label">{{ ref.locator.path || "Selection" }}</span>
      <span v-if="ref.locator.startLine" class="chip-line"
        >L{{ ref.locator.startLine }}{{ ref.locator.endLine ? `-${ref.locator.endLine}` : "" }}</span
      >
      <button
        class="chip-remove"
        title="Remove context"
        @click.stop="emit('remove', ref.id)"
      >
        <X :size="12" />
      </button>
    </div>
    <div v-if="totalTokens !== undefined" class="chip-token-count" title="Estimated context tokens">
      <BarChart3 :size="11" />
      <span>{{ formatTokens(totalTokens) }}</span>
    </div>
  </div>
</template>