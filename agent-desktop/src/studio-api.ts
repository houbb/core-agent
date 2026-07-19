import type { CreateAgentInput, RegisteredVisualPanel, StudioAsset, StudioSnapshot } from "./studio-types";
import { runtimeRequest } from "./runtime-request";

export interface StudioApi {
  load(): Promise<StudioSnapshot>;
  createAgent(input: CreateAgentInput): Promise<StudioAsset>;
}

export class HttpStudioApi implements StudioApi {
  constructor(private readonly baseUrl?: string) {}

  async load(): Promise<StudioSnapshot> {
    const paths = [
      "/api/agent",
      "/api/workflow",
      "/api/prompt",
      "/api/memory",
      "/api/capability",
      "/api/knowledge",
      "/api/trace",
      "/api/model",
      "/api/visual/catalog",
    ];
    const results = await Promise.allSettled(paths.map((path) => this.request<unknown>(path)));
    const pick = <T>(index: number, fallback: T): T => {
      const result = results[index];
      if (result.status !== "fulfilled") return fallback;
      const value = result.value as { items?: T; panels?: T } | T;
      if (Array.isArray(value)) return value as T;
      return (value as { items?: T; panels?: T }).items ?? (value as { panels?: T }).panels ?? fallback;
    };
    if (results.every((result) => result.status === "rejected")) {
      throw new Error("Studio API is offline. Start core-agent to manage Agent assets.");
    }
    return {
      agents: pick<StudioAsset[]>(0, []),
      workflows: pick<StudioAsset[]>(1, []),
      prompts: pick<StudioAsset[]>(2, []),
      memories: pick<StudioAsset[]>(3, []),
      capabilities: pick<StudioAsset[]>(4, []),
      knowledge: pick<StudioAsset[]>(5, []),
      traces: pick<StudioAsset[]>(6, []),
      models: pick<StudioAsset[]>(7, []),
      panels: pick<RegisteredVisualPanel[]>(8, []),
    };
  }

  async createAgent(input: CreateAgentInput): Promise<StudioAsset> {
    for (const value of [input.name, input.role, input.model, input.memory]) {
      if (!value.trim() || value.length > 256 || value.includes("\0")) {
        throw new Error("Agent fields must contain safe bounded text.");
      }
    }
    if (input.tools.length > 64 || input.tools.some((tool) => !/^[A-Za-z0-9._:/-]{1,128}$/.test(tool))) {
      throw new Error("Agent tool selection is invalid.");
    }
    return this.request<StudioAsset>("/api/agent", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    });
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    return runtimeRequest<T>(path, init, this.baseUrl);
  }
}
