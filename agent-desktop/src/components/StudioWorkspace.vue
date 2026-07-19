<script setup lang="ts">
import { computed, onMounted, reactive } from "vue";
import {
  Activity,
  Bot,
  Boxes,
  Brain,
  Braces,
  Cable,
  Database,
  Home,
  Network,
  Plus,
} from "lucide-vue-next";
import { HttpStudioApi } from "../studio-api";
import { createStudioController } from "../studio-controller";
import type { StudioAsset, StudioSection } from "../studio-types";
import EmptyState from "./EmptyState.vue";
import PanelShell from "./PanelShell.vue";
import VisualPanelRenderer from "./VisualPanelRenderer.vue";

const controller = createStudioController(new HttpStudioApi());
const form = reactive({ name: "", role: "architect", model: "default", memory: "project", tools: "git,filesystem" });

const navigation = [
  { id: "home", label: "Home", icon: Home },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "workflow", label: "Workflow", icon: Network },
  { id: "prompt", label: "Prompt", icon: Braces },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "capability", label: "Capabilities", icon: Cable },
  { id: "knowledge", label: "Knowledge", icon: Database },
  { id: "trace", label: "Trace", icon: Activity },
  { id: "model", label: "Models", icon: Boxes },
] as const;

const assets = computed<StudioAsset[]>(() => {
  const key = `${controller.state.section === "workflow" ? "workflows" : controller.state.section === "prompt" ? "prompts" : controller.state.section === "memory" ? "memories" : controller.state.section === "capability" ? "capabilities" : controller.state.section === "knowledge" ? "knowledge" : controller.state.section === "trace" ? "traces" : controller.state.section === "model" ? "models" : "agents"}` as keyof typeof controller.state.snapshot;
  const value = controller.state.snapshot[key];
  return Array.isArray(value) ? (value as StudioAsset[]) : [];
});

onMounted(controller.load);

async function createAgent() {
  await controller.createAgent({
    name: form.name,
    role: form.role,
    model: form.model,
    memory: form.memory,
    tools: form.tools.split(",").map((value) => value.trim()).filter(Boolean),
  });
  form.name = "";
}

function select(section: StudioSection) {
  controller.select(section);
}
</script>

<template>
  <section class="studio-workspace">
    <aside class="studio-nav">
      <div class="studio-heading"><span class="eyebrow">Developer Platform</span><h2>Agent Studio</h2></div>
      <button v-for="item in navigation" :key="item.id" :class="{ active: controller.state.section === item.id }" @click="select(item.id)">
        <component :is="item.icon" :size="15" />{{ item.label }}
      </button>
    </aside>

    <div class="studio-content">
      <div v-if="controller.state.error" class="error-banner"><span>{{ controller.state.error }}</span><button class="button button-emphasis" @click="controller.load">Retry</button></div>
      <template v-if="controller.state.section === 'home'">
        <header class="studio-hero"><div><span class="eyebrow">Agent IDE</span><h2>Design, debug and operate reusable Agents.</h2><p>Runtime capabilities stay in core-agent. Studio turns them into governed assets and visual panels.</p></div><button class="button button-primary" @click="select('agents')"><Plus :size="15" />New Agent</button></header>
        <div class="studio-metrics">
          <article><span>Agents</span><strong>{{ controller.state.snapshot.agents.length }}</strong><small>Reusable identities</small></article>
          <article><span>Workflows</span><strong>{{ controller.state.snapshot.workflows.length }}</strong><small>Runtime definitions</small></article>
          <article><span>Capabilities</span><strong>{{ controller.state.snapshot.capabilities.length }}</strong><small>Registered providers</small></article>
          <article><span>Visual Panels</span><strong>{{ controller.state.snapshot.panels.length }}</strong><small>Auto-composed</small></article>
        </div>
        <div class="visual-panel-grid">
          <VisualPanelRenderer v-for="panel in controller.state.snapshot.panels" :key="panel.id" :descriptor="panel" />
          <EmptyState v-if="!controller.state.snapshot.panels.length" title="No Visual Runtime panels" detail="Registered Runtime descriptors will auto-compose here." />
        </div>
      </template>

      <template v-else-if="controller.state.section === 'agents'">
        <div class="designer-grid">
          <PanelShell title="Agent Designer">
            <form class="designer-form" @submit.prevent="createAgent">
              <label><span>Name</span><input v-model="form.name" required maxlength="256" placeholder="Coding Agent" /></label>
              <label><span>Role</span><select v-model="form.role"><option>architect</option><option>coder</option><option>reviewer</option><option>sre</option></select></label>
              <label><span>Model</span><input v-model="form.model" required maxlength="256" /></label>
              <label><span>Memory</span><input v-model="form.memory" required maxlength="256" /></label>
              <label class="form-wide"><span>Tools</span><input v-model="form.tools" placeholder="git,filesystem,shell" /></label>
              <div class="form-actions form-wide"><span>Saved as a versioned core-agent asset.</span><button class="button button-primary" :disabled="controller.state.saving || !form.name.trim()">{{ controller.state.saving ? "Saving…" : "Create Agent" }}</button></div>
            </form>
          </PanelShell>
          <PanelShell title="Agent Assets" :count="controller.state.snapshot.agents.length">
            <div class="asset-list"><article v-for="agent in controller.state.snapshot.agents" :key="agent.id"><span class="badge badge-accent">{{ agent.state }}</span><h3>{{ agent.name }}</h3><p>{{ agent.description || 'No description' }}</p><small>v{{ agent.version }}</small></article></div>
            <EmptyState v-if="!controller.state.snapshot.agents.length" title="No Agents yet" detail="Create the first versioned Agent asset." />
          </PanelShell>
        </div>
      </template>

      <template v-else>
        <PanelShell :title="navigation.find((item) => item.id === controller.state.section)?.label ?? 'Studio'" :count="assets.length">
          <div v-if="controller.state.section === 'workflow' && assets.length" class="workflow-list">
            <article v-for="workflow in assets" :key="workflow.id"><header><strong>{{ workflow.name }}</strong><span class="badge badge-neutral">v{{ workflow.version }}</span></header><div class="workflow-nodes"><span v-for="node in workflow.nodes ?? []" :key="node.id"><b>{{ node.kind }}</b>{{ node.label }}</span></div></article>
          </div>
          <div v-else class="asset-grid"><article v-for="asset in assets" :key="asset.id" class="info-card"><span class="badge badge-neutral">{{ asset.state }}</span><h3>{{ asset.name }}</h3><p>{{ asset.description || 'No description' }}</p><small>Version {{ asset.version }}</small></article></div>
          <EmptyState v-if="!assets.length" :title="`No ${controller.state.section} assets`" detail="Assets owned by core-agent will appear here." />
        </PanelShell>
      </template>
    </div>
  </section>
</template>
