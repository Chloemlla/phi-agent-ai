// Shapes mirrored from the Rust read model (`phi-node` / `phi-domain`).

import type { WorkbenchView } from "./workbench/types";
import type { ProviderView } from "./settings/types";

export type ModelStatus = {
  provider: string;
  model: string;
  configured: boolean;
  providerId?: string;
  protocol?: string;
  baseUrl?: string;
  hasKey?: boolean;
  contextWindow?: number;
};

export type SessionSummary = {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
  lastRunState: string | null;
};

export type Block =
  | { type: "text"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | {
      type: "tool_result";
      tool_use_id: string;
      content: string;
      is_error: boolean;
    };

export type Message = {
  id: string;
  turnId: string | null;
  role: "user" | "assistant";
  ts: string;
  blocks: Block[];
};

export type Run = {
  id: string;
  taskId: string;
  turnId: string;
  state: string;
  steps: number;
  costUsd: number;
  workbenchMode: boolean;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  pendingApprovalId: string | null;
  error: string | null;
  stopConfirmation: string | null;
  startedAt: string | null;
  updatedAt: string;
};

export type SessionView = {
  id: string;
  ownerNodeId: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: Message[];
  runs: Run[];
  uiView: WorkbenchView | null;
};

export type PendingApproval = {
  id: string;
  sessionId: string;
  runId: string;
  invocationId: string;
  toolId: string;
  argsPreview: unknown;
  actionDigest: string;
  risk: string;
  requestedAt: string;
  expiresAt: string;
};

export type SessionDetail = {
  session: SessionView;
  pendingApprovals: PendingApproval[];
};

export type SendResult = { runId: string };

export type ModelStreamEvent = {
  runId: string;
  callId: string;
  kind: string;
  text?: string;
};

export type Bootstrap = {
  nodeId: string;
  mode: string;
  modeReason: string;
  model: ModelStatus;
  workspace: string | null;
  sessions: SessionSummary[];
  lastSessionId: string | null;
  providers: ProviderView[];
  activeProviderId: string | null;
  language: string;
};

export type MemoryScope = "user" | "project" | "agent" | "run";
export type MemoryTrust = "user" | "system" | "tool" | "model" | "external";
export type MemoryState = "active" | "invalidated" | "superseded";

export type MemoryRecord = {
  id: string;
  fact: string;
  scope: MemoryScope;
  userId: string;
  projectId: string | null;
  agentId: string | null;
  runId: string | null;
  sourceTrust: MemoryTrust;
  confidence: number;
  validFrom: string | null;
  validTo: string | null;
  recordedAt: string;
  invalidatedAt: string | null;
  supersededBy: string | null;
  reason: string | null;
  state: MemoryState;
  episodeId: string | null;
  redacted: boolean;
};

export type MemoryHit = {
  record: MemoryRecord;
  score: number;
  matchedTerms: string[];
};

export type MemoryStats = {
  total: number;
  active: number;
  invalidated: number;
  superseded: number;
  userScope: number;
  projectScope: number;
  agentScope: number;
  runScope: number;
  episodes: number;
  redacted: number;
};

export type MemoryAddOutcome =
  | { status: "admitted"; recordId: string; redacted: boolean }
  | { status: "duplicate"; recordId: string }
  | { status: "rejected"; candidateId: string; reason: string; message: string };

export type ForgetReport = { records: number; episodes: number };
