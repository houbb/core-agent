<script setup lang="ts">
import {
  Bot,
  Brain,
  Building2,
  FileClock,
  FolderTree,
  GitCompareArrows,
  ListChecks,
  Settings,
  SquareActivity,
  PanelsTopLeft,
  UsersRound,
  Wrench,
  Store,
} from "lucide-vue-next";
import { computed } from "vue";
import type { Component } from "vue";
import type { WorkspaceKind } from "../controller";

const props = withDefaults(defineProps<{ active: WorkspaceKind; locale?: "zh-CN" | "en" }>(), {
  locale: "en",
});
defineEmits<{ select: [workspace: WorkspaceKind] }>();

const definitions: Array<{ id: WorkspaceKind; labels: [string, string]; icon: Component }> = [
  { id: "console", labels: ["对话", "Console"], icon: Bot },
  { id: "project", labels: ["项目", "Project"], icon: FolderTree },
  { id: "changes", labels: ["变更", "Changes"], icon: GitCompareArrows },
  { id: "trace", labels: ["追踪", "Trace"], icon: SquareActivity },
  { id: "tools", labels: ["工具", "Tools"], icon: Wrench },
  { id: "memory", labels: ["记忆", "Memory"], icon: Brain },
  { id: "sessions", labels: ["会话", "Sessions"], icon: FileClock },
  { id: "plan", labels: ["计划", "Plan"], icon: ListChecks },
  { id: "studio", labels: ["工作室", "Studio"], icon: PanelsTopLeft },
  { id: "collaboration", labels: ["协作", "Team"], icon: UsersRound },
  { id: "enterprise", labels: ["企业", "Enterprise"], icon: Building2 },
  { id: "ecosystem", labels: ["生态", "Ecosystem"], icon: Store },
  { id: "settings", labels: ["设置", "Settings"], icon: Settings },
];

const items = computed(() => definitions.map((item) => ({
  ...item,
  label: item.labels[props.locale === "zh-CN" ? 0 : 1],
})));
</script>

<template>
  <nav class="sidebar" aria-label="Workspace navigation">
    <div class="brand-mark" aria-label="AgentOS">A</div>
    <button
      v-for="item in items"
      :key="item.id"
      class="nav-button"
      :class="{ active: item.id === active }"
      :aria-current="item.id === active ? 'page' : undefined"
      :aria-label="item.label"
      :title="item.label"
      @click="$emit('select', item.id)"
    >
      <component :is="item.icon" :size="18" :stroke-width="1.7" />
      <span>{{ item.label }}</span>
    </button>
  </nav>
</template>
