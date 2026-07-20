<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ArrowUp,
  Copy,
  FileCode,
  FolderOpen,
  Gauge,
  Globe2,
  MessageSquare,
  Moon,
  Plus,
  RefreshCw,
  Settings,
  Slash,
  Sun,
  Trash2,
} from "lucide-vue-next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { TauriDesktopApi } from "./api";
import { createDesktopController, type WorkspaceKind } from "./controller";
import ApprovalDialog from "./components/ApprovalDialog.vue";
import CollaborationWorkspace from "./components/CollaborationWorkspace.vue";
import ContextChip from "./components/ContextChip.vue";
import EcosystemWorkspace from "./components/EcosystemWorkspace.vue";
import EmptyState from "./components/EmptyState.vue";
import EnterpriseWorkspace from "./components/EnterpriseWorkspace.vue";
import PanelShell from "./components/PanelShell.vue";
import ProjectTree from "./components/ProjectTree.vue";
import SidebarNav from "./components/SidebarNav.vue";
import StudioWorkspace from "./components/StudioWorkspace.vue";
import TraceList from "./components/TraceList.vue";
import type { ApprovalRequest, ModelSetting, ProjectNode } from "./types";
import {
  applyPromptCompletion,
  commandCompletions,
  contextCompletions,
  mentionQueryAtCursor,
  type PromptCompletion,
} from "./prompt-completion";

const api = new TauriDesktopApi();
const controller = createDesktopController(api);
const prompt = ref("");
const promptInput = ref<HTMLTextAreaElement>();
const completions = ref<PromptCompletion[]>([]);
const completionHint = ref("");
const selectedCompletion = ref(0);
const selectedPath = ref("");
const copiedMessageId = ref("");
const selectionMenu = ref<{
  visible: boolean;
  text: string;
  x: number;
  y: number;
  sourcePath?: string;
  startLine?: number;
  endLine?: number;
}>({ visible: false, text: "", x: 0, y: 0 });
const openingWorkspace = ref(false);
const savingSettings = ref(false);
const pendingApproval = ref<ApprovalRequest>();
const decidingApproval = ref(false);
const usageModel = ref("all");
let completionTimer: ReturnType<typeof setTimeout> | undefined;
let completionRequest = 0;
let closeApprovals: (() => void) | undefined;

const copy = {
  "zh-CN": {
    projects: "项目",
    sessions: "会话",
    newSession: "新建会话",
    openFolder: "打开文件夹",
    emptySession: "还没有会话",
    welcome: "今天想让 Agent 完成什么？",
    welcomeDetail: "使用 @ 添加项目上下文，使用 / 执行内置命令。",
    placeholder: "向 Agent 提问，@文件 补充上下文，/ 查看命令",
    files: "项目文件",
    settings: "设置",
    models: "模型配置",
    usage: "消耗统计",
    compression: "压缩配置",
    save: "保存并应用",
    activeModel: "当前模型",
    addModel: "添加模型",
    language: "语言",
    theme: "主题",
    connected: "已连接",
    offline: "离线",
    context: "上下文",
    elapsed: "本次耗时",
    input: "输入",
    output: "输出",
    requestHistory: "最近请求",
  },
  en: {
    projects: "Projects",
    sessions: "Sessions",
    newSession: "New session",
    openFolder: "Open folder",
    emptySession: "No sessions yet",
    welcome: "What should the Agent work on?",
    welcomeDetail: "Use @ for project context and / for built-in commands.",
    placeholder: "Ask Agent, use @file for context, / for commands",
    files: "Project files",
    settings: "Settings",
    models: "Model configuration",
    usage: "Usage",
    compression: "Compression",
    save: "Save and apply",
    activeModel: "Active model",
    addModel: "Add model",
    language: "Language",
    theme: "Theme",
    connected: "Connected",
    offline: "Offline",
    context: "Context",
    elapsed: "Elapsed",
    input: "Input",
    output: "Output",
    requestHistory: "Recent requests",
  },
} as const;

