import type { EnterpriseSnapshot } from "./enterprise-types";
import { runtimeRequest } from "./runtime-request";

export interface EnterpriseApi {
  load(): Promise<EnterpriseSnapshot>;
  approveAsset(assetId: string, comment: string): Promise<void>;
  transitionAsset(assetId: string, target: "PRODUCTION" | "SUSPENDED" | "RETIRED"): Promise<void>;
}

export class HttpEnterpriseApi implements EnterpriseApi {
  constructor(private readonly baseUrl?: string) {}

  async load(): Promise<EnterpriseSnapshot> {
    const names = ["organizations", "identity", "assets", "policies", "costs", "audits", "operations"];
    const results = await Promise.allSettled(names.map((name) => this.request<unknown>(`/api/enterprise/${name}`)));
    if (results.every((result) => result.status === "rejected")) throw new Error("Enterprise API is offline.");
    const pick = <T>(index: number): T[] => {
      const result = results[index];
      if (result.status !== "fulfilled") return [];
      const value = result.value as { items?: T[] } | T[];
      return Array.isArray(value) ? value : value.items ?? [];
    };
    return { organizations: pick(0), principals: pick(1), assets: pick(2), policies: pick(3), costs: pick(4), audits: pick(5), operations: pick(6) };
  }

  async approveAsset(assetId: string, comment: string): Promise<void> {
    this.validateId(assetId);
    if (!comment.trim() || comment.length > 2048) throw new Error("Approval comment is invalid.");
    await this.request(`/api/enterprise/assets/${encodeURIComponent(assetId)}/approvals`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ comment }) });
  }

  async transitionAsset(assetId: string, target: "PRODUCTION" | "SUSPENDED" | "RETIRED"): Promise<void> {
    this.validateId(assetId);
    await this.request(`/api/enterprise/assets/${encodeURIComponent(assetId)}/transition`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ target }) });
  }

  private validateId(value: string) { if (!/^[A-Za-z0-9-]{1,128}$/.test(value)) throw new Error("Asset identifier is invalid."); }
  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    return runtimeRequest<T>(path, init, this.baseUrl);
  }
}
