// Status semantics shared by the run bar, session list and title bar.
//
// Tone names map to CSS classes; the visual weight always follows risk, so a
// high-risk approval is structurally heavier than a medium or low one.

import { t } from "./i18n";

export type Tone = "live" | "ok" | "warn" | "danger" | "idle";
export type RiskLevel = "low" | "medium" | "high";

const STATE_LABELS: Record<string, string> = {
  Scheduled: "Scheduled",
  Running: "Running",
  WaitingApproval: "Waiting for approval",
  Succeeded: "Succeeded",
  Failed: "Failed",
  Aborted: "Aborted",
  AwaitingReconciliation: "Needs review",
  Normal: "Normal",
  Degraded: "Degraded",
  Maintenance: "Maintenance",
};

export function stateLabel(state: string | null | undefined): string {
  if (!state) return "";
  return t(STATE_LABELS[state] ?? state);
}

export function isActiveState(state: string | null | undefined): boolean {
  return state === "Scheduled" || state === "Running" || state === "WaitingApproval";
}

export function runTone(state: string | null | undefined): Tone {
  switch (state) {
    case "Scheduled":
    case "Running":
      return "live";
    case "WaitingApproval":
    case "AwaitingReconciliation":
      return "warn";
    case "Succeeded":
      return "ok";
    case "Failed":
    case "Aborted":
      return "danger";
    default:
      return "idle";
  }
}

export function modeTone(mode: string | null | undefined): Tone {
  switch (mode) {
    case "Normal":
      return "ok";
    case "Degraded":
      return "warn";
    case "Maintenance":
      return "danger";
    default:
      return "idle";
  }
}

// The backend reports a structured risk tier; the human sentence is only a
// fallback for journals written before the field existed. Tool identity is the
// last resort because it is a stable semantic signal.
export function riskLevel(level: string | undefined, toolId: string, risk: string): RiskLevel {
  if (level === "low" || level === "medium" || level === "high") return level;
  if (toolId === "shell_exec") return "high";
  if (toolId === "fs_write") return "medium";
  const lowered = risk.toLowerCase();
  if (lowered.includes("irreversible")) return "high";
  if (lowered.includes("reversible")) return "medium";
  return "low";
}

export function riskTagText(level: RiskLevel): string {
  switch (level) {
    case "high":
      return t("High risk");
    case "medium":
      return t("Medium risk");
    default:
      return t("Low risk");
  }
}