const t = computed(() => copy[controller.state.language]);
const otherRecentProjects = computed(() =>
  controller.state.recentProjects.filter(
    (project) => project.path !== controller.state.snapshot.workspacePath,
  ),
);
const contextPercent = computed(() => {
  const usage = controller.state.snapshot.contextUsage;
  return usage && usage.maxTokens > 0
    ? Math.min(100, Math.round((usage.totalTokens / usage.maxTokens) * 100))
    : 0;
});
const contextStyle = computed(() => ({
  background: `conic-gradient(var(--accent) ${contextPercent.value}%, var(--bg-tertiary) 0)`,
}));
const usageModels = computed(() => [
  ...new Set((controller.state.usage?.buckets ?? []).map((bucket) => bucket.modelName)),
]);
const visibleUsageBuckets = computed(() =>
  (controller.state.usage?.buckets ?? []).filter(
    (bucket) => usageModel.value === "all" || bucket.modelName === usageModel.value,
  ),
);
const dailyUsage = computed(() => {
  const values = new Map<string, number>();
  for (const bucket of visibleUsageBuckets.value) {
    values.set(bucket.day, (values.get(bucket.day) ?? 0) + bucket.totalTokens);
  }
  const maximum = Math.max(1, ...values.values());
  return Array.from({ length: 35 }, (_, offset) => {
    const date = new Date();
    date.setDate(date.getDate() - (34 - offset));
    const day = localDay(date);
    const tokens = values.get(day) ?? 0;
    return { day, tokens, level: Math.ceil((tokens / maximum) * 4) };
  });
});
const chartUsage = computed(() => {
  const values = new Map<string, { input: number; output: number }>();
  for (const bucket of visibleUsageBuckets.value) {
    const value = values.get(bucket.day) ?? { input: 0, output: 0 };
    value.input += bucket.promptTokens;
    value.output += bucket.completionTokens;
    values.set(bucket.day, value);
  }
  const rows = [...values.entries()].slice(-14).map(([day, value]) => ({ day, ...value }));
  const maximum = Math.max(1, ...rows.map((row) => row.input + row.output));
  return rows.map((row) => ({
    ...row,
    inputHeight: (row.input / maximum) * 100,
    outputHeight: (row.output / maximum) * 100,
  }));
});

onMounted(async () => {
  closeApprovals = await api.subscribeApprovals((request) => (pendingApproval.value = request));
  document.addEventListener("selectionchange", handleSelectionChange);
  await controller.load();
});

onBeforeUnmount(() => {
  if (completionTimer) clearTimeout(completionTimer);
  closeApprovals?.();
  controller.dispose();
});

watch(prompt, refreshCompletions);
watch(
  () => controller.state.activeWorkspace,
  (workspace) => {
    if (workspace === "settings") void controller.loadSettings();
  },
);

function refreshCompletions(value: string) {
  completionRequest += 1;
  const request = completionRequest;
  if (completionTimer) clearTimeout(completionTimer);
  completions.value = commandCompletions(value, controller.state.snapshot.commands);
  completionHint.value = "";
  selectedCompletion.value = 0;
  if (completions.value.length) return;
  const mention = mentionQueryAtCursor(value);
  if (!mention) return;
  if ([...mention.query].length < 3) {
    completionHint.value = "Type at least 3 characters after @.";
    return;
  }
  completionHint.value = "Searching…";
  completionTimer = setTimeout(async () => {
    try {
      const result = await api.searchContext(mention.query, 100);
      if (request !== completionRequest || prompt.value !== value) return;
      completions.value = contextCompletions(value, result.matches);
      completionHint.value = result.matches.length ? `${result.matches.length} matches` : "No match";
    } catch (error) {
      if (request === completionRequest) {
        completionHint.value = error instanceof Error ? error.message : "Search failed";
      }
    }
  }, 80);
}

function applyCompletion(index = selectedCompletion.value) {
  const completion = completions.value[index];
  if (!completion) return;
  prompt.value = applyPromptCompletion(prompt.value, completion);
  completions.value = [];
  completionHint.value = "";
  void nextTick(() => promptInput.value?.focus());
}

function handlePromptKeydown(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (completions.value.length && event.key === "ArrowDown") {
    event.preventDefault();
    selectedCompletion.value = (selectedCompletion.value + 1) % completions.value.length;
  } else if (completions.value.length && event.key === "ArrowUp") {
    event.preventDefault();
    selectedCompletion.value = (selectedCompletion.value + completions.value.length - 1) % completions.value.length;
  } else if (completions.value.length && (event.key === "Tab" || (event.key === "Enter" && !event.shiftKey))) {
    event.preventDefault();
    applyCompletion();
  } else if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    void send();
  }
}

