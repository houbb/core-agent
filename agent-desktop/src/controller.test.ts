import { describe, expect, it, vi } from "vitest";
import { createDesktopController } from "./controller";
import type { DesktopApi } from "./api";
import type { AgentSubmission, TraceStep, WorkspaceSnapshot } from "./types";

function snapshot(): WorkspaceSnapshot {
  return {
    projectName: "Core Agent",
    profile: "Architect",
    model: "Local",
    projectTree: [],
    commands: [{ name: "plan", usage: "/plan <request>", summary: "Create a plan" }],
    changes: [],
    trace: [],
    memory: [],
    tools: [],
    sessions: [{ sessionId: "session-1", title: "Task", state: "RUNNING", updatedAt: "now" }],
    resumeSession: true,
    permissionMode: "risk-based",
    configSources: [],
    effectiveConfig: {},
  };
}

class FakeApi implements DesktopApi {
  listener?: (event: TraceStep) => void;
  closed = false;
  sendMessage = vi.fn(async (): Promise<AgentSubmission> => ({
    sessionId: "session-1",
    action: "none",
  }));
  loadWorkspace = vi.fn(async () => snapshot());
  openWorkspace = vi.fn(async () => undefined);
  searchContext = vi.fn(async (query: string) => ({
    indexedFiles: 1,
    indexedDirectories: 0,
    source: "test",
    minimumQueryChars: 3,
    queryReady: query.length >= 3,
    matches: query.length >= 3 ? ["src/main.rs"] : [],
  }));
  subscribe(_sessionId: string, onEvent: (event: TraceStep) => void) {
    this.listener = onEvent;
    return () => {
      this.closed = true;
    };
  }
}

describe("desktop controller", () => {
  it("loads workspace, sends a message and applies streamed trace", async () => {
    const api = new FakeApi();
    const controller = createDesktopController(api);
    await controller.load();
    expect(controller.state.connected).toBe(true);
    expect(controller.state.currentSessionId).toBe("session-1");

    await controller.send("Review the current change");
    expect(api.sendMessage).toHaveBeenCalledOnce();
    api.listener?.({
      id: "response-1",
      kind: "response",
      title: "Agent response",
      state: "COMPLETED",
      summary: "Review complete",
    });
    expect(controller.state.snapshot.trace).toHaveLength(1);
    expect(controller.state.conversation.at(-1)?.content).toBe("Review complete");
    controller.dispose();
    expect(api.closed).toBe(true);
  });

  it("routes local slash commands through the backend without creating a model trace", async () => {
    const api = new FakeApi();
    api.sendMessage.mockResolvedValueOnce({
      action: "new-session",
      response: "Started a new chat session.",
    });
    const controller = createDesktopController(api);
    await controller.load();
    await controller.send("/new");
    expect(controller.state.currentSessionId).toBeUndefined();
    expect(controller.state.snapshot.trace).toEqual([]);
    expect(controller.state.conversation.at(-1)?.role).toBe("system");
  });

  it("isolates load failure as an offline workspace state", async () => {
    const api = new FakeApi();
    api.loadWorkspace.mockRejectedValueOnce(new Error("offline"));
    const controller = createDesktopController(api);
    await controller.load();
    expect(controller.state.connected).toBe(false);
    expect(controller.state.error).toBe("offline");
    expect(controller.state.snapshot.projectTree).toEqual([]);
  });

  it("switches the embedded runtime workspace and reloads a clean session", async () => {
    const api = new FakeApi();
    const controller = createDesktopController(api);
    controller.state.currentSessionId = "session-old";
    controller.state.conversation.push({ id: "message", role: "user", content: "old" });
    await controller.openWorkspace("D:/workspace/new");
    expect(api.openWorkspace).toHaveBeenCalledWith("D:/workspace/new");
    expect(controller.state.conversation).toEqual([]);
    expect(api.loadWorkspace).toHaveBeenLastCalledWith(undefined);
  });

  it("switches workspace kinds without mutating data", () => {
    const controller = createDesktopController(new FakeApi());
    for (const workspace of [
      "console",
      "project",
      "changes",
      "trace",
      "tools",
      "memory",
      "sessions",
      "studio",
      "collaboration",
      "enterprise",
      "ecosystem",
      "settings",
    ] as const) {
      controller.selectWorkspace(workspace);
      expect(controller.state.activeWorkspace).toBe(workspace);
    }
  });
});
