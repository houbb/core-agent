import type {
  ChangeItem,
  MemoryItem,
  ProjectNode,
  SessionItem,
  ToolStatus,
  TraceStep,
  WorkspaceSnapshot,
  ApprovalRequest,
} from "./types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

export interface DesktopApi {
  loadWorkspace(sessionId?: string): Promise<WorkspaceSnapshot>;
  sendMessage(message: string, sessionId?: string): Promise<{ sessionId: string }>;
  subscribe(sessionId: string, onEvent: (event: TraceStep) => void): () => void;
}

export class TauriDesktopApi implements DesktopApi {
  async loadWorkspace(sessionId?: string): Promise<WorkspaceSnapshot> {
    return invoke<WorkspaceSnapshot>("agent_load_workspace", { sessionId });
  }

  async sendMessage(message: string, sessionId?: string): Promise<{ sessionId: string }> {
    validateMessage(message);
    return invoke<{ sessionId: string }>("agent_send_message", {
      request: { message, sessionId },
    });
  }

  subscribe(sessionId: string, onEvent: (event: TraceStep) => void): () => void {
    let active = true;
    void invoke<TraceStep[]>("agent_session_events", { sessionId }).then((events) => {
      if (active) events.forEach(onEvent);
    });
    return () => {
      active = false;
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
      profile: status.profile ?? "Coder",
      model: status.model ?? "Unavailable",
      projectTree: project.nodes ?? [],
      changes: value(1, []),
      trace: value(2, []),
      memory: value(3, []),
      tools: value(4, []),
      sessions: value(5, []),
    };
  }

  async sendMessage(message: string, sessionId?: string): Promise<{ sessionId: string }> {
    validateMessage(message);
    return this.request("/api/chat", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sessionId, message }),
    });
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
