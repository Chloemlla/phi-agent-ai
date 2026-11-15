// Notification store. Toasts are small, deduplicated and self-dismissing so a
// refresh loop can report the same failure without stacking up cards.

import { createSignal } from "solid-js";

export type ToastTone = "info" | "ok" | "warn" | "danger";

export type Toast = {
  id: number;
  tone: ToastTone;
  message: string;
};

const AUTO_DISMISS_MS = 6000;
const MAX_VISIBLE = 4;

const [toasts, setToasts] = createSignal<Toast[]>([]);
const timers = new Map<number, number>();
let nextId = 1;

function clearTimer(id: number) {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
}

function armTimer(id: number) {
  clearTimer(id);
  const timer = window.setTimeout(() => dismissToast(id), AUTO_DISMISS_MS);
  timers.set(id, timer);
}

export { toasts };

export function pushToast(tone: ToastTone, message: string): void {
  const text = String(message ?? "").trim();
  if (!text) return;

  // Identical messages collapse into the existing toast; the timer restarts so
  // the notice stays visible for a moment after the last occurrence. Tone is
  // part of the identity so a later warning is never hidden by an info toast.
  const existing = toasts().find(
    (toast) => toast.message === text && toast.tone === tone,
  );
  if (existing) {
    armTimer(existing.id);
    return;
  }

  const id = nextId;
  nextId += 1;
  setToasts((current) => {
    const next = [...current, { id, tone, message: text }];
    while (next.length > MAX_VISIBLE) {
      const removed = next.shift();
      if (removed) clearTimer(removed.id);
    }
    return next;
  });
  armTimer(id);
}

export function dismissToast(id: number): void {
  clearTimer(id);
  setToasts((current) => current.filter((toast) => toast.id !== id));
}
