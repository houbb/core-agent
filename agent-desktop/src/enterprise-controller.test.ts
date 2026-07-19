import { describe, expect, it, vi } from "vitest";
import type { EnterpriseApi } from "./enterprise-api";
import { createEnterpriseController } from "./enterprise-controller";

const snapshot = { organizations: [], principals: [], assets: [{ id: "asset-1", key: "agent", name: "Agent", assetType: "AGENT", assetVersion: "1.0.0", ownerSubject: "alice", classification: "INTERNAL", environment: "STAGING", state: "REVIEWED", riskScore: 72, approvals: 0, requiredApprovals: 1 }], policies: [], costs: [], audits: [], operations: [] };

describe("enterprise controller", () => {
  it("loads governance data and refreshes after an approval", async () => {
    const api: EnterpriseApi = { load: vi.fn(async () => snapshot), approveAsset: vi.fn(async () => undefined), transitionAsset: vi.fn(async () => undefined) };
    const controller = createEnterpriseController(api);
    await controller.load();
    expect(controller.state.snapshot.assets[0].riskScore).toBe(72);
    await controller.approve("asset-1");
    expect(api.approveAsset).toHaveBeenCalledWith("asset-1", expect.stringContaining("Enterprise"));
    expect(api.load).toHaveBeenCalledTimes(2);
  });

  it("keeps failed governance mutations visible", async () => {
    const api: EnterpriseApi = { load: vi.fn(async () => snapshot), approveAsset: vi.fn(async () => { throw new Error("denied"); }), transitionAsset: vi.fn(async () => undefined) };
    const controller = createEnterpriseController(api);
    await expect(controller.approve("asset-1")).rejects.toThrow("denied");
    expect(controller.state.error).toBe("denied");
  });
});
