import type {
  ChangeItem,
  MemoryItem,
  PlanSnapshot,
  ProjectNode,
  SessionItem,
  ToolStatus,
  TraceStep,
  WorkspaceSnapshot,
  ApprovalRequest,
  AgentSubmission,
  ContextCandidateSearch,
  ConversationItem,
  SettingsSnapshot,
  UsageSnapshot,
  UiPreference,
  ModelSetting,
  CompressionSetting,
  ContextReference,
} from "./types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

export interface DesktopApi {
  loadWorkspace(sessionId?: string): Promise<WorkspaceSnapshot>;
  openWorkspace(path: string): Promise<void>;
  searchContext(query: string, limit?: number): Promise<ContextCandidateSearch>;
  sendMessage(message: string, sessionId?: string): Promise<AgentSubmission>;
  openFile(path: string, line?: number): Promise<void>;
  addReference(request: {
    sessionId: string;
    referenceType: string;
    path?: string;
    startLine?: number;
    endLine?: number;
    content?: string;
    messageId?: string;
    snapshot?: string;
  }): Promise<ContextReference>;
  loadSession?(sessionId: string): Promise<ConversationItem[]>;
  loadSettings?(): Promise<SettingsSnapshot>;
  saveSettings?(request: {
    fingerprint?: string;
    activeModel: string;
    models: ModelSetting[];
    compression: CompressionSetting;
  }): Promise<SettingsSnapshot>;
  loadUsage?(): Promise<UsageSnapshot>;
  setPermissionMode?(mode: string): Promise<string>;
  listPreferences?(): Promise<UiPreference[]>;
  savePreference?(request: {
    key: string;
    kind: string;
    value: unknown;
    expectedVersion?: number;
  }): Promise<UiPreference>;
  loadPlan(planId: string): Promise<PlanSnapshot>;
  loadPlans(): Promise<PlanSnapshot[]>;
  approvePlan(planId: string): Promise<void>;
  subscribe(sessionId: string, onEvent: (event: TraceStep) => void): () => void;
}

export class TauriDesktopApi implements DesktopApi {
  async loadWorkspace(sessionId?: string): Promise<WorkspaceSnapshot> {
    return invoke<WorkspaceSnapshot>("agent_load_workspace", { sessionId });
  }

  async openWorkspace(path: string): Promise<void> {
    return invoke<void>("agent_open_workspace", { path });
  }

  async searchContext(query: string, limit = 100): Promise<ContextCandidateSearch> {
    return invoke<ContextCandidateSearch>("agent_context_candidates", { query, limit });
  }

  async sendMessage(message: string, sessionId?: string): Promise<AgentSubmission> {
    validateMessage(message);
    return invoke<AgentSubmission>("agent_send_message", {
      request: { message, sessionId },
    });
  }

  async loadSession(sessionId: string): Promise<ConversationItem[]> {
    return invoke<ConversationItem[]>("agent_load_session", { sessionId });
  }

  async loadSettings(): Promise<SettingsSnapshot> {
    return invoke<SettingsSnapshot>("agent_load_settings");
  }

  async saveSettings(request: {
    fingerprint?: string;
    activeModel: string;
    models: ModelSetting[];
    compression: CompressionSetting;
  }): Promise<SettingsSnapshot> {
    return invoke<SettingsSnapshot>("agent_save_settings", { request });
  }

  async loadUsage(): Promise<UsageSnapshot> {
    return invoke<UsageSnapshot>("agent_usage");
  }

  async setPermissionMode(mode: string): Promise<string> {
    return invoke<string>("agent_set_permission_mode", { request: { mode } });
  }

  async listPreferences(): Promise<UiPreference[]> {
    return invoke<UiPreference[]>("list_preferences");
  }

  async savePreference(request: {
    key: string;
    kind: string;
    value: unknown;
    expectedVersion?: number;
  }): Promise<UiPreference> {
    return invoke<UiPreference>("save_preference", { request });
  }

