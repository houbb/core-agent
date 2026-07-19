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
}

export interface AgentSubmission {
  sessionId?: string;
  response?: string;
  action: "none" | "new-session" | "clear-view" | "exit";
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
}
