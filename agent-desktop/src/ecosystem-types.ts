export type EcosystemSection = "marketplace" | "my-agents" | "capabilities" | "templates" | "developer" | "publishing" | "community" | "cloud";
export interface EcosystemPackage { id: string; key: string; name: string; packageVersion: string; kind: "AGENT" | "CAPABILITY" | "TEMPLATE" | "SDK"; description: string; publisher: string; state: string; requiredCapabilities: string[]; downloads: number; rating?: number; }
export interface EcosystemPublisher { id: string; key: string; name: string; state: string; packages: number; }
export interface EcosystemInstall { packageId: string; state: string; installedVersion: string; updatedAt: string; }
export interface EcosystemReview { id: string; packageId: string; packageName: string; state: string; reviewer?: string; submittedAt: string; }
export interface EcosystemSdk { key: string; name: string; language: string; version: string; documentationPath: string; }
export interface EcosystemSnapshot { packages: EcosystemPackage[]; publishers: EcosystemPublisher[]; installs: EcosystemInstall[]; reviews: EcosystemReview[]; sdks: EcosystemSdk[]; updates: EcosystemPackage[]; }
