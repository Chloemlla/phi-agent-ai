// Types crossing the Tauri boundary. The DTOs are generated from the Rust
// definitions by `cargo run -p phi-node --bin export-types`; only UI-only
// helpers and narrower chat-facing views live in this file.

import type {
  ContentBlock,
  MessageView,
  ModelSpec as GeneratedModelSpec,
  PendingApprovalView as GeneratedPendingApprovalView,
  ProviderView as GeneratedProviderView,
  RunView,
  SourceTrust,
  ThinkingEffort,
} from "./generated";

export type {
  Bootstrap,
  ContextUsageView,
  ForgetReport,
  MemoryAddOutcome,
  MemoryHit,
  MemoryRecord,
  MemoryScope,
  MemoryState,
  MemoryStats,
  ModelStatus,
  ModelStreamEvent,
  SendResult,
  SessionDetail,
  SessionSummary,
  SessionView,
} from "./generated";

/** Host platform reported by the node. */
export type Platform = "windows" | "macos" | "linux" | "android" | "ios";

/** Mirrors the Rust `ThinkingEffort` wire values. */
export type ThinkingLevel = ThinkingEffort;

export const THINKING_LEVELS: ThinkingLevel[] = ["off", "low", "medium", "high", "xhigh", "max"];

export function isThinkingLevel(value: unknown): value is ThinkingLevel {
  return (
    value === "off" ||
    value === "low" ||
    value === "medium" ||
    value === "high" ||
    value === "xhigh" ||
    value === "max"
  );
}

/** The model the composer will call and how hard it should think. */
export type ModelSelection = {
  providerId: string;
  modelId: string;
  thinking: ThinkingLevel;
};

/**
 * The subset of a provider's model spec the chat shell needs. The generated
 * `ModelSpec` is assignable to this; the composer may also synthesize a row
 * from an id alone.
 */
export type ModelSpec = Pick<GeneratedModelSpec, "id" | "name" | "enabled"> & {
  tags?: string[];
};

/**
 * The subset of a provider view exposed to the chat shell. The generated
 * `ProviderView` is assignable to this.
 */
export type ProviderView = Pick<GeneratedProviderView, "id" | "name" | "model"> & {
  models: ModelSpec[];
  active?: boolean;
};

export type Block = ContentBlock;
export type Message = MessageView;
export type Run = RunView;
export type PendingApproval = GeneratedPendingApprovalView;
export type MemoryTrust = SourceTrust;
