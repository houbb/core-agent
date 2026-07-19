import type { CollaborationSnapshot } from "./collaboration-types";
import { runtimeRequest } from "./runtime-request";

export interface CollaborationApi {
  load(projectId?: string): Promise<CollaborationSnapshot>;
  decideReview(reviewId: string, decision: "APPROVE" | "REJECT", comment: string): Promise<void>;
}

export class HttpCollaborationApi implements CollaborationApi {
  constructor(private readonly baseUrl?: string) {}

  async load(projectId?: string): Promise<CollaborationSnapshot> {
    const query = projectId ? `?projectId=${encodeURIComponent(projectId)}` : "";
    const names = ["projects", "agents", "members", "tasks", "reviews", "approvals", "knowledge", "activity", "notifications"];
    const results = await Promise.allSettled(names.map((name) => this.request<unknown>(`/api/collaboration/${name}${query}`)));
    if (results.every((result) => result.status === "rejected")) throw new Error("Collaboration API is offline.");
    const pick = <T>(index: number): T[] => {
      const result = results[index];
      if (result.status !== "fulfilled") return [];
      const value = result.value as { items?: T[] } | T[];
      return Array.isArray(value) ? value : value.items ?? [];
    };
    return {
      projects: pick(0), agents: pick(1), members: pick(2), tasks: pick(3), reviews: pick(4), approvals: pick(5),
      knowledge: pick(6), activity: pick(7), notifications: pick(8),
    };
  }

  async decideReview(reviewId: string, decision: "APPROVE" | "REJECT", comment: string): Promise<void> {
    if (!/^[A-Za-z0-9-]{1,128}$/.test(reviewId) || !comment.trim() || comment.length > 2048) throw new Error("Approval input is invalid.");
    await this.request(`/api/collaboration/reviews/${encodeURIComponent(reviewId)}/decision`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ decision, comment }),
    });
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    return runtimeRequest<T>(path, init, this.baseUrl);
  }
}
