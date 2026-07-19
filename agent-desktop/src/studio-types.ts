export type StudioSection =
  | "home"
  | "agents"
  | "workflow"
  | "prompt"
  | "memory"
  | "capability"
  | "knowledge"
  | "trace"
  | "model";

export interface StudioAsset {
  id: string;
  name: string;
  version: string;
  state: string;
  description?: string;
  nodes?: Array<{ id: string; kind: string; label: string }>;
}

export interface VisualField {
  key: string;
  label: string;
  kind: string;
  sortable: boolean;
  filterable: boolean;
}

export interface VisualAction {
  key: string;
  label: string;
  method: "Get" | "Post" | "Patch" | "Delete";
  endpoint: string;
  dangerous: boolean;
  requires_approval: boolean;
}

export interface RegisteredVisualPanel {
  id: string;
  runtime_id: string;
  runtime_version: string;
  descriptor_revision: number;
  panel: {
    key: string;
    title: string;
    description: string;
    kind: string;
    data_source: { endpoint: string; refresh_seconds?: number };
    fields: VisualField[];
    actions: VisualAction[];
  };
}

export interface StudioSnapshot {
  agents: StudioAsset[];
  workflows: StudioAsset[];
  prompts: StudioAsset[];
  memories: StudioAsset[];
  capabilities: StudioAsset[];
  knowledge: StudioAsset[];
  traces: StudioAsset[];
  models: StudioAsset[];
  panels: RegisteredVisualPanel[];
}

export interface CreateAgentInput {
  name: string;
  role: string;
  model: string;
  memory: string;
  tools: string[];
}
