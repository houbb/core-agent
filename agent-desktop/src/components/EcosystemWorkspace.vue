<script setup lang="ts">
import { computed, onMounted } from "vue";
import { Blocks, BookOpen, Boxes, Cloud, Code2, PackageCheck, Rocket, Store } from "lucide-vue-next";
import { HttpEcosystemApi } from "../ecosystem-api";
import { createEcosystemController } from "../ecosystem-controller";
import type { EcosystemSection } from "../ecosystem-types";
import EmptyState from "./EmptyState.vue";
import PanelShell from "./PanelShell.vue";

const controller = createEcosystemController(new HttpEcosystemApi());
const navigation = [
  { id: "marketplace", label: "Marketplace", icon: Store }, { id: "my-agents", label: "My Agents", icon: PackageCheck },
  { id: "capabilities", label: "Capabilities", icon: Blocks }, { id: "templates", label: "Templates", icon: Boxes },
  { id: "developer", label: "Developer", icon: Code2 }, { id: "publishing", label: "Publishing", icon: Rocket },
  { id: "community", label: "Community", icon: BookOpen }, { id: "cloud", label: "Cloud", icon: Cloud },
] as const;
const listed = computed(() => controller.state.snapshot.packages.filter((item) => item.state === "LISTED"));
const agents = computed(() => listed.value.filter((item) => item.kind === "AGENT"));
const capabilities = computed(() => listed.value.filter((item) => item.kind === "CAPABILITY"));
const templates = computed(() => listed.value.filter((item) => item.kind === "TEMPLATE"));
const publishing = computed(() => controller.state.snapshot.packages.filter((item) => item.state !== "LISTED"));
onMounted(controller.load);
function select(section: EcosystemSection) { controller.select(section); }
</script>

