// Mirrors of the provider / model-gateway contract
// (`phi-contracts/src/provider.rs`). The settings page is the only writer;
// the node validates every value before it is persisted.

export type ModelSource = "catalog" | "fetched" | "manual";

export type ModelSpec = {
  id: string;
  name: string;
  description: string;
  tags: string[];
  contextWindow?: number | null;
  maxOutputTokens?: number | null;
  temperature?: number | null;
  /** Capability scale from 1 (small, fast) to 5 (frontier); 3 when unknown. */
  scale: number;
  enabled: boolean;
  /** `catalog`, `fetched` or `manual` — provenance for the editor. */
  source: ModelSource;
};

export type ProviderView = {
  id: string;
  name: string;
  protocol: string;
  baseUrl: string;
  model: string;
  models: ModelSpec[];
  modelMap: Record<string, string>;
  headers: Record<string, string>;
  enabled: boolean;
  active: boolean;
  hasKey: boolean;
  keyHint?: string;
  auth: string;
  maxOutputTokens?: number;
  temperature?: number;
  sortIndex: number;
  icon?: string;
  iconColor?: string;
  websiteUrl?: string;
  notes?: string;
  createdAt: string;
  updatedAt: string;
};

export type ProviderInput = {
  id?: string;
  name?: string;
  protocol?: string;
  baseUrl?: string;
  /** Omit to keep the current secret; `""` clears it. */
  apiKey?: string;
  model?: string;
  models?: ModelSpec[];
  modelMap?: Record<string, string>;
  auth?: string;
  headers?: Record<string, string>;
  maxOutputTokens?: number;
  temperature?: number;
  clearMaxOutputTokens?: boolean;
  clearTemperature?: boolean;
  enabled?: boolean;
  sortIndex?: number;
  icon?: string;
  iconColor?: string;
  websiteUrl?: string;
  notes?: string;
};

export type ProviderTestView = {
  ok: boolean;
  latencyMs: number;
  modelCount: number;
  error?: string;
};

export type Protocol = "anthropic_messages" | "openai_chat" | "openai_responses";

export const PROTOCOLS: Protocol[] = [
  "openai_chat",
  "openai_responses",
  "anthropic_messages",
];

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
