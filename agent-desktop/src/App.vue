<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowUp, Circle, RefreshCw, Search } from "lucide-vue-next";
import { TauriDesktopApi } from "./api";
import { loadPreferences, savePreference } from "./bridge";
import { createDesktopController, type WorkspaceKind } from "./controller";
import EmptyState from "./components/EmptyState.vue";
import PanelShell from "./components/PanelShell.vue";
import ProjectTree from "./components/ProjectTree.vue";
import SidebarNav from "./components/SidebarNav.vue";
import TraceList from "./components/TraceList.vue";
import StudioWorkspace from "./components/StudioWorkspace.vue";
import CollaborationWorkspace from "./components/CollaborationWorkspace.vue";
import ApprovalDialog from "./components/ApprovalDialog.vue";
import EnterpriseWorkspace from "./components/EnterpriseWorkspace.vue";
import EcosystemWorkspace from "./components/EcosystemWorkspace.vue";
import type { ApprovalRequest, ProjectNode } from "./types";

const api = new TauriDesktopApi();
const controller = createDesktopController(api);
const prompt = ref("");
const selectedPath = ref("");
const theme = ref("obsidian-gold");
const bottomTab = ref<"tools" | "log">("tools");
const activeTitle = computed(() => controller.state.activeWorkspace[0].toUpperCase() + controller.state.activeWorkspace.slice(1));
const pendingApproval = ref<ApprovalRequest>();
const decidingApproval = ref(false);
let closeApprovals: (() => void) | undefined;

onMounted(async () => {
  const preferences = await loadPreferences();
  const storedTheme = preferences.find((item) => item.key === "theme.current");
  if (storedTheme && typeof storedTheme.value === "object" && storedTheme.value) {
    const value = (storedTheme.value as { name?: string }).name;
    if (value) theme.value = value;
  }
  closeApprovals = await api.subscribeApprovals((request) => {
    pendingApproval.value = request;
  });
  await controller.load();
});
onBeforeUnmount(() => {
  closeApprovals?.();
  controller.dispose();
});

async function send() {
  const value = prompt.value;
  prompt.value = "";
  await controller.send(value);
}

function selectNode(node: ProjectNode) {
  selectedPath.value = node.path;
}

async function changeTheme() {
  theme.value = theme.value === "obsidian-gold" ? "graphite" : "obsidian-gold";
  await savePreference("theme.current", "THEME", { name: theme.value });
}

function selectWorkspace(workspace: WorkspaceKind) {
  controller.selectWorkspace(workspace);
}

async function decideApproval(decision: "ALLOW_ONCE" | "DENY") {
  const request = pendingApproval.value;
  if (!request || decidingApproval.value) return;
  decidingApproval.value = true;
  try {
    await api.decideApproval(request.id, decision);
    pendingApproval.value = undefined;
  } finally {
    decidingApproval.value = false;
  }
}
</script>

