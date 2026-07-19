export type CollaborationSection =
  | "home" | "projects" | "agents" | "team" | "tasks" | "reviews" | "approvals" | "knowledge" | "activity" | "notifications";

export interface CollaborationProject { id: string; name: string; state: string; members: number; agents: number; tasks: number; knowledge: string; }
export interface CollaborationMember { id: string; name: string; role: string; state: string; }
export interface CollaborationTask { id: string; number: number; title: string; state: string; assignee: string; ownerAgent?: string; reviewer?: string; progress: number; }
export interface CollaborationReview { id: string; taskId: string; taskTitle: string; state: string; risk: string; summary: string; reviewer?: string; createdBy: string; }
export interface CollaborationKnowledge { id: string; title: string; kind: string; state: string; version: number; summary: string; }
export interface CollaborationActivity { id: string; kind: string; subject: string; summary: string; entityType: string; entityId: string; occurredAt: string; unread?: boolean; }
export interface CollaborationAgent { id: string; name: string; owner: string; version: string; model: string; state: string; }
export interface CollaborationSnapshot {
  projects: CollaborationProject[]; agents: CollaborationAgent[]; members: CollaborationMember[]; tasks: CollaborationTask[];
  reviews: CollaborationReview[]; approvals: CollaborationReview[]; knowledge: CollaborationKnowledge[];
  activity: CollaborationActivity[]; notifications: CollaborationActivity[];
}
