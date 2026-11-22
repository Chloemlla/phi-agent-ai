// Block state: parse the inert DOM and keep declared state values for the
// session, keyed by the block source.

import type { UiValues } from "../uiExpression";
import { BLOCKED_KEYS, MAX_CACHED_STATES } from "./allowlist";

/** States live for the session, keyed by the block source. The source changes
 * only when the model writes a different answer, so a reply keeps its choices
 * while the transcript refreshes. */
export const stateCache = new Map<string, UiValues>();

export function cacheState(key: string, values: UiValues) {
  if (!stateCache.has(key) && stateCache.size >= MAX_CACHED_STATES) {
    const oldest = stateCache.keys().next().value;
    if (oldest) stateCache.delete(oldest);
  }
  stateCache.set(key, values);
}

export function plain(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [name, child] of Object.entries(value as Record<string, unknown>)) {
      if (BLOCKED_KEYS.has(name)) continue;
      out[name] = plain(child);
    }
    return out;
  }
  if (typeof value === "function") return undefined;
  return value;
}

export function compatible(defaultValue: unknown, savedValue: unknown): boolean {
  if (defaultValue === null) return savedValue === null;
  if (Array.isArray(defaultValue)) return Array.isArray(savedValue);
  return typeof defaultValue === typeof savedValue;
}

export function mergeState(defaults: UiValues, saved: UiValues): UiValues {
  const merged: UiValues = {};
  for (const [name, value] of Object.entries(defaults)) {
    if (BLOCKED_KEYS.has(name)) continue;
    merged[name] =
      Object.prototype.hasOwnProperty.call(saved, name) && compatible(value, saved[name])
        ? saved[name]
        : value;
  }
  for (const [name, value] of Object.entries(saved)) {
    if (BLOCKED_KEYS.has(name) || name in merged) continue;
    merged[name] = value;
  }
  return merged;
}

function parseState(source: string): UiValues {
  try {
    const value = JSON.parse(source) as unknown;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const state: UiValues = {};
    for (const [name, child] of Object.entries(value as Record<string, unknown>)) {
      if (BLOCKED_KEYS.has(name)) continue;
      state[name] = child;
    }
    return state;
  } catch {
    return {};
  }
}

export function parseBlock(html: string): { body: Element; state: UiValues } {
  const document = new DOMParser().parseFromString(html, "text/html");
  const script = document.querySelector(
    'script[type="application/json"][data-ui-state]',
  );
  // A template with the same attribute is an equally inert alternative for
  // models that prefer not to write a script tag.
  const template = document.querySelector("template[data-ui-state]");
  const text = script?.textContent ?? template?.getAttribute("data-ui-state") ?? "";
  return {
    body: document.body,
    state: text.trim() ? parseState(text.slice(0, 20_000)) : {},
  };
}