<template>
  <main class="desktop-shell" :data-theme="theme">
    <ApprovalDialog v-if="pendingApproval" :request="pendingApproval" :busy="decidingApproval" @decide="decideApproval" />
    <SidebarNav :active="controller.state.activeWorkspace" @select="selectWorkspace" />
    <section class="workspace-shell">
      <header class="topbar">
        <div>
          <span class="eyebrow">{{ activeTitle }} Workspace</span>
          <h1>{{ controller.state.snapshot.projectName }}</h1>
        </div>
        <label class="search-box">
          <Search :size="15" />
          <span class="sr-only">Search workspace</span>
          <input placeholder="Search files, traces, sessions" />
          <kbd>⌘ K</kbd>
        </label>
        <div class="runtime-identities">
          <span class="badge badge-accent">{{ controller.state.snapshot.profile }}</span>
          <span class="badge badge-neutral">{{ controller.state.snapshot.model }}</span>
          <span class="connection-dot" :class="{ online: controller.state.connected }">
            <Circle :size="7" fill="currentColor" />
            {{ controller.state.connected ? "Connected" : "Offline" }}
          </span>
        </div>
      </header>

      <div v-if="controller.state.error" class="error-banner" role="alert">
        <span>{{ controller.state.error }}</span>
        <button class="button button-emphasis" :disabled="controller.state.loading" @click="controller.load">
          <RefreshCw :size="14" /> Retry
        </button>
      </div>

      <section v-if="controller.state.activeWorkspace === 'console'" class="console-workspace">
        <PanelShell title="Project" :count="controller.state.snapshot.projectTree.length">
          <ProjectTree
            v-if="controller.state.snapshot.projectTree.length"
            :nodes="controller.state.snapshot.projectTree"
            @select="selectNode"
          />
          <EmptyState v-else title="No project context" detail="Connect the Agent API to load the project tree." />
          <div v-if="selectedPath" class="selection-context">Context: {{ selectedPath }}</div>
        </PanelShell>

        <PanelShell title="Agent Console" :state="controller.state.sending ? 'Running' : 'Ready'">
          <div class="conversation">
            <div v-if="!controller.state.conversation.length" class="welcome-copy">
              <span class="eyebrow">Developer workspace</span>
              <h3>What should the Agent work on?</h3>
              <p>Project context, execution trace and changes stay visible while the task runs.</p>
            </div>
            <article v-for="item in controller.state.conversation" :key="item.id" class="message" :class="item.role">
              <span>{{ item.role }}</span>
              <p>{{ item.content }}</p>
            </article>
          </div>
          <form class="prompt-box" @submit.prevent="send">
            <textarea v-model="prompt" rows="2" placeholder="Ask AgentOS to analyze, change, or verify this project…" />
            <button class="button button-primary send-button" :disabled="controller.state.sending || !prompt.trim()" aria-label="Send message">
              <ArrowUp :size="16" />
            </button>
          </form>
        </PanelShell>

        <PanelShell title="Trace" :count="controller.state.snapshot.trace.length">
          <TraceList v-if="controller.state.snapshot.trace.length" :steps="controller.state.snapshot.trace" />
          <EmptyState v-else title="No execution trace" detail="Planner, model, tool and memory steps will appear here." />
        </PanelShell>

        <PanelShell title="Changes" :count="controller.state.snapshot.changes.length">
          <ul v-if="controller.state.snapshot.changes.length" class="change-list">
            <li v-for="change in controller.state.snapshot.changes" :key="change.path">
              <span class="change-status">{{ change.status }}</span>
              <strong>{{ change.path }}</strong>
              <span class="diff-count"><b>+{{ change.additions }}</b> −{{ change.deletions }}</span>
            </li>
          </ul>
          <EmptyState v-else title="Working tree unchanged" detail="Agent edits and Git diff summaries will appear here." />
        </PanelShell>

        <PanelShell class="bottom-panel" title="Execution">
          <template #actions>
            <div class="segmented-control">
              <button :class="{ active: bottomTab === 'tools' }" @click="bottomTab = 'tools'">Tools</button>
              <button :class="{ active: bottomTab === 'log' }" @click="bottomTab = 'log'">Log</button>
            </div>
          </template>
          <div v-if="bottomTab === 'tools'" class="tool-strip">
            <span v-for="tool in controller.state.snapshot.tools" :key="tool.key" class="tool-pill">
              <i :class="tool.state.toLowerCase()" />{{ tool.name }}<small>{{ tool.state }}</small>
            </span>
            <EmptyState v-if="!controller.state.snapshot.tools.length" title="No tool status" detail="Registered Tool Runtime providers will appear here." />
          </div>
          <EmptyState v-else title="No runtime log" detail="Bounded execution observations will appear here." />
        </PanelShell>
      </section>

      <section v-else class="focused-workspace">
        <PanelShell v-if="controller.state.activeWorkspace === 'project'" title="Project Explorer" :count="controller.state.snapshot.projectTree.length">
          <ProjectTree v-if="controller.state.snapshot.projectTree.length" :nodes="controller.state.snapshot.projectTree" @select="selectNode" />
          <EmptyState v-else title="No project context" detail="Index a project to browse its structure." />
        </PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'changes'" title="Changes" :count="controller.state.snapshot.changes.length">
          <ul class="change-list"><li v-for="change in controller.state.snapshot.changes" :key="change.path"><strong>{{ change.path }}</strong><span class="diff-count"><b>+{{ change.additions }}</b> −{{ change.deletions }}</span></li></ul>
          <EmptyState v-if="!controller.state.snapshot.changes.length" title="No changes" detail="The working tree has no reported Agent edits." />
        </PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'trace'" title="Trace Explorer" :count="controller.state.snapshot.trace.length">
          <TraceList :steps="controller.state.snapshot.trace" />
          <EmptyState v-if="!controller.state.snapshot.trace.length" title="No trace" detail="Start an Agent execution to inspect its path." />
        </PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'tools'" title="Tool Explorer" :count="controller.state.snapshot.tools.length">
          <div class="card-grid"><article v-for="tool in controller.state.snapshot.tools" :key="tool.key" class="info-card"><span class="badge badge-neutral">{{ tool.state }}</span><h3>{{ tool.name }}</h3><p>{{ tool.key }}</p></article></div>
          <EmptyState v-if="!controller.state.snapshot.tools.length" title="No tools" detail="Tool providers will appear after the API connects." />
        </PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'memory'" title="Memory Explorer" :count="controller.state.snapshot.memory.length">
          <div class="card-grid"><article v-for="memory in controller.state.snapshot.memory" :key="memory.id" class="info-card"><span class="badge badge-accent">{{ memory.kind }}</span><h3>{{ memory.title }}</h3><p>{{ memory.summary }}</p></article></div>
          <EmptyState v-if="!controller.state.snapshot.memory.length" title="No visible memory" detail="Project facts and preferences will appear when available." />
        </PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'sessions'" title="Session Explorer" :count="controller.state.snapshot.sessions.length">
          <div class="session-list"><button v-for="session in controller.state.snapshot.sessions" :key="session.sessionId" class="session-row"><span><strong>{{ session.title }}</strong><small>{{ session.updatedAt }}</small></span><span class="badge badge-neutral">{{ session.state }}</span></button></div>
          <EmptyState v-if="!controller.state.snapshot.sessions.length" title="No sessions" detail="Recent Agent sessions will appear here." />
        </PanelShell>
        <StudioWorkspace v-else-if="controller.state.activeWorkspace === 'studio'" />
        <CollaborationWorkspace v-else-if="controller.state.activeWorkspace === 'collaboration'" />
        <EnterpriseWorkspace v-else-if="controller.state.activeWorkspace === 'enterprise'" />
        <EcosystemWorkspace v-else-if="controller.state.activeWorkspace === 'ecosystem'" />
        <PanelShell v-else title="Settings">
          <div class="settings-list">
            <div><span><strong>Theme</strong><small>Device-local appearance preference</small></span><button class="button button-emphasis" @click="changeTheme">{{ theme }}</button></div>
            <div><span><strong>Agent Runtime</strong><small>All modules run inside this desktop process</small></span><code>Embedded</code></div>
            <div><span><strong>Layout</strong><small>Single-window workspace panels</small></span><span class="badge badge-neutral">Default</span></div>
          </div>
        </PanelShell>
      </section>

      <footer class="statusbar">
        <span><i :class="{ online: controller.state.connected }" />{{ controller.state.connected ? "Runtime healthy" : "Runtime offline" }}</span>
        <span>Session {{ controller.state.currentSessionId?.slice(0, 8) ?? "—" }}</span>
        <span>{{ controller.state.snapshot.trace.reduce((sum, item) => sum + (item.tokens ?? 0), 0) }} tokens</span>
        <span>{{ controller.state.snapshot.trace.reduce((sum, item) => sum + (item.durationMs ?? 0), 0) }} ms</span>
      </footer>
    </section>
  </main>
</template>
