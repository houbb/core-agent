<script setup lang="ts">
defineProps<{ title: string; count?: number; state?: string }>();
</script>

<template>
  <section class="panel-shell">
    <header class="panel-header">
      <div class="panel-title-row">
        <h2>{{ title }}</h2>
        <span v-if="count !== undefined" class="badge badge-neutral">{{ count }}</span>
      </div>
      <span v-if="state" class="panel-state">{{ state }}</span>
      <slot name="actions" />
    </header>
    <div class="panel-content"><slot /></div>
  </section>
</template>
