import { reactive } from "vue";
import type { DesktopApi } from "./api";
import type {
  CompressionSetting,
  ConversationItem,
  ModelSetting,
  SettingsSnapshot,
  TraceStep,
  UiPreference,
  UsageSnapshot,
  WorkspaceSnapshot,
} from "./types";

export type WorkspaceKind =
  | "console"
  | "project"
  | "changes"
  | "trace"
  | "tools"
  | "memory"
  | "sessions"
  | "studio"
  | "collaboration"
  | "enterprise"
  | "ecosystem"
  | "settings";

const emptySnapshot = (): WorkspaceSnapshot => ({
  projectName: "Workspace",
  workspacePath: "",
  profile: "Coder",
  model: "Unavailable",
  projectTree: [],
  commands: [],
  changes: [],
  trace: [],
  memory: [],
  tools: [],
  sessions: [],
  resumeSession: false,
  permissionMode: "risk-based",
  configSources: [],
  effectiveConfig: {},
  contextUsage: undefined,
});

export function createDesktopController(api: DesktopApi) {
  let closeEvents: (() => void) | undefined;
  let requestTimer: ReturnType<typeof setInterval> | undefined;
  let generation = 0;
  const state = reactive({
    activeWorkspace: "console" as WorkspaceKind,
    loading: false,
    sending: false,
    error: "",
    connected: false,
    snapshot: emptySnapshot(),
    conversation: [] as ConversationItem[],
    currentSessionId: undefined as string | undefined,
    requestElapsedMs: 0,
    lastWallDurationMs: undefined as number | undefined,
    lastActiveDurationMs: undefined as number | undefined,
    telemetryRecorded: undefined as boolean | undefined,
    settings: undefined as SettingsSnapshot | undefined,
    usage: undefined as UsageSnapshot | undefined,
    preferences: [] as UiPreference[],
    recentProjects: [] as Array<{ name: string; path: string }>,
    theme: "dark" as "light" | "dark",
    language: "zh-CN" as "zh-CN" | "en",
  });

  async function load() {
    state.loading = true;
    state.error = "";
    try {
      state.snapshot = await api.loadWorkspace(state.currentSessionId);
      if (state.snapshot.resumeSession) {
        state.currentSessionId ??= state.snapshot.sessions[0]?.sessionId;
      }
      if (state.currentSessionId) {
        state.conversation = await (api.loadSession?.(state.currentSessionId) ?? Promise.resolve([]));
        closeEvents?.();
        closeEvents = api.subscribe(state.currentSessionId, appendTrace);
      }
      state.connected = true;
      await loadPreferences().catch(() => undefined);
    } catch (error) {
      state.connected = false;
      state.error = error instanceof Error ? error.message : "Unable to load workspace";
    } finally {
      state.loading = false;
    }
  }

  async function send(message: string) {
    if (state.sending) return;
    const text = message.trim();
    if (!text) return;
    state.sending = true;
    const activeGeneration = generation;
    const started = performance.now();
    state.requestElapsedMs = 0;
    requestTimer = setInterval(() => {
      state.requestElapsedMs = Math.max(0, Math.round(performance.now() - started));
    }, 100);
    state.error = "";
    state.conversation.push({ id: crypto.randomUUID(), role: "user", content: text });
    try {
      const submission = await api.sendMessage(text, state.currentSessionId);
      if (activeGeneration !== generation) return;
      if (submission.action === "new-session") {
        closeEvents?.();
        closeEvents = undefined;
        state.currentSessionId = undefined;
        state.snapshot.trace = [];
        state.conversation = [];
      } else if (submission.action === "clear-view") {
        state.conversation = [];
      }
      if (submission.response) {
        state.conversation.push({
          id: crypto.randomUUID(),
          role: submission.action === "none" ? "agent" : "system",
          content: submission.response,
        });
      }
      if (submission.sessionId) {
        state.currentSessionId = submission.sessionId;
        closeEvents?.();
        closeEvents = api.subscribe(submission.sessionId, appendTrace);
      } else if (!submission.response) {
        throw new Error("Agent did not return a session or command response");
      }
      state.connected = true;
      const measuredDuration = Math.round(performance.now() - started);
      state.lastWallDurationMs = submission.wallDurationMs ?? measuredDuration;
      state.lastActiveDurationMs = submission.activeDurationMs ?? measuredDuration;
      state.telemetryRecorded = submission.telemetryRecorded;
      state.requestElapsedMs = state.lastWallDurationMs;
      if (submission.sessionId) {
        state.snapshot = await api.loadWorkspace(submission.sessionId);
      }
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to send message";
    } finally {
      if (requestTimer) clearInterval(requestTimer);
      requestTimer = undefined;
      state.sending = false;
    }
  }

  async function openWorkspace(path: string) {
    generation += 1;
    const wasConnected = state.connected;
    state.loading = true;
    state.error = "";
    try {
      await api.openWorkspace(path);
      closeEvents?.();
      closeEvents = undefined;
      state.currentSessionId = undefined;
      state.conversation = [];
      state.snapshot = emptySnapshot();
      state.connected = true;
    } catch (error) {
      state.connected = wasConnected;
      state.error = error instanceof Error ? error.message : "Unable to open workspace";
      return;
    } finally {
      state.loading = false;
    }
    await load();
  }

  async function selectSession(sessionId: string) {
    if (state.sending || sessionId === state.currentSessionId) return;
    generation += 1;
    closeEvents?.();
    closeEvents = undefined;
    state.currentSessionId = sessionId;
    state.loading = true;
    state.error = "";
    try {
      const [snapshot, conversation] = await Promise.all([
        api.loadWorkspace(sessionId),
        api.loadSession?.(sessionId) ?? Promise.resolve([]),
      ]);
      state.snapshot = snapshot;
      state.conversation = conversation;
      closeEvents = api.subscribe(sessionId, appendTrace);
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to load session";
    } finally {
      state.loading = false;
    }
  }

  async function newSession() {
    await send("/new");
  }

  async function loadPreferences() {
    if (!api.listPreferences) return;
    state.preferences = await api.listPreferences();
    state.recentProjects = state.preferences
      .filter((item) => item.kind === "RECENT_PROJECT")
      .sort((left, right) => (right.updatedAt ?? "").localeCompare(left.updatedAt ?? ""))
      .map((item) => item.value as { name: string; path: string })
      .filter((item) => typeof item?.path === "string");
    const theme = state.preferences.find((item) => item.key === "appearance.theme")?.value;
    const language = state.preferences.find((item) => item.key === "appearance.language")?.value;
    if (theme === "light" || theme === "dark") state.theme = theme;
    if (language === "zh-CN" || language === "en") state.language = language;
  }

  async function setPreference(key: string, kind: string, value: unknown) {
    if (!api.savePreference) return;
    const current = state.preferences.find((item) => item.key === key);
    const saved = await api.savePreference({
      key,
      kind,
      value,
      expectedVersion: current?.version,
    });
    const index = state.preferences.findIndex((item) => item.key === key);
    if (index >= 0) state.preferences[index] = saved;
    else state.preferences.push(saved);
  }

  async function setTheme(theme: "light" | "dark") {
    await setPreference("appearance.theme", "THEME", theme);
    state.theme = theme;
  }

  async function setLanguage(language: "zh-CN" | "en") {
    await setPreference("appearance.language", "LANGUAGE", language);
    state.language = language;
  }

  async function loadSettings() {
    state.error = "";
    try {
      if (api.loadSettings) state.settings = await api.loadSettings();
      if (api.loadUsage) state.usage = await api.loadUsage();
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to load settings";
    }
  }

  async function saveSettings(
    activeModel: string,
    models: ModelSetting[],
    compression: CompressionSetting,
  ) {
    if (!api.saveSettings || !state.settings) return;
    state.error = "";
    try {
      state.settings = await api.saveSettings({
        fingerprint: state.settings.fingerprint,
        activeModel,
        models,
        compression,
      });
      await load();
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to save settings";
    }
  }

  async function setPermissionMode(mode: string) {
    if (!api.setPermissionMode || state.sending) return;
    try {
      state.snapshot.permissionMode = await api.setPermissionMode(mode);
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to change permissions";
    }
  }

  function appendTrace(event: TraceStep) {
    const index = state.snapshot.trace.findIndex((item) => item.id === event.id);
    if (index >= 0) state.snapshot.trace[index] = event;
    else state.snapshot.trace.push(event);
    if (event.kind === "response" && event.summary) {
      state.conversation.push({
        id: event.id,
        role: "agent",
        content: event.summary,
      });
    }
  }

  function selectWorkspace(workspace: WorkspaceKind) {
    state.activeWorkspace = workspace;
  }

  function dispose() {
    closeEvents?.();
    if (requestTimer) clearInterval(requestTimer);
  }

  return {
    state,
    load,
    openWorkspace,
    selectSession,
    newSession,
    send,
    appendTrace,
    selectWorkspace,
    setTheme,
    setLanguage,
    loadSettings,
    saveSettings,
    setPermissionMode,
    dispose,
  };
}