async function send() {
  const value = prompt.value;
  if (!value.trim()) return;
  prompt.value = "";
  await controller.send(value);
}

function insertPrompt(value: string) {
  if (prompt.value && !prompt.value.endsWith(" ")) prompt.value += " ";
  prompt.value += value;
  void nextTick(() => promptInput.value?.focus());
}

function selectNode(node: ProjectNode) {
  selectedPath.value = node.path;
}

function insertSelectedPath() {
  if (!selectedPath.value) return;
  insertPrompt(`@"${selectedPath.value.replaceAll('"', '\\"')}" `);
}

async function chooseWorkspace() {
  if (openingWorkspace.value) return;
  openingWorkspace.value = true;
  try {
    const selected = await openDialog({ directory: true, multiple: false, title: "Open Agent Workspace" });
    if (typeof selected === "string") await controller.openWorkspace(selected);
  } finally {
    openingWorkspace.value = false;
  }
}

async function copyMessage(id: string, content: string) {
  await navigator.clipboard.writeText(content);
  copiedMessageId.value = id;
  setTimeout(() => (copiedMessageId.value = ""), 1_500);
}

function handleSelectionChange() {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || !selection.toString().trim()) {
    selectionMenu.value.visible = false;
    return;
  }
  const text = selection.toString().trim();
  if (text.length > 500) {
    selectionMenu.value.visible = false;
    return;
  }
  const range = selection.getRangeAt(0);
  const rect = range.getBoundingClientRect();
  selectionMenu.value = {
    visible: true,
    text,
    x: rect.left + rect.width / 2,
    y: rect.top - 10,
    sourcePath: undefined,
    startLine: undefined,
    endLine: undefined,
  };
}

function addSelectionReference() {
  if (!selectionMenu.value.text) return;
  controller.addReference({
    referenceType: "SELECTION",
    content: selectionMenu.value.text,
    path: selectionMenu.value.sourcePath,
    startLine: selectionMenu.value.startLine,
    endLine: selectionMenu.value.endLine,
  });
  selectionMenu.value.visible = false;
  window.getSelection()?.removeAllRanges();
}

function quoteMessage(messageId: string) {
  insertPrompt(`@message:${messageId} `);
}

async function decideApproval(decision: "ALLOW_ONCE" | "DENY") {
  if (!pendingApproval.value || decidingApproval.value) return;
  decidingApproval.value = true;
  try {
    await api.decideApproval(pendingApproval.value.id, decision);
    pendingApproval.value = undefined;
  } finally {
    decidingApproval.value = false;
  }
}

function addModel() {
  const settings = controller.state.settings;
  if (!settings) return;
  settings.models.push({
    provider: "openai-compatible",
    baseURL: "https://api.example.com/v1",
    name: `model-${settings.models.length + 1}`,
    profile: "",
    maxContextTokens: 128_000,
    apiKeyConfigured: false,
    apiKey: "",
  });
}

function removeModel(model: ModelSetting) {
  const settings = controller.state.settings;
  if (!settings || settings.models.length === 1 || settings.activeModel === model.name) return;
  settings.models = settings.models.filter((value) => value !== model);
}

async function saveSettings() {
  const settings = controller.state.settings;
  if (!settings || savingSettings.value) return;
  savingSettings.value = true;
  try {
    await controller.saveSettings(settings.activeModel, settings.models, settings.compression);
  } finally {
    savingSettings.value = false;
  }
}

function selectWorkspace(workspace: WorkspaceKind) {
  controller.selectWorkspace(workspace);
}

function changePermission(event: Event) {
  void controller.setPermissionMode((event.target as HTMLSelectElement).value);
}

function formatDuration(milliseconds?: number) {
  if (milliseconds == null) return "—";
  return milliseconds < 60_000
    ? `${(milliseconds / 1000).toFixed(1)}s`
    : `${Math.floor(milliseconds / 60_000)}m ${Math.floor((milliseconds % 60_000) / 1000)}s`;
}

interface ContentSegment {
  type: "text" | "file-link" | "code-block";
  text: string;
  path?: string;
  line?: number;
  display?: string;
  code?: string;
  language?: string;
  sourcePath?: string;
}

