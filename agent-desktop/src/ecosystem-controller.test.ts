import { describe, expect, it, vi } from "vitest";
import type { EcosystemApi } from "./ecosystem-api";
import { createEcosystemController } from "./ecosystem-controller";

const snapshot = { packages: [{ id: "package-1", key: "rca-agent", name: "RCA Agent", packageVersion: "1.0.0", kind: "AGENT" as const, description: "RCA", publisher: "Acme", state: "LISTED", requiredCapabilities: ["metrics.query"], downloads: 2, rating: 5 }], publishers: [], installs: [], reviews: [], sdks: [], updates: [] };
describe("ecosystem controller", () => {
  it("loads marketplace and refreshes after install", async () => { const api: EcosystemApi = { load: vi.fn(async () => snapshot), install: vi.fn(async () => undefined), submit: vi.fn(async () => undefined) }; const controller = createEcosystemController(api); await controller.load(); await controller.install("package-1"); expect(api.install).toHaveBeenCalledWith("package-1"); expect(api.load).toHaveBeenCalledTimes(2); });
  it("surfaces a rejected publication", async () => { const api: EcosystemApi = { load: vi.fn(async () => snapshot), install: vi.fn(async () => undefined), submit: vi.fn(async () => { throw new Error("denied"); }) }; const controller = createEcosystemController(api); await expect(controller.submit("package-1")).rejects.toThrow("denied"); expect(controller.state.error).toBe("denied"); });
});
