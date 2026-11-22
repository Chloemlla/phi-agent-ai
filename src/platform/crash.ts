// Uncaught webview failures, handed to the native shell.
//
// A JS error in the webview never reaches the JVM: Solid's boundary may recover
// it, or the page may sit in a broken state until the user reloads, but nothing
// is recorded. The shell writes these entries into the crash inbox as
// non-fatal, so the next real crash report carries the frontend errors that led
// to it instead of arriving with no context at all.

import { invoke } from "@tauri-apps/api/core";

/** Failures reported per session; a render loop must not flood the inbox. */
const MAX_REPORTS = 5;

const seen = new Set<string>();
let reported = 0;

function isNativeShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function stackOf(error: unknown): string {
  if (error instanceof Error && typeof error.stack === "string") return error.stack;
  return "";
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

/** Records one recoverable failure. Never throws and never blocks the caller. */
export function reportWebFailure(source: string, message: string, stack = ""): void {
  if (!isNativeShell() || reported >= MAX_REPORTS) return;
  const trimmed = message.trim();
  if (trimmed.length === 0) return;
  const key = `${source}\u0000${trimmed}`;
  if (seen.has(key)) return;
  seen.add(key);
  reported += 1;
  void invoke("record_web_failure", { source, message: trimmed, stack }).catch(() => undefined);
}

/**
 * Subscribes to the two webview-level failure channels.
 *
 * Call once, before the app renders: errors thrown during startup are the ones
 * most likely to leave the user staring at a blank window.
 */
export function installWebFailureReporting(): void {
  if (!isNativeShell()) return;

  window.addEventListener("error", (event) => {
    // Resource load failures (a missing image, a script that 404s) also land
    // here, but they are plain events without a message.
    const message = typeof event.message === "string" ? event.message : "";
    if (message.length === 0) return;
    reportWebFailure("webview.js", message, stackOf(event.error));
  });

  window.addEventListener("unhandledrejection", (event) => {
    reportWebFailure(
      "webview.js",
      `Unhandled rejection: ${messageOf(event.reason)}`,
      stackOf(event.reason),
    );
  });
}
