export type EnterpriseSection =
  | "dashboard" | "organization" | "identity" | "assets" | "governance"
  | "policies" | "cost" | "audit" | "operation" | "settings";

export interface EnterpriseOrganization { id: string; name: string; key: string; state: string; members: number; assets: number; }
export interface EnterprisePrincipal { id: string; externalSubject: string; displayName: string; provider: string; roles: string[]; groups: string[]; state: string; }
export interface EnterpriseAsset { id: string; key: string; name: string; assetType: string; assetVersion: string; ownerSubject: string; classification: string; environment: string; state: string; riskScore: number; approvals: number; requiredApprovals: number; }
export interface EnterprisePolicy { id: string; name: string; key: string; state: string; rules: number; scope: string; }
export interface EnterpriseCost { id: string; eventKey: string; project?: string; agent?: string; model?: string; currency: string; amountMicros: string; inputTokens: number; outputTokens: number; occurredAt: string; }
export interface EnterpriseAudit { id: string; subject: string; action: string; resource: string; decision: string; reason: string; occurredAt: string; }
export interface EnterpriseOperation { component: string; state: string; message: string; checkedAt: string; }
export interface EnterpriseSnapshot { organizations: EnterpriseOrganization[]; principals: EnterprisePrincipal[]; assets: EnterpriseAsset[]; policies: EnterprisePolicy[]; costs: EnterpriseCost[]; audits: EnterpriseAudit[]; operations: EnterpriseOperation[]; }
