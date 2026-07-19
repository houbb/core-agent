<script setup lang="ts">
defineProps<{ title: string; detail: string }>();
</script>

<template>
  <div class="empty-state" role="status">
    <strong>{{ title }}</strong>
    <span>{{ detail }}</span>
  </div>
</template>
