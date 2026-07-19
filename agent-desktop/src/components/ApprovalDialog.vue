<script setup lang="ts">
import type { ApprovalRequest } from "../types";

withDefaults(defineProps<{ request: ApprovalRequest; busy?: boolean }>(), { busy: false });
defineEmits<{ decide: [decision: "ALLOW_ONCE" | "DENY"] }>();
</script>

<template>
  <section class="approval-overlay" role="dialog" aria-modal="true" aria-labelledby="approval-title">
    <article class="approval-dialog">
      <span class="badge badge-accent">{{ request.risk }} RISK</span>
      <h2 id="approval-title">Allow {{ request.tool }}?</h2>
      <p>{{ request.reason }}</p>
      <pre>{{ JSON.stringify(request.parameters, null, 2) }}</pre>
      <footer>
        <button class="button" :disabled="busy" @click="$emit('decide', 'DENY')">Deny</button>
        <button class="button button-primary" :disabled="busy" @click="$emit('decide', 'ALLOW_ONCE')">Allow once</button>
      </footer>
    </article>
  </section>
</template>
