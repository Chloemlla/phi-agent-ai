// Shared helpers for the provider editor and its sections. Validation and
// formatting stay here so the form sections remain presentational.

import { t } from "../i18n";
import { formatTokens } from "../format";
import type { ProviderInput } from "./types";

export function entries(map: Record<string, string> | undefined): { key: string; value: string }[] {
  return Object.entries(map ?? {}).map(([key, value]) => ({ key, value }));
}

export function formatWindow(value: number | null | undefined): string {
  return value != null && value > 0 ? formatTokens(value) : "";
}

function urlMessage(kind: "base" | "website"): string {
  return kind === "base"
    ? t("Base URL must start with http:// or https://.")
    : t("Website must start with http:// or https://.");
}

function urlError(raw: string, kind: "base" | "website"): string {
  const value = raw.trim();
  if (!value) return "";
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? "" : urlMessage(kind);
  } catch {
    return urlMessage(kind);
  }
}

export type FieldErrors = {
  name?: string;
  baseUrl?: string;
  website?: string;
};

export function validateConnection(draft: ProviderInput): FieldErrors {
  const errors: FieldErrors = {};
  if (!(draft.name ?? "").trim()) errors.name = t("Name is required.");
  if (!(draft.baseUrl ?? "").trim()) {
    errors.baseUrl = t("Base URL is required.");
  } else {
    const baseError = urlError(draft.baseUrl ?? "", "base");
    if (baseError) errors.baseUrl = baseError;
  }
  const websiteError = urlError(draft.websiteUrl ?? "", "website");
  if (websiteError) errors.website = websiteError;
  return errors;
}

export function protocolLabel(protocol: string): string {
  switch (protocol) {
    case "anthropic_messages":
      return t("Anthropic Messages");
    case "openai_chat":
      return t("OpenAI Chat Completions");
    case "openai_responses":
      return t("OpenAI Responses");
    default:
      return protocol;
  }
}
