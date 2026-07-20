<script setup lang="ts">
import { X, FileCode, MessageSquare, Code } from "lucide-vue-next";
import type { ContextReference } from "../types";

defineProps<{
  references: ContextReference[];
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
</script>

<template>
  <div v-if="references.length" class="context-chip-bar">
    <div
      v-for="ref in references"
      :key="ref.id"
      class="context-chip"
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
  </div>
</template>