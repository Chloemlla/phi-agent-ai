// Typed wrappers around the Tauri command/event boundary.
//
// The Rust side is the source of truth. DTOs in `src/types.ts` and
// `src/settings/types.ts` re-export the generated bindings; regenerate them
// with `cargo run -p phi-node --bin export-types` after changing a Rust DTO.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Bootstrap,
  ForgetReport,
  MemoryAddOutcome,
  MemoryHit,
  MemoryRecord,
  MemoryStats,
  ModelStreamEvent,
  SendResult,
  SessionDetail,
  SessionSummary,
  ThinkingLevel,
} from "./types";
import type { ModelSpec, ProviderInput, ProviderTestView, ProviderView } from "./settings/types";

// ─────────────────────────── bootstrap / workspace ──────────────────────────

export function getBootstrap(): Promise<Bootstrap> {
  return invoke<Bootstrap>("get_bootstrap");
}

export function setLanguage(language: string): Promise<string> {
  return invoke<string>("set_language", { language });
}

export function setWorkspace(path: string): Promise<string> {
  return invoke<string>("set_workspace", { path });
}

// ─────────────────────────── sessions / runs ────────────────────────────────

export function createSession(title: string | null = null): Promise<SessionSummary> {
  return invoke<SessionSummary>("create_session", { title });
}

export function getSession(sessionId: string): Promise<SessionDetail> {
  return invoke<SessionDetail>("get_session", { sessionId });
}

export type SendMessageInput = {
  sessionId: string;
  text: string;
  model?: string;
  thinking?: ThinkingLevel;
};

export function sendMessage(input: SendMessageInput): Promise<SendResult> {
  return invoke<SendResult>("send_message", input);
}

export function approve(
  approvalId: string,
  approved: boolean,
  note: string | null = null,
): Promise<void> {
  return invoke<void>("approve", { approvalId, approved, note });
}

export function cancelRun(runId: string): Promise<boolean> {
  return invoke<boolean>("cancel_run", { runId });
}

// ─────────────────────────── providers (settings) ───────────────────────────

export function listProviders(): Promise<ProviderView[]> {
  return invoke<ProviderView[]>("list_providers");
}

export function saveProvider(input: ProviderInput): Promise<ProviderView> {
  return invoke<ProviderView>("save_provider", { input });
}

export function removeProvider(providerId: string): Promise<boolean> {
  return invoke<boolean>("remove_provider", { providerId });
}

export function setActiveProvider(providerId: string): Promise<void> {
  return invoke<void>("set_active_provider", { providerId });
}

export function testProvider(providerId: string): Promise<ProviderTestView> {
  return invoke<ProviderTestView>("test_provider", { providerId });
}

/** Live model ids from a saved provider, without persisting anything. */
export function previewModels(providerId: string): Promise<string[]> {
  return invoke<string[]>("preview_models", { providerId });
}

/** Catalogue suggestions for the given model ids. */
export function describeModels(modelIds: string[]): Promise<ModelSpec[]> {
  return invoke<ModelSpec[]>("describe_models", { modelIds });
}

// ─────────────────────────── memory (settings) ──────────────────────────────

export type AddMemoryScope = "user" | "project";

export function listMemories(text: string | null = null): Promise<MemoryRecord[]> {
  return invoke<MemoryRecord[]>("list_memories", { text });
}

export function searchMemories(query: string, limit?: number): Promise<MemoryHit[]> {
  return invoke<MemoryHit[]>("search_memories", { query, limit });
}

export function getMemoryStats(): Promise<MemoryStats> {
  return invoke<MemoryStats>("get_memory_stats");
}

export function addMemory(fact: string, scope: AddMemoryScope): Promise<MemoryAddOutcome> {
  return invoke<MemoryAddOutcome>("add_memory", { fact, scope });
}

export function forgetMemories(ids: string[]): Promise<ForgetReport> {
  return invoke<ForgetReport>("forget_memories", { ids });
}

// ─────────────────────────── events ─────────────────────────────────────────

export const PHI_EVENT = "phi://event";
export const MODEL_STREAM_EVENT = "phi://model-stream";

/** The `payload` of a durable `EventRecord`, narrowed to the fields the UI reads. */
export type PhiEventPayload = {
  type?: string;
  runId?: string;
  callId?: string;
};

/** A durable event notification; the shell reacts by pulling a read model. */
export function onPhiEvent(
  handler: (payload: PhiEventPayload | undefined) => void,
): Promise<UnlistenFn> {
  return listen<{ payload?: PhiEventPayload }>(PHI_EVENT, (event) => {
    handler(event.payload?.payload);
  });
}

/** A transient model delta for live rendering; never journaled. */
export function onModelStream(handler: (event: ModelStreamEvent) => void): Promise<UnlistenFn> {
  return listen<ModelStreamEvent>(MODEL_STREAM_EVENT, (event) => {
    handler(event.payload);
  });
}
