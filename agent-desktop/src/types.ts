export interface ProjectNode {
  id: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  children?: ProjectNode[];
}

export interface ChangeItem {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

export interface TraceStep {
  id: string;
  kind: string;
  title: string;
  state: string;
  durationMs?: number;
  tokens?: number;
  summary?: string;
}

export interface MemoryItem {
  id: string;
  kind: string;
  title: string;
  summary: string;
  pinned: boolean;
}

export interface ToolStatus {
  key: string;
  name: string;
  state: string;
}

export interface SessionItem {
  sessionId: string;
  title: string;
  state: string;
  updatedAt: string;
}

export interface ConversationItem {
  id: string;
  role: "user" | "agent" | "system";
  content: string;
}

export interface CommandSuggestion {
  name: string;
  usage: string;
  summary: string;
}

export interface ContextReference {
  id: string;
  referenceType: "FILE" | "SELECTION" | "MESSAGE";
  locator: {
    path?: string;
    startLine?: number;
    endLine?: number;
    content?: string;
    sessionId?: string;
    messageId?: string;
  };
  snapshot?: string;
  createdAt: string;
}

export interface ContextCandidateSearch {
  indexedFiles: number;
  indexedDirectories: number;
  source: string;
  minimumQueryChars: number;
  queryReady: boolean;
  matches: string[];
}

export interface WorkspaceSnapshot {
  projectName: string;
  workspacePath: string;
  profile: string;
  model: string;
  projectTree: ProjectNode[];
  commands: CommandSuggestion[];
  changes: ChangeItem[];
  trace: TraceStep[];
  memory: MemoryItem[];
  tools: ToolStatus[];
  sessions: SessionItem[];
  resumeSession: boolean;
  permissionMode: string;
  configSources: Array<{ provider: string; priority: number; location?: string }>;
  effectiveConfig: unknown;
  contextUsage?: ContextUsage;
}

export interface ContextUsage {
  contextId: string;
  totalTokens: number;
  maxTokens: number;
  buildDurationMs: number;
  estimated: boolean;
  distribution: Record<string, number>;
}

export interface AgentSubmission {
  sessionId?: string;
  response?: string;
  action: "none" | "new-session" | "clear-view" | "exit";
  requestId?: string;
  wallDurationMs?: number;
  activeDurationMs?: number;
  telemetryRecorded?: boolean;
}

export interface ApprovalRequest {
  id: string;
  sessionId: string;
  tool: string;
  risk: string;
  reason: string;
  parameters: unknown;
}

export interface UiPreference {
  id: string;
  key: string;
  kind: string;
  value: unknown;
  version: number;
  updatedAt?: string;
}

export interface ModelSetting {
  provider: string;
  baseURL: string;
  name: string;
  profile: string;
  maxContextTokens: number;
  apiKeyConfigured: boolean;
  apiKeyRef?: string;
  apiKey?: string;
}

export interface CompressionSetting {
  strategy: "recent-window" | "extractive-summary";
  triggerPercent: number;
  keepRecentMessages: number;
}

export interface SettingsSnapshot {
  path: string;
  fingerprint?: string;
  activeModel: string;
  models: ModelSetting[];
  compression: CompressionSetting;
  sources: Array<{ provider: string; priority: number; location?: string }>;
}

export interface UsageBucket {
  day: string;
  modelName: string;
  promptTokens: number;
  completionTokens: number;
  cacheTokens: number;
  totalTokens: number;
  modelCalls: number;
}

export interface RequestMetric {
  id: string;
  workspaceKey: string;
  sessionId?: string;
  entrypoint: string;
  modelName: string;
  startedAt: string;
  completedAt?: string;
  wallDurationMs: number;
  activeDurationMs: number;
  approvalWaitMs: number;
  contextDurationMs: number;
  modelDurationMs: number;
  toolDurationMs: number;
  contextTokens: number;
  status: string;
  errorKind?: string;
}

export interface UsageSnapshot {
  buckets: UsageBucket[];
  requests: RequestMetric[];
}