<template>
  <section class="ecosystem-workspace">
    <aside class="ecosystem-nav">
      <div class="studio-heading"><span class="eyebrow">AgentOS</span><h2>Ecosystem</h2></div>
      <button v-for="item in navigation" :key="item.id" :class="{ active: controller.state.section === item.id }" @click="select(item.id)">
        <component :is="item.icon" :size="15" />{{ item.label }}
        <span v-if="item.id === 'publishing' && publishing.length" class="badge badge-accent">{{ publishing.length }}</span>
      </button>
    </aside>
    <div class="ecosystem-content">
      <div v-if="controller.state.error" class="error-banner" role="alert"><span>{{ controller.state.error }}</span><button class="button button-emphasis" @click="controller.load">Retry</button></div>

      <template v-if="controller.state.section === 'marketplace'">
        <header class="ecosystem-hero"><div><span class="eyebrow">Build products, not prompts</span><h2>Discover the AgentOS ecosystem.</h2><p>Install governed Agents, Capabilities and Templates. Every listed version has passed independent publication review; runtime permissions still apply at install and execution.</p></div><span class="badge badge-accent">{{ listed.length }} verified packages</span></header>
        <div class="ecosystem-feature-grid">
          <article v-for="item in agents.slice(0, 3)" :key="item.id" class="market-card featured"><header><span class="package-icon">A</span><span class="badge badge-accent">Featured Agent</span></header><h3>{{ item.name }}</h3><p>{{ item.description }}</p><div class="package-meta"><span>{{ item.publisher }}</span><span>v{{ item.packageVersion }}</span><span>★ {{ item.rating ?? '—' }}</span></div><footer><small>{{ item.requiredCapabilities.length }} capabilities · {{ item.downloads }} installs</small><button class="button button-primary" :disabled="controller.state.mutating === item.id" @click="controller.install(item.id)">Install</button></footer></article>
        </div>
        <div class="ecosystem-home-grid">
          <PanelShell title="Capability Marketplace" :count="capabilities.length"><div class="compact-package-list"><article v-for="item in capabilities.slice(0, 5)" :key="item.id"><span class="package-icon capability">C</span><span><strong>{{ item.name }}</strong><small>{{ item.publisher }} · {{ item.key }}</small></span><button class="button button-emphasis" @click="controller.install(item.id)">Install</button></article></div><EmptyState v-if="!capabilities.length" title="No listed capabilities" detail="Reviewed Git, Browser, SQL and platform connectors will appear here." /></PanelShell>
          <PanelShell title="Updates" :count="controller.state.snapshot.updates.length"><div class="compact-package-list"><article v-for="item in controller.state.snapshot.updates" :key="item.id"><span class="package-icon">↑</span><span><strong>{{ item.name }}</strong><small>v{{ item.packageVersion }} available</small></span><button class="button button-emphasis" @click="controller.install(item.id)">Review update</button></article></div><EmptyState v-if="!controller.state.snapshot.updates.length" title="Everything is current" detail="Reviewed package updates will appear here." /></PanelShell>
        </div>
      </template>

      <PanelShell v-else-if="controller.state.section === 'my-agents'" title="Installed Agents" :count="controller.state.snapshot.installs.length">
        <div class="asset-grid"><article v-for="item in controller.state.snapshot.installs" :key="item.packageId" class="info-card"><span class="badge badge-neutral">{{ item.state }}</span><h3>{{ controller.state.snapshot.packages.find((pkg) => pkg.id === item.packageId)?.name ?? item.packageId }}</h3><p>Installed v{{ item.installedVersion }}</p><small>Updated {{ item.updatedAt }}</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.installs.length" title="No ecosystem installs" detail="Install a listed Agent or Capability from Marketplace." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'capabilities' || controller.state.section === 'templates'" :title="controller.state.section === 'capabilities' ? 'Capability Marketplace' : 'Template Center'" :count="(controller.state.section === 'capabilities' ? capabilities : templates).length">
        <div class="market-grid"><article v-for="item in (controller.state.section === 'capabilities' ? capabilities : templates)" :key="item.id" class="market-card"><header><span class="package-icon" :class="{ capability: item.kind === 'CAPABILITY' }">{{ item.kind.slice(0,1) }}</span><span class="badge badge-neutral">{{ item.kind }}</span></header><h3>{{ item.name }}</h3><p>{{ item.description }}</p><div class="package-meta"><span>{{ item.publisher }}</span><span>v{{ item.packageVersion }}</span><span>★ {{ item.rating ?? '—' }}</span></div><footer><small>{{ item.downloads }} installs</small><button class="button button-primary" @click="controller.install(item.id)">Install</button></footer></article></div>
        <EmptyState v-if="!(controller.state.section === 'capabilities' ? capabilities : templates).length" title="No listed packages" detail="Independently reviewed packages will appear after publication." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'developer'" title="Developer Center" :count="controller.state.snapshot.sdks.length">
        <header class="developer-intro"><span class="eyebrow">Runtime First · API First · Capability First</span><h2>Build on stable AgentOS contracts.</h2><p>SDK packages describe Agent, Tool, Capability, Workflow and Memory extension contracts. Local documentation paths never carry credentials.</p></header>
        <div class="sdk-grid"><article v-for="sdk in controller.state.snapshot.sdks" :key="sdk.key"><span class="package-icon">{{ sdk.language.slice(0,1) }}</span><div><h3>{{ sdk.name }}</h3><p>{{ sdk.language }} · v{{ sdk.version }}</p><code>{{ sdk.documentationPath }}</code></div></article></div>
        <EmptyState v-if="!controller.state.snapshot.sdks.length" title="No SDK metadata" detail="The Rust SDK is exposed when the Ecosystem API publishes its versioned contract." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'publishing'" title="Publishing Center" :count="publishing.length">
        <div class="enterprise-table-wrap"><table class="visual-table"><thead><tr><th>Package</th><th>Kind</th><th>Version</th><th>Publisher</th><th>State</th><th>Action</th></tr></thead><tbody><tr v-for="item in publishing" :key="item.id"><td><strong>{{ item.name }}</strong><small>{{ item.key }}</small></td><td>{{ item.kind }}</td><td>{{ item.packageVersion }}</td><td>{{ item.publisher }}</td><td><span class="badge badge-neutral">{{ item.state }}</span></td><td><button v-if="item.state === 'DRAFT'" class="button button-primary" :disabled="controller.state.mutating === item.id" @click="controller.submit(item.id)">Submit review</button><span v-else class="badge badge-accent">Independent review</span></td></tr></tbody></table></div>
        <EmptyState v-if="!publishing.length" title="No publication drafts" detail="Publisher-owned package drafts and review status appear here." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'community'" title="Community Signals" :count="listed.filter((item) => item.rating).length">
        <div class="market-grid"><article v-for="item in listed.filter((pkg) => pkg.rating)" :key="item.id" class="market-card"><header><span class="package-icon">{{ item.kind.slice(0,1) }}</span><strong>★ {{ item.rating }}</strong></header><h3>{{ item.name }}</h3><p>{{ item.description }}</p><small>{{ item.downloads }} governed installs</small></article></div>
        <EmptyState v-if="!listed.some((item) => item.rating)" title="No ratings yet" detail="Bounded one-to-five package ratings will appear here; discussions and issues remain external." />
      </PanelShell>

      <PanelShell v-else title="Cloud Center">
        <div class="cloud-boundary"><Cloud :size="36" /><h2>Cloud synchronization is not configured.</h2><p>Workspace, Session, Memory, Trace and Build remain device-local until a governed Cloud provider implements the open protocol.</p><span class="badge badge-neutral">No remote data</span></div>
      </PanelShell>
    </div>
  </section>
</template>