function parseContentSegments(text: string): ContentSegment[] {
  const segments: ContentSegment[] = [];
  // Match code blocks first: ```lang:path ... ```
  const codeBlockRe = /```(\w+)?(?::([\w./\\-]+))?\n([\s\S]*?)```/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = codeBlockRe.exec(text)) !== null) {
    // Emit any text before this code block
    if (match.index > lastIndex) {
      const before = text.slice(lastIndex, match.index);
      segments.push(...parseFileLinks(before));
    }
    const language = match[1] || "";
    const sourcePath = match[2] || "";
    const code = match[3];
    segments.push({
      type: "code-block",
      text: match[0],
      code,
      language,
      sourcePath,
      display: sourcePath || "",
    });
    lastIndex = match.index + match[0].length;
  }
  // Emit remaining text
  if (lastIndex < text.length) {
    segments.push(...parseFileLinks(text.slice(lastIndex)));
  }
  return segments.length ? segments : [{ type: "text", text }];
}

function parseFileLinks(text: string): ContentSegment[] {
  const segments: ContentSegment[] = [];
  const filePathRe = /((?:@?[\w./-]+\.[a-z]+)(?::(\d+)(?:-\d+)?)?)/gi;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = filePathRe.exec(text)) !== null) {
    if (match.index > lastIndex) {
      segments.push({ type: "text", text: text.slice(lastIndex, match.index) });
    }
    const full = match[1];
    const pathPart = full.startsWith("@") ? full.slice(1) : full;
    const linePart = match[2] ? parseInt(match[2], 10) : undefined;
    const [filePath] = pathPart.split(":");
    segments.push({
      type: "file-link",
      text: full,
      path: filePath,
      line: linePart,
      display: full,
    });
    lastIndex = match.index + full.length;
  }
  if (lastIndex < text.length) {
    segments.push({ type: "text", text: text.slice(lastIndex) });
  }
  return segments;
}

function openFileFromSegment(segment: ContentSegment) {
  if (segment.path) controller.openFile(segment.path, segment.line);
}

function formatTokens(tokens: number) {
  return tokens >= 1_000_000
    ? `${(tokens / 1_000_000).toFixed(1)}M`
    : tokens >= 1_000
      ? `${(tokens / 1_000).toFixed(1)}K`
      : `${tokens}`;
}

