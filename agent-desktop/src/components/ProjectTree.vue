<script setup lang="ts">
import { ChevronRight, File, Folder } from "lucide-vue-next";
import { ref } from "vue";
import type { ProjectNode } from "../types";

defineOptions({ name: "ProjectTree" });
const props = defineProps<{ nodes: ProjectNode[]; level?: number }>();
const emit = defineEmits<{ select: [node: ProjectNode] }>();
const expanded = ref(new Set(props.level ? [] : props.nodes.filter((node) => node.kind === "directory").map((node) => node.id)));

function activate(node: ProjectNode) {
  if (node.kind === "directory") {
    const next = new Set(expanded.value);
    if (next.has(node.id)) next.delete(node.id);
    else next.add(node.id);
    expanded.value = next;
  }
  emit("select", node);
}
</script>

<template>
  <ul class="project-tree" :aria-label="level ? undefined : 'Project files'">
    <li v-for="node in nodes" :key="node.id">
      <button class="tree-row" :style="{ paddingInlineStart: `${(level ?? 0) * 12 + 8}px` }" @click="activate(node)">
        <ChevronRight
          v-if="node.kind === 'directory'"
          :size="13"
          :class="{ expanded: expanded.has(node.id) }"
        />
        <span v-else class="tree-spacer" />
        <Folder v-if="node.kind === 'directory'" :size="14" />
        <File v-else :size="14" />
        <span>{{ node.name }}</span>
      </button>
      <ProjectTree
        v-if="node.children?.length && expanded.has(node.id)"
        :nodes="node.children"
        :level="(level ?? 0) + 1"
        @select="emit('select', $event)"
      />
    </li>
  </ul>
</template>
