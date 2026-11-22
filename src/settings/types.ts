// Provider / model-gateway shapes. The DTOs are generated from
// `phi-contracts/src/provider.rs` by `cargo run -p phi-node --bin export-types`.
// The settings page is the only writer; the node validates every value before
// it is persisted.

import type {
  ModelSpec,
  Protocol as GeneratedProtocol,
  ProviderInput as GeneratedProviderInput,
  ProviderTestView,
  ProviderView,
} from "../generated";

export type { ModelSpec, ProviderTestView, ProviderView };

/** Provenance for the model editor. `ModelSpec.source` is a string on the wire. */
export type ModelSource = "catalog" | "fetched" | "manual";

/** Mirrors the Rust `Protocol` wire values. */
export type Protocol = GeneratedProtocol;

/**
 * Patch-style input. Omitted keys keep their current value, matching the
 * `#[serde(default)]` fields on the Rust `ProviderInput`. `null` is not a
 * meaningful patch value for these fields: omitted means "keep", `""` clears
 * the secret.
 */
export type ProviderInput = Partial<{
  [K in keyof GeneratedProviderInput]: Exclude<GeneratedProviderInput[K], null>;
}>;

export const PROTOCOLS: Protocol[] = ["openai_chat", "openai_responses", "anthropic_messages"];

/** Provider APIs are inconsistent about id casing, so identity is normalized. */
export function normalizeModelId(id: string): string {
  return id.trim().toLowerCase();
}

export function modelFromId(id: string): ModelSpec {
  return {
    id,
    name: id,
    description: "",
    tags: [],
    contextWindow: null,
    maxOutputTokens: null,
    temperature: null,
    scale: 3,
    enabled: true,
    source: "manual",
  };
}

export function cloneModel(model: ModelSpec): ModelSpec {
  return {
    ...model,
    tags: [...model.tags],
  };
}

/// Fill only empty fields from a catalogue suggestion. Explicit edits win.
export function mergeSuggestion(current: ModelSpec, suggested: ModelSpec): ModelSpec {
  const suggestedName =
    suggested.name && suggested.name !== suggested.id ? suggested.name : current.name;
  return {
    ...current,
    name: current.name && current.name !== current.id ? current.name : suggestedName,
    description: current.description || suggested.description,
    tags: current.tags.length > 0 ? current.tags : [...suggested.tags],
    contextWindow: current.contextWindow ?? suggested.contextWindow ?? null,
    maxOutputTokens: current.maxOutputTokens ?? suggested.maxOutputTokens ?? null,
    temperature: current.temperature ?? suggested.temperature ?? null,
    scale: current.scale !== 3 ? current.scale : (suggested.scale ?? 3),
    source:
      suggested.source === "catalog" &&
      (!current.description || current.tags.length === 0 || current.source === "manual")
        ? "catalog"
        : current.source,
  };
}
