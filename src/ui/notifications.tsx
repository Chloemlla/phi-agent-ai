// Notification store and stack. Operation outcomes (a failed save, a deleted
// session, a copied block) surface here; field validation stays inline with the
// field it belongs to.

import { CheckCircle2, Info, TriangleAlert, X, XCircle } from "lucide-solid";
import { For, Show, createSignal } from "solid-js";
import { Portal } from "solid-js/web";
import { t } from "../i18n";
import { isMobileShell } from "../layout/environment";
import "./notifications.css";

export type NotificationTone = "info" | "success" | "warning" | "danger";

export interface NotificationAction {
  label: string;
  onClick: () => void;
}

export interface NotificationOptions {
  tone?: NotificationTone;
  duration?: number;
  action?: NotificationAction;
}

export interface NotificationItem {
  id: number;
  message: string;
  tone: NotificationTone;
  duration: number;
  leaving: boolean;
  action?: NotificationAction;
}

const EXIT_MS = 140;
const MAX_VISIBLE = 4;
const DEFAULT_DURATION: Record<NotificationTone, number> = {
  info: 4000,
  success: 3600,
  warning: 5000,
  danger: 6000,
};

const [items, setItems] = createSignal<NotificationItem[]>([]);
const timers = new Map<number, number>();
let nextId = 1;

export { items as notifications };

function clearTimer(id: number) {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
}

function armTimer(id: number, duration: number) {
  clearTimer(id);
  if (duration <= 0) return;
  const timer = window.setTimeout(() => dismissNotification(id), duration);
  timers.set(id, timer);
}

export function notify(message: string, options: NotificationOptions = {}): number {
  const text = String(message ?? "").trim();
  if (!text) return -1;
  const tone = options.tone ?? "info";

  // Identical messages collapse into the existing card and restart its timer.
  const existing = items().find(
    (item) => !item.leaving && item.message === text && item.tone === tone,
  );
  if (existing) {
    armTimer(existing.id, options.duration ?? existing.duration);
    return existing.id;
  }

  const id = nextId++;
  const duration = options.action !== undefined ? 0 : (options.duration ?? DEFAULT_DURATION[tone]);
  const item: NotificationItem = {
    id,
    message: text,
    tone,
    duration,
    leaving: false,
    ...(options.action ? { action: options.action } : {}),
  };
  setItems((current) => {
    const next = [...current, item];
    while (next.length > MAX_VISIBLE) {
      const removed = next.shift();
      if (removed) clearTimer(removed.id);
    }
    return next;
  });
  armTimer(id, duration);
  return id;
}

export function dismissNotification(id: number) {
  clearTimer(id);
  setItems((current) =>
    current.map((item) => (item.id === id && !item.leaving ? { ...item, leaving: true } : item)),
  );
  window.setTimeout(() => {
    setItems((current) => current.filter((item) => item.id !== id));
  }, EXIT_MS);
}

const ICONS: Record<NotificationTone, typeof Info> = {
  info: Info,
  success: CheckCircle2,
  warning: TriangleAlert,
  danger: XCircle,
};

const TONE_LABEL: Record<NotificationTone, string> = {
  info: "Note",
  success: "Success",
  warning: "Warning",
  danger: "Error",
};

export function NotificationContainer() {
  return (
    <Portal>
      <aside
        class="ui-notifications"
        classList={{ "ui-notifications--mobile": isMobileShell() }}
        aria-label={t("Notifications")}
        aria-live="polite"
      >
        <For each={items()}>
          {(item) => {
            const Icon = ICONS[item.tone];
            return (
              <div
                class="ui-notification"
                data-tone={item.tone}
                data-leaving={item.leaving ? "true" : undefined}
                role={item.tone === "danger" ? "alert" : "status"}
              >
                <Icon size={18} class="ui-notification-icon" aria-hidden="true" />
                <span class="sr-only">{`${t(TONE_LABEL[item.tone])}: `}</span>
                <span class="ui-notification-message">{item.message}</span>
                <Show when={item.action}>
                  {(action) => (
                    <button
                      type="button"
                      class="ui-notification-action"
                      onClick={() => {
                        dismissNotification(item.id);
                        action().onClick();
                      }}
                    >
                      {action().label}
                    </button>
                  )}
                </Show>
                <button
                  type="button"
                  class="ui-notification-close"
                  title={t("Dismiss")}
                  aria-label={t("Dismiss")}
                  onClick={() => dismissNotification(item.id)}
                >
                  <X size={15} aria-hidden="true" />
                </button>
              </div>
            );
          }}
        </For>
      </aside>
    </Portal>
  );
}
