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
  changes: [],
  trace: [],
  memory: [],
  tools: [],
  sessions: [],
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
      state.currentSessionId ??= state.snapshot.sessions[0]?.sessionId;
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
      state.currentSessionId = submission.sessionId;
      closeEvents?.();
      closeEvents = api.subscribe(submission.sessionId, appendTrace);
      state.connected = true;
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to send message";
    } finally {
      state.sending = false;
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
  }

  return { state, load, send, appendTrace, selectWorkspace, dispose };
}