  async approvePlan(planId: string): Promise<void> {
    await invoke<void>("agent_send_message", {
      request: { message: `/plan-approve ${planId}`, sessionId: undefined },
    });
  }

  async loadPlan(planId: string): Promise<PlanSnapshot> {
    return invoke<PlanSnapshot>("agent_load_plan", { planId });
  }

  async loadPlans(): Promise<PlanSnapshot[]> {
    const response = await invoke<string>("agent_send_message", {
      request: { message: "/plan-list", sessionId: undefined },
    });
    return []; // Parse from response
  }

  subscribe(sessionId: string, onEvent: (event: TraceStep) => void): () => void {
    let unlistenFn: (() => void) | undefined;
    let unlisten: Promise<() => void> | undefined;
    // Also load any existing historical events
    void invoke<TraceStep[]>("agent_session_events", { sessionId }).then((events) => {
      events.forEach(onEvent);
    });
    // Listen for real-time streaming events pushed from Rust via Tauri
    unlisten = listen<TraceStep>("agent-event", (payload) => {
      onEvent(payload.payload);
    });
    return () => {
      unlisten?.then((fn) => fn());
    };
  }

  async openFile(path: string, line?: number): Promise<void> {
    return invoke<void>("agent_open_file", { path, line });
  }

  async addReference(request: {
    sessionId: string;
    referenceType: string;
    path?: string;
    startLine?: number;
    endLine?: number;
    content?: string;
    messageId?: string;
    snapshot?: string;
  }): Promise<ContextReference> {
    const result = await invoke<{ id: string; referenceType: string; locator: unknown; snapshot?: string; createdAt: string }>("agent_add_reference", {
      request: {
        sessionId: request.sessionId,
        referenceType: request.referenceType,
        path: request.path,
        startLine: request.startLine,
        endLine: request.endLine,
        content: request.content,
        messageId: request.messageId,
        snapshot: request.snapshot,
      },
    });
    const loc = result.locator as Record<string, unknown>;
    return {
      id: result.id,
      referenceType: result.referenceType as "FILE" | "SELECTION" | "MESSAGE",
      locator: {
        path: (loc?.File as Record<string, unknown>)?.["path"] as string | undefined,
        startLine: (loc?.File as Record<string, unknown>)?.["startLine"] as number | undefined,
        endLine: (loc?.File as Record<string, unknown>)?.["endLine"] as number | undefined,
        content: (loc?.Selection as Record<string, unknown>)?.["content"] as string | undefined,
      },
      snapshot: result.snapshot,
      createdAt: result.createdAt,
    };
  }

  async subscribeApprovals(onRequest: (request: ApprovalRequest) => void): Promise<() => void> {
    return listen<ApprovalRequest>("agent-approval-required", (event) => onRequest(event.payload));
  }

  decideApproval(approvalId: string, decision: "ALLOW_ONCE" | "DENY"): Promise<boolean> {
    return invoke<boolean>("agent_decide_approval", {
      request: { approvalId, decision },
    });
  }
}

export class HttpDesktopApi implements DesktopApi {
  constructor(private readonly baseUrl = "http://127.0.0.1:8080") {}

  async loadWorkspace(sessionId?: string): Promise<WorkspaceSnapshot> {
    const tracePath = sessionId ? `/api/trace/${encodeURIComponent(sessionId)}` : "/api/trace/current";
    const results = await Promise.allSettled([
      this.get<{ name?: string; nodes?: ProjectNode[] }>("/api/project/tree"),
      this.get<ChangeItem[]>("/api/project/changes"),
      this.get<TraceStep[]>(tracePath),
      this.get<MemoryItem[]>("/api/memory/list"),
      this.get<ToolStatus[]>("/api/tool/status"),
      this.get<SessionItem[]>("/api/session/list"),
      this.get<{ profile?: string; model?: string }>("/api/status"),
    ]);
    const value = <T>(index: number, fallback: T): T => {
      const result = results[index];
      return result.status === "fulfilled" ? (result.value as T) : fallback;
    };
    const project = value(0, { name: "Workspace", nodes: [] as ProjectNode[] });
    const status = value(6, { profile: "Coder", model: "Unavailable" });
    const rejected = results.filter((result) => result.status === "rejected");
    if (rejected.length === results.length) {
      throw new Error("Agent API is offline. Start the local core-agent service to load the workspace.");
    }
    return {
      projectName: project.name ?? "Workspace",
      workspacePath: "",
      profile: status.profile ?? "Coder",
      model: status.model ?? "Unavailable",
      projectTree: project.nodes ?? [],
      commands: [],
      changes: value(1, []),
      trace: value(2, []),
      memory: value(3, []),
      tools: value(4, []),
      sessions: value(5, []),
      resumeSession: false,
      permissionMode: "risk-based",
      configSources: [],
      effectiveConfig: {},
      contextUsage: undefined,
    };
  }

