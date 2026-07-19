import { describe, expect, it, vi } from "vitest";
import { createCollaborationController } from "./collaboration-controller";
import type { CollaborationApi } from "./collaboration-api";
import type { CollaborationSnapshot } from "./collaboration-types";

const snapshot = (): CollaborationSnapshot => ({
  projects: [{ id: "project-1", name: "Monolith", state: "ACTIVE", members: 2, agents: 1, tasks: 1, knowledge: "READY" }],
  agents: [], members: [], tasks: [], reviews: [],
  approvals: [{ id: "review-1", taskId: "task-1", taskTitle: "Refactor login", state: "PENDING", risk: "MEDIUM", summary: "Review diff", reviewer: "bob", createdBy: "alice" }],
  knowledge: [], activity: [], notifications: [],
});

class FakeApi implements CollaborationApi {
  load = vi.fn(async () => snapshot());
  decideReview = vi.fn(async () => undefined);
}

describe("Collaboration controller", () => {
  it("selects the first project and refreshes after an approval", async () => {
    const api = new FakeApi();
    const controller = createCollaborationController(api);
    await controller.load();
    expect(controller.state.projectId).toBe("project-1");
    await controller.decide("review-1", "APPROVE");
    expect(api.decideReview).toHaveBeenCalledWith("review-1", "APPROVE", expect.stringContaining("Approved"));
    expect(api.load).toHaveBeenCalledTimes(2);
  });

  it("preserves the current snapshot when refresh fails", async () => {
    const api = new FakeApi();
    const controller = createCollaborationController(api);
    await controller.load();
    api.load.mockRejectedValueOnce(new Error("offline"));
    await controller.load();
    expect(controller.state.error).toBe("offline");
    expect(controller.state.snapshot.projects[0].name).toBe("Monolith");
  });

  it("supports all collaboration destinations", () => {
    const controller = createCollaborationController(new FakeApi());
    for (const section of ["home", "projects", "agents", "team", "tasks", "reviews", "approvals", "knowledge", "activity", "notifications"] as const) {
      controller.select(section);
      expect(controller.state.section).toBe(section);
    }
  });
});
