import { describe, expect, it, vi } from "vitest";
import { createStudioController } from "./studio-controller";
import type { StudioApi } from "./studio-api";
import type { CreateAgentInput, StudioAsset, StudioSnapshot } from "./studio-types";

const empty = (): StudioSnapshot => ({
  agents: [], workflows: [], prompts: [], memories: [], capabilities: [], knowledge: [], traces: [], models: [], panels: [],
});

class FakeStudioApi implements StudioApi {
  load = vi.fn(async () => empty());
  createAgent = vi.fn(async (input: CreateAgentInput): Promise<StudioAsset> => ({
    id: "agent-1",
    name: input.name,
    version: "1.0.0",
    state: "DRAFT",
  }));
}

describe("Studio controller", () => {
  it("loads assets and inserts a newly created versioned Agent", async () => {
    const api = new FakeStudioApi();
    const controller = createStudioController(api);
    await controller.load();
    await controller.createAgent({
      name: "Coding Agent",
      role: "architect",
      model: "default",
      memory: "project",
      tools: ["git", "filesystem"],
    });
    expect(api.createAgent).toHaveBeenCalledOnce();
    expect(controller.state.snapshot.agents[0].name).toBe("Coding Agent");
    expect(controller.state.section).toBe("agents");
  });

  it("keeps the prior snapshot when Studio reload fails", async () => {
    const api = new FakeStudioApi();
    const controller = createStudioController(api);
    controller.state.snapshot.agents.push({ id: "a", name: "Existing", version: "1", state: "READY" });
    api.load.mockRejectedValueOnce(new Error("offline"));
    await controller.load();
    expect(controller.state.error).toBe("offline");
    expect(controller.state.snapshot.agents[0].name).toBe("Existing");
  });

  it("exposes every Studio section deterministically", () => {
    const controller = createStudioController(new FakeStudioApi());
    for (const section of ["home", "agents", "workflow", "prompt", "memory", "capability", "knowledge", "trace", "model"] as const) {
      controller.select(section);
      expect(controller.state.section).toBe(section);
    }
  });
});