  async openWorkspace(_path: string): Promise<void> {
    throw new Error("Changing workspace is only available in the embedded desktop runtime.");
  }

  async openFile(path: string, line?: number): Promise<void> {
    throw new Error("Opening files is only available in the embedded desktop runtime.");
  }

  async addReference(_request: {
    sessionId: string;
    referenceType: string;
    path?: string;
    startLine?: number;
    endLine?: number;
    content?: string;
    messageId?: string;
    snapshot?: string;
  }): Promise<ContextReference> {
    throw new Error("Adding references is only available in the embedded desktop runtime.");
  }

  async sendMessage(message: string, sessionId?: string): Promise<AgentSubmission> {
    validateMessage(message);
    const submission = await this.request<{ sessionId?: string; response?: string; action?: AgentSubmission["action"] }>("/api/chat", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sessionId, message }),
    });
    return { ...submission, action: submission.action ?? "none" };
  }

  async searchContext(query: string, limit = 100): Promise<ContextCandidateSearch> {
    try {
      return await this.get<ContextCandidateSearch>(
        `/api/context/candidates?query=${encodeURIComponent(query)}&limit=${Math.min(limit, 500)}`,
      );
    } catch {
      return {
        indexedFiles: 0,
        indexedDirectories: 0,
        source: "remote",
        minimumQueryChars: 3,
        queryReady: [...query].length >= 3,
        matches: [],
      };
    }
  }

  subscribe(sessionId: string, onEvent: (event: TraceStep) => void): () => void {
    const source = new EventSource(`${this.baseUrl}/api/session/${encodeURIComponent(sessionId)}/events`);
    source.onmessage = (message) => {
      try {
        const event = JSON.parse(message.data) as TraceStep;
        if (event.id && event.kind && event.title && event.state) onEvent(event);
      } catch {
        // Malformed observation events are isolated from the workspace state.
      }
    };
    return () => source.close();
  }

  async loadPlan(planId: string): Promise<PlanSnapshot> {
    return this.get<PlanSnapshot>(`/api/plan/${encodeURIComponent(planId)}`);
  }

  async loadPlans(): Promise<PlanSnapshot[]> {
    try {
      return await this.get<PlanSnapshot[]>("/api/plan/list");
    } catch {
      return [];
    }
  }

  async approvePlan(planId: string): Promise<void> {
    await this.request(`/api/plan/${encodeURIComponent(planId)}/approve`, { method: "POST" });
  }

  private get<T>(path: string): Promise<T> {
    return this.request(path);
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, init);
    if (!response.ok) throw new Error(`Agent API returned ${response.status}`);
    const declared = Number(response.headers.get("content-length") ?? 0);
    if (declared > MAX_RESPONSE_BYTES) throw new Error("Agent API response exceeds 2 MiB");
    const text = await response.text();
    if (text.length > MAX_RESPONSE_BYTES) throw new Error("Agent API response exceeds 2 MiB");
    return JSON.parse(text) as T;
  }
}

function validateMessage(message: string): void {
  if (!message.trim() || message.length > 64 * 1024 || message.includes("\0")) {
    throw new Error("Message must contain at most 64 KiB of text.");
  }
}
