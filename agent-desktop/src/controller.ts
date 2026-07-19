import { reactive } from "vue";
import type { DesktopApi } from "./api";
import type { ConversationItem, TraceStep, WorkspaceSnapshot } from "./types";

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
});

export function createDesktopController(api: DesktopApi) {
  let closeEvents: (() => void) | undefined;
  const state = reactive({
    activeWorkspace: "console" as WorkspaceKind,
    loading: false,
    sending: false,
    error: "",
    connected: false,
    snapshot: emptySnapshot(),
    conversation: [] as ConversationItem[],
    currentSessionId: undefined as string | undefined,
  });

  async function load() {
    state.loading = true;
    state.error = "";
    try {
      state.snapshot = await api.loadWorkspace(state.currentSessionId);
      if (state.snapshot.resumeSession) {
        state.currentSessionId ??= state.snapshot.sessions[0]?.sessionId;
      }
      state.connected = true;
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
    state.error = "";
    state.conversation.push({ id: crypto.randomUUID(), role: "user", content: text });
    try {
      const submission = await api.sendMessage(text, state.currentSessionId);
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
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to send message";
    } finally {
      state.sending = false;
    }
  }

  async function openWorkspace(path: string) {
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
  }

  return { state, load, openWorkspace, send, appendTrace, selectWorkspace, dispose };
}
