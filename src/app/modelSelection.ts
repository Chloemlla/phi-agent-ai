// Model selection persistence and the selection derived from the bootstrap.

import { createEffect, createSignal, type Accessor } from "solid-js";
import {
  isThinkingLevel,
  type Bootstrap,
  type ModelSelection,
  type ProviderView,
  type ThinkingLevel,
} from "../types";

const STORAGE_PREFIX = "phi.model.";

export function activeProviderOf(bootstrap: Bootstrap): ProviderView | null {
  return (
    bootstrap.providers.find((provider) => provider.id === bootstrap.activeProviderId) ??
    bootstrap.providers.find((provider) => provider.active) ??
    bootstrap.providers[0] ??
    null
  );
}

export function readStoredSelection(
  providerId: string,
): { modelId: string; thinking: ThinkingLevel } | null {
  try {
    const raw = window.localStorage.getItem(`${STORAGE_PREFIX}${providerId}`);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as {
      modelId?: unknown;
      thinking?: unknown;
    };
    if (typeof parsed.modelId !== "string" || !parsed.modelId) return null;
    return {
      modelId: parsed.modelId,
      thinking: isThinkingLevel(parsed.thinking) ? parsed.thinking : "off",
    };
  } catch {
    return null;
  }
}

export function writeStoredSelection(selection: ModelSelection) {
  try {
    window.localStorage.setItem(
      `${STORAGE_PREFIX}${selection.providerId}`,
      JSON.stringify({
        modelId: selection.modelId,
        thinking: selection.thinking,
      }),
    );
  } catch {
    // Persistence is best-effort; the selection still applies to this window.
  }
}

/**
 * Pick the selection that stays valid for `active`. Returning `current` means
 * "already valid", so setting it back is a no-op for Solid's equality check.
 */
export function deriveSelection(
  active: ProviderView | null,
  current: ModelSelection | null,
): ModelSelection | null {
  if (!active) return null;

  if (current && current.providerId === active.id) {
    const model = active.models.find(
      (candidate) => candidate.id === current.modelId && candidate.enabled,
    );
    if (model) return current;
  }

  const enabled = active.models.filter((candidate) => candidate.enabled);
  if (enabled.length === 0) return null;

  const primary = enabled.find((candidate) => candidate.id === active.model) ?? enabled[0];
  const stored = readStoredSelection(active.id);
  const storedModel = stored ? enabled.find((candidate) => candidate.id === stored.modelId) : null;

  const modelId = storedModel?.id ?? primary.id;
  const thinking: ThinkingLevel = stored && stored.modelId === modelId ? stored.thinking : "off";
  return { providerId: active.id, modelId, thinking };
}

/** Keep the composer's model choice valid for the active provider. */
export function useModelSelection(boot: Accessor<Bootstrap | null>) {
  const [selection, setSelection] = createSignal<ModelSelection | null>(null);

  createEffect(() => {
    const bootstrap = boot();
    if (!bootstrap) return;
    setSelection(deriveSelection(activeProviderOf(bootstrap), selection()));
  });

  function updateSelection(next: ModelSelection) {
    setSelection(next);
    writeStoredSelection(next);
  }

  return { selection, updateSelection };
}
