<script setup lang="ts">
import {
  Bot,
  Brain,
  Building2,
  FileClock,
  FolderTree,
  GitCompareArrows,
  Settings,
  SquareActivity,
  PanelsTopLeft,
  UsersRound,
  Wrench,
  Store,
} from "lucide-vue-next";
import type { Component } from "vue";
import type { WorkspaceKind } from "../controller";

defineProps<{ active: WorkspaceKind }>();
defineEmits<{ select: [workspace: WorkspaceKind] }>();

const items: Array<{ id: WorkspaceKind; label: string; icon: Component }> = [
  { id: "console", label: "Console", icon: Bot },
  { id: "project", label: "Project", icon: FolderTree },
  { id: "changes", label: "Changes", icon: GitCompareArrows },
  { id: "trace", label: "Trace", icon: SquareActivity },
  { id: "tools", label: "Tools", icon: Wrench },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "sessions", label: "Sessions", icon: FileClock },
  { id: "studio", label: "Studio", icon: PanelsTopLeft },
  { id: "collaboration", label: "Team", icon: UsersRound },
  { id: "enterprise", label: "Enterprise", icon: Building2 },
  { id: "ecosystem", label: "Ecosystem", icon: Store },
  { id: "settings", label: "Settings", icon: Settings },
];
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
