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
  maxSteps: number;
  costUsd: number;
  maxCostUsd: number;
  maxWallSecs: number;
  workbenchMode: boolean;
  inputTokens: number;
  outputTokens: number;
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
  maxSteps: number;
  maxCostUsd: number;
  maxWallSecs: number;
  providers: ProviderView[];
  activeProviderId: string | null;
  language: string;
};
