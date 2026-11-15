// Number, time and duration formatting. Everything here feeds the mono face:
// data should always line up and never jump when a value changes width.

import { lang, t, tArgs } from "./i18n";

function locale(): string {
  return lang() === "zh-CN" ? "zh-CN" : "en-US";
}

export function formatMoney(value: number, digits = 3): string {
  return `$${value.toFixed(digits)}`;
}

export function formatTokens(value: number): string {
  return value.toLocaleString(locale());
}

export function formatClock(iso: string): string {
  const time = Date.parse(iso);
  if (!Number.isFinite(time)) return "";
  return new Date(time).toLocaleTimeString(locale(), {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function formatElapsed(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
  }
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

export function secondsBetween(fromIso: string | null, toMs: number): number {
  if (!fromIso) return 0;
  const from = Date.parse(fromIso);
  if (!Number.isFinite(from)) return 0;
  return Math.max(0, (toMs - from) / 1000);
}

export function formatCountdown(expiresAt: string, now: number): string {
  const expires = Date.parse(expiresAt);
  if (!Number.isFinite(expires)) return "";
  const remaining = expires - now;
  if (remaining <= 0) return t("Expired");
  return formatElapsed(remaining / 1000);
}

export function formatRelative(iso: string, now: number): string {
  const then = Date.parse(iso);
  if (!Number.isFinite(then)) return "";
  const seconds = Math.max(0, Math.round((now - then) / 1000));
  if (seconds < 45) return t("just now");
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return tArgs("{n} min ago", { n: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 24) return tArgs("{n} h ago", { n: hours });
  const days = Math.round(hours / 24);
  if (days < 7) return tArgs("{n} d ago", { n: days });
  return new Date(then).toLocaleDateString(locale(), {
    month: "short",
    day: "numeric",
  });
}