function localDay(date: Date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
</script>

<template>
  <main class="desktop-shell" :data-theme="controller.state.theme" :lang="controller.state.language">
    <ApprovalDialog v-if="pendingApproval" :request="pendingApproval" :busy="decidingApproval" @decide="decideApproval" />
    <SidebarNav :active="controller.state.activeWorkspace" :locale="controller.state.language" @select="selectWorkspace" />

    <aside class="project-session-rail">
      <header>
        <strong>{{ t.projects }}</strong>
        <button class="icon-button" :title="t.openFolder" :disabled="openingWorkspace || controller.state.sending" @click="chooseWorkspace"><Plus :size="15" /></button>
      </header>
      <button class="project-entry active"><span>{{ controller.state.snapshot.projectName }}</span><small>{{ controller.state.connected ? t.connected : t.offline }}</small></button>
      <button v-for="project in otherRecentProjects" :key="project.path" class="project-entry" @click="controller.openWorkspace(project.path)"><span>{{ project.name }}</span><small>{{ project.path }}</small></button>
      <div class="rail-section-title"><strong>{{ t.sessions }}</strong><button class="icon-button" :title="t.newSession" @click="controller.newSession"><Plus :size="14" /></button></div>
      <div class="session-list rail-sessions">
        <button v-for="session in controller.state.snapshot.sessions" :key="session.sessionId" class="session-row" :class="{ active: session.sessionId === controller.state.currentSessionId }" @click="controller.selectSession(session.sessionId)">
          <span><strong>{{ session.title }}</strong><small>{{ session.updatedAt }}</small></span>
        </button>
        <small v-if="!controller.state.snapshot.sessions.length" class="rail-empty">{{ t.emptySession }}</small>
      </div>
    </aside>

    <section class="workspace-shell">
      <header class="topbar compact-topbar">
        <div class="project-heading"><span class="eyebrow">Core Agent</span><h1>{{ controller.state.snapshot.projectName }}</h1></div>
        <div class="runtime-identities">
          <select aria-label="Permission mode" :value="controller.state.snapshot.permissionMode" :disabled="controller.state.sending" @change="changePermission">
            <option value="strict">strict</option><option value="risk-based">risk-based</option><option value="auto">auto</option>
          </select>
          <span class="badge badge-neutral">{{ controller.state.snapshot.model }}</span>
          <button class="icon-button" :title="t.theme" @click="controller.setTheme(controller.state.theme === 'dark' ? 'light' : 'dark')"><Sun v-if="controller.state.theme === 'dark'" :size="15" /><Moon v-else :size="15" /></button>
          <button class="icon-button" :title="t.language" @click="controller.setLanguage(controller.state.language === 'zh-CN' ? 'en' : 'zh-CN')"><Globe2 :size="15" /></button>
          <button class="icon-button" :title="t.settings" @click="selectWorkspace('settings')"><Settings :size="15" /></button>
        </div>
      </header>

      <div v-if="controller.state.error" class="error-banner" role="alert"><span>{{ controller.state.error }}</span><button class="button button-emphasis" @click="controller.load"><RefreshCw :size="14" /> Retry</button></div>

      <section v-if="controller.state.activeWorkspace === 'console'" class="chat-workspace">
        <div class="conversation">
          <div v-if="!controller.state.conversation.length" class="welcome-copy"><span class="brand-orb">A</span><h2>{{ t.welcome }}</h2><p>{{ t.welcomeDetail }}</p></div>
          <article v-for="item in controller.state.conversation" :key="item.id" class="message" :class="item.role">
            <header class="message-header"><span>{{ item.role }}</span><span class="message-actions">
              <button v-if="item.role === 'agent' || item.role === 'user'" :aria-label="`Quote ${item.role} message`" title="Reference this message in context" @click="quoteMessage(item.id)"><MessageSquare :size="12" /> Quote</button>
              <button :aria-label="`Copy ${item.role} message`" @click="copyMessage(item.id, item.content)"><Copy :size="12" />{{ copiedMessageId === item.id ? "Copied" : "Copy" }}</button>
            </span></header>
            <div class="message-content">
              <template v-for="(segment, i) in parseContentSegments(item.content)" :key="i">
                <span v-if="segment.type === 'text'">{{ segment.text }}</span>
                <a v-else-if="segment.type === 'file-link'" class="file-link" href="#" @click.prevent="openFileFromSegment(segment)">
                  <FileCode :size="12" />{{ segment.display }}
                </a>
                <div v-else-if="segment.type === 'code-block'" class="code-block-wrapper">
                  <div v-if="segment.sourcePath" class="code-block-source">
                    <FileCode :size="11" />
                    <a href="#" class="file-link" @click.prevent="controller.openFile(segment.sourcePath!)">{{ segment.sourcePath }}</a>
                  </div>
                  <pre class="code-block"><code>{{ segment.code }}</code></pre>
                </div>
              </template>
            </div>
          </article>
        </div>

        <!-- Selection context menu -->
        <div
          v-if="selectionMenu.visible"
          class="selection-menu"
          :style="{ left: `${selectionMenu.x}px`, top: `${selectionMenu.y}px` }"
          @click.stop
        >
          <button @click="addSelectionReference">
            <Code :size="12" /> Add to context
          </button>
        </div>

        <form class="prompt-box chat-prompt" @submit.prevent="send">
          <ContextChip
            :references="controller.state.contextReferences"
            :total-tokens="controller.state.snapshot.contextUsage?.totalTokens"
            @remove="(id: string) => controller.state.contextReferences = controller.state.contextReferences.filter(r => r.id !== id)"
            @open="(ref) => openFileFromSegment({ type: 'file-link', text: ref.locator.path || '', path: ref.locator.path, line: ref.locator.startLine, display: ref.locator.path })"
          />
          <div v-if="completions.length || completionHint" class="prompt-completions" role="listbox">
            <button v-for="(completion, index) in completions" :key="`${completion.kind}:${completion.label}`" type="button" :class="{ selected: index === selectedCompletion }" @mousedown.prevent="applyCompletion(index)"><strong>{{ completion.label }}</strong><span>{{ completion.detail }}</span></button>
            <small v-if="completionHint">{{ completionHint }}</small>
          </div>
          <textarea ref="promptInput" v-model="prompt" rows="3" :placeholder="t.placeholder" @keydown="handlePromptKeydown" />
          <div class="prompt-toolbar">
            <button type="button" class="prompt-tool" title="Add context" @click="insertPrompt('@')"><Plus :size="14" /></button>
            <button type="button" class="prompt-tool" title="Commands" @click="insertPrompt('/')"><Slash :size="14" /></button>
            <div class="context-meter" :style="contextStyle" role="meter" :aria-valuenow="contextPercent" aria-valuemin="0" aria-valuemax="100" tabindex="0">
              <span />
              <div class="context-tooltip"><strong>{{ t.context }} {{ contextPercent }}%</strong><small>{{ formatTokens(controller.state.snapshot.contextUsage?.totalTokens ?? 0) }} / {{ formatTokens(controller.state.snapshot.contextUsage?.maxTokens ?? 128000) }} tokens · estimated</small></div>
            </div>
            <span v-if="controller.state.sending" class="live-elapsed"><Gauge :size="13" />{{ formatDuration(controller.state.requestElapsedMs) }}</span>
            <button class="button button-primary send-button" :disabled="controller.state.sending || !prompt.trim()" aria-label="Send message"><ArrowUp :size="16" /></button>
          </div>
        </form>
      </section>

      <section v-else-if="controller.state.activeWorkspace === 'settings'" class="settings-workspace">
        <header class="settings-hero"><div><span class="eyebrow">{{ t.settings }}</span><h2>{{ t.models }}</h2><p>{{ controller.state.settings?.path }}</p><div class="config-sources"><span v-for="source in controller.state.settings?.sources" :key="`${source.priority}:${source.provider}`" class="badge badge-neutral">{{ source.provider }} · {{ source.priority }}</span></div></div><button class="button button-primary" :disabled="savingSettings || !controller.state.settings" @click="saveSettings">{{ savingSettings ? "Saving…" : t.save }}</button></header>
        <template v-if="controller.state.settings">
          <PanelShell :title="t.models" :count="controller.state.settings.models.length">
            <div class="model-list">
              <article v-for="model in controller.state.settings.models" :key="model.name" class="model-card">
                <label>Name<input v-model="model.name" required /></label>
                <label>Base URL<input v-model="model.baseURL" required /></label>
                <label>Provider<input v-model="model.provider" required /></label>
                <label>Max context<input v-model.number="model.maxContextTokens" type="number" min="1" required /></label>
                <label class="form-wide">API Key<input v-model="model.apiKey" type="password" :placeholder="model.apiKeyConfigured ? 'Configured — leave blank to keep' : 'Required'" /></label>
                <footer><label class="radio-label"><input v-model="controller.state.settings.activeModel" type="radio" :value="model.name" />{{ t.activeModel }}</label><button class="icon-button danger" :disabled="controller.state.settings.models.length === 1 || controller.state.settings.activeModel === model.name" @click="removeModel(model)"><Trash2 :size="14" /></button></footer>
              </article>
              <button class="add-model-card" @click="addModel"><Plus :size="18" />{{ t.addModel }}</button>
            </div>
          </PanelShell>
          <div class="settings-grid">
            <PanelShell :title="t.compression">
              <div class="settings-form">
                <label>Strategy<select v-model="controller.state.settings.compression.strategy"><option value="recent-window">Recent window</option><option value="extractive-summary">Extractive summary</option></select></label>
                <label>Trigger %<input v-model.number="controller.state.settings.compression.triggerPercent" type="number" min="1" max="100" /></label>
                <label>Keep recent messages<input v-model.number="controller.state.settings.compression.keepRecentMessages" type="number" min="1" /></label>
              </div>
            </PanelShell>
            <PanelShell :title="t.usage">
              <div class="usage-filter"><label>Model<select v-model="usageModel"><option value="all">All models</option><option v-for="name in usageModels" :key="name" :value="name">{{ name }}</option></select></label></div>
              <div class="usage-summary"><span>{{ formatTokens(visibleUsageBuckets.reduce((sum, row) => sum + row.promptTokens, 0)) }}<small>{{ t.input }}</small></span><span>{{ formatTokens(visibleUsageBuckets.reduce((sum, row) => sum + row.completionTokens, 0)) }}<small>{{ t.output }}</small></span></div>
              <div class="usage-calendar" aria-label="Usage calendar"><i v-for="day in dailyUsage" :key="day.day" :class="`level-${day.level}`" :title="`${day.day}: ${day.tokens} tokens`" /></div>
              <div class="usage-chart"><span v-for="row in chartUsage" :key="row.day" :title="`${row.day}: ${row.input + row.output}`"><i class="input" :style="{ height: `${row.inputHeight}%` }" /><i class="output" :style="{ height: `${row.outputHeight}%` }" /></span></div>
            </PanelShell>
          </div>
          <PanelShell :title="t.requestHistory" :count="controller.state.usage?.requests.length ?? 0">
            <div class="request-table"><div v-for="request in controller.state.usage?.requests.slice(0, 20)" :key="request.id"><span><strong>{{ request.modelName }}</strong><small>{{ request.entrypoint }} · {{ request.status }}</small></span><span>{{ formatDuration(request.wallDurationMs) }}<small>active {{ formatDuration(request.activeDurationMs) }}</small></span><span>{{ formatTokens(request.contextTokens) }}<small>context</small></span></div></div>
          </PanelShell>
        </template>
        <EmptyState v-else title="Loading settings" detail="Reading the shared Terminal/Desktop configuration." />
      </section>

      <section v-else class="focused-workspace">
        <PanelShell v-if="controller.state.activeWorkspace === 'project'" :title="t.files" :count="controller.state.snapshot.projectTree.length"><ProjectTree :nodes="controller.state.snapshot.projectTree" @select="selectNode" /></PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'trace'" title="Trace"><TraceList :steps="controller.state.snapshot.trace" /></PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'changes'" title="Changes" :count="controller.state.snapshot.changes.length"><ul class="change-list"><li v-for="change in controller.state.snapshot.changes" :key="change.path"><span class="change-status">{{ change.status }}</span><strong>{{ change.path }}</strong><span class="diff-count"><b>+{{ change.additions }}</b>-{{ change.deletions }}</span></li></ul></PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'tools'" title="Tools" :count="controller.state.snapshot.tools.length"><div class="tool-strip"><span v-for="tool in controller.state.snapshot.tools" :key="tool.key" class="tool-pill"><i :class="tool.state" />{{ tool.name }}<small>{{ tool.state }}</small></span></div></PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'memory'" title="Memory" :count="controller.state.snapshot.memory.length"><div class="card-grid"><article v-for="item in controller.state.snapshot.memory" :key="item.id" class="info-card"><span class="badge badge-neutral">{{ item.kind }}</span><h3>{{ item.title }}</h3><p>{{ item.summary }}</p></article></div></PanelShell>
        <PanelShell v-else-if="controller.state.activeWorkspace === 'sessions'" :title="t.sessions" :count="controller.state.snapshot.sessions.length"><div class="session-list"><button v-for="session in controller.state.snapshot.sessions" :key="session.sessionId" class="session-row" @click="controller.selectSession(session.sessionId)"><span><strong>{{ session.title }}</strong><small>{{ session.updatedAt }}</small></span><span class="badge badge-neutral">{{ session.state }}</span></button></div></PanelShell>
        <StudioWorkspace v-else-if="controller.state.activeWorkspace === 'studio'" />
        <CollaborationWorkspace v-else-if="controller.state.activeWorkspace === 'collaboration'" />
        <EnterpriseWorkspace v-else-if="controller.state.activeWorkspace === 'enterprise'" />
        <EcosystemWorkspace v-else-if="controller.state.activeWorkspace === 'ecosystem'" />
        <PanelShell v-else :title="controller.state.activeWorkspace"><EmptyState title="Advanced workspace" detail="This capability remains available outside the primary conversation." /></PanelShell>
      </section>

      <footer class="statusbar"><span><i :class="{ online: controller.state.connected }" />{{ controller.state.connected ? t.connected : t.offline }}</span><span>Session {{ controller.state.currentSessionId?.slice(0, 8) ?? "—" }}</span><span>{{ t.context }} {{ contextPercent }}%</span><span>{{ t.elapsed }} {{ formatDuration(controller.state.sending ? controller.state.requestElapsedMs : controller.state.lastWallDurationMs) }}</span></footer>
    </section>

    <aside class="file-rail">
      <header><strong>{{ t.files }}</strong><button class="icon-button" :title="t.openFolder" @click="chooseWorkspace"><FolderOpen :size="14" /></button></header>
      <ProjectTree v-if="controller.state.snapshot.projectTree.length" :nodes="controller.state.snapshot.projectTree" @select="selectNode" />
      <EmptyState v-else title="No files" detail="Open a workspace to browse files." />
      <button v-if="selectedPath" class="selected-context" @click="insertSelectedPath"><Plus :size="13" />@{{ selectedPath }}</button>
    </aside>
  </main>
</template>
