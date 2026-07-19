<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { RegisteredVisualPanel, VisualAction } from "../studio-types";
import { runtimeRequest } from "../runtime-request";
import EmptyState from "./EmptyState.vue";
import PanelShell from "./PanelShell.vue";

const props = defineProps<{ descriptor: RegisteredVisualPanel }>();
const rows = ref<Array<Record<string, unknown>>>([]);
const error = ref("");
const loading = ref(false);
let timer: number | undefined;

async function load() {
  const endpoint = props.descriptor.panel.data_source.endpoint;
  if (!endpoint.startsWith("/api/") || endpoint.includes("..") || endpoint.includes("?") || endpoint.includes("#")) {
    error.value = "Visual descriptor endpoint was rejected.";
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    const body = await runtimeRequest<unknown>(endpoint);
    const values = Array.isArray(body) ? body : (body as { items?: unknown[] }).items ?? [];
    rows.value = values.slice(0, 500).filter((value): value is Record<string, unknown> => Boolean(value) && typeof value === "object");
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : "Unable to load panel";
  } finally {
    loading.value = false;
  }
}

async function execute(action: VisualAction) {
  if (!action.endpoint.startsWith("/api/") || action.endpoint.includes("..") || action.endpoint.includes("?") || action.endpoint.includes("#")) {
    error.value = "Visual action endpoint was rejected.";
    return;
  }
  if (action.requires_approval && !window.confirm(`${action.label} requires approval. Continue?`)) return;
  const method = action.method.toUpperCase();
  await runtimeRequest(action.endpoint, { method });
  await load();
}

onMounted(() => {
  void load();
  const refresh = props.descriptor.panel.data_source.refresh_seconds;
  if (refresh) timer = window.setInterval(load, refresh * 1000);
});
onBeforeUnmount(() => timer && window.clearInterval(timer));
</script>

<template>
  <PanelShell :title="descriptor.panel.title" :count="rows.length" :state="loading ? 'Loading' : descriptor.runtime_id">
    <template #actions>
      <div class="visual-actions">
        <button
          v-for="action in descriptor.panel.actions"
          :key="action.key"
          class="button"
          :class="action.dangerous ? 'button-danger' : 'button-emphasis'"
          @click="execute(action)"
        >{{ action.label }}</button>
      </div>
    </template>
    <div v-if="error" class="inline-error">{{ error }}</div>
    <table v-if="rows.length && descriptor.panel.fields.length" class="visual-table">
      <thead><tr><th v-for="field in descriptor.panel.fields" :key="field.key">{{ field.label }}</th></tr></thead>
      <tbody>
        <tr v-for="(row, index) in rows" :key="String(row.id ?? index)">
          <td v-for="field in descriptor.panel.fields" :key="field.key">
            <span v-if="field.kind === 'Status'" class="badge badge-neutral">{{ String(row[field.key] ?? '—') }}</span>
            <code v-else-if="field.kind === 'Json'">{{ JSON.stringify(row[field.key] ?? null) }}</code>
            <span v-else>{{ String(row[field.key] ?? '—') }}</span>
          </td>
        </tr>
      </tbody>
    </table>
    <EmptyState v-else-if="!loading" title="No runtime data" :detail="descriptor.panel.description" />
  </PanelShell>
</template>
