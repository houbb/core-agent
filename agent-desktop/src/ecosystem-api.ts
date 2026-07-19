import type { EcosystemSnapshot } from "./ecosystem-types";
import { runtimeRequest } from "./runtime-request";

export interface EcosystemApi { load(): Promise<EcosystemSnapshot>; install(packageId: string): Promise<void>; submit(packageId: string): Promise<void>; }

export class HttpEcosystemApi implements EcosystemApi {
  constructor(private readonly baseUrl?: string) {}
  async load(): Promise<EcosystemSnapshot> {
    const names = ["packages", "publishers", "installs", "reviews", "sdks", "updates"];
    const results = await Promise.allSettled(names.map((name) => this.request<unknown>(`/api/ecosystem/${name}`)));
    if (results.every((result) => result.status === "rejected")) throw new Error("Ecosystem API is offline.");
    const pick = <T>(index: number): T[] => { const result = results[index]; if (result.status !== "fulfilled") return []; const value = result.value as { items?: T[] } | T[]; return Array.isArray(value) ? value : value.items ?? []; };
    return { packages: pick(0), publishers: pick(1), installs: pick(2), reviews: pick(3), sdks: pick(4), updates: pick(5) };
  }
  async install(packageId: string): Promise<void> { this.validateId(packageId); await this.request(`/api/ecosystem/packages/${encodeURIComponent(packageId)}/install`, { method: "POST" }); }
  async submit(packageId: string): Promise<void> { this.validateId(packageId); await this.request(`/api/ecosystem/packages/${encodeURIComponent(packageId)}/submit`, { method: "POST" }); }
  private validateId(value: string) { if (!/^[A-Za-z0-9-]{1,128}$/.test(value)) throw new Error("Package identifier is invalid."); }
  private async request<T>(path: string, init?: RequestInit): Promise<T> { return runtimeRequest<T>(path, init, this.baseUrl); }
}
