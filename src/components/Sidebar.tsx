// Session rail. Rows stay single-line: title plus a quiet delete action that
// appears on hover or keyboard focus. Hovering a row opens a glass bubble with
// the details that no longer fit in the row (state, message count, time, id).
// On narrow windows the same element becomes a drawer.

import { For, Show, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js";
import { formatFullTime, formatRelative } from "../format";
import { t, tArgs } from "../i18n";
import { modeTone, stateLabel } from "../status";
import type { SessionSummary } from "../types";
import { Alt } from "./Alt";
import { EmptyState } from "./EmptyState";
import {
  IconCheck,
  IconCircleAlert,
  IconClock,
  IconHash,
  IconMessageSquare,
  IconPlus,
  IconSpinner,
  IconTrash,
} from "./Icons";
import { SideScrollbar } from "./SideScrollbar";

type SessionStatus = {
  tone: "live" | "ok" | "warn" | "danger" | "idle";
  label: string;
  spinning: boolean;
  icon: JSX.Element | null;
};

function statusOf(session: SessionSummary): SessionStatus {
  const state = session.lastRunState;
  if (!state) {
    return {
      tone: "idle",
      label: t("Draft"),
      spinning: false,
      icon: <IconMessageSquare size={13} />,
    };
  }
  switch (state) {
    case "Scheduled":
    case "Running":
      return { tone: "live", label: stateLabel(state), spinning: true, icon: null };
    case "WaitingApproval":
    case "AwaitingReconciliation":
      return {
        tone: "warn",
        label: stateLabel(state),
        spinning: false,
        icon: <IconCircleAlert size={13} />,
      };
    case "Succeeded":
      return {
        tone: "ok",
        label: stateLabel(state),
        spinning: false,
        icon: <IconCheck size={13} />,
      };
    case "Failed":
    case "Aborted":
      return {
        tone: "danger",
        label: stateLabel(state),
        spinning: false,
        icon: <IconCircleAlert size={13} />,
      };
    default:
      return {
        tone: "idle",
        label: stateLabel(state),
        spinning: false,
        icon: <IconMessageSquare size={13} />,
      };
  }
}

function SessionBubble(props: {
  session: SessionSummary;
  now: () => number;
  status: () => SessionStatus;
}) {
  const title = () => props.session.title || t("New session");
  return (
    <div class="alt-session">
      <span class="alt-session-title">{title()}</span>
      <span class="alt-session-row">
        <IconMessageSquare size={12} />
        <span>{tArgs("Messages: {n}", { n: props.session.messageCount })}</span>
      </span>
      <span class="alt-session-row">
        <IconClock size={12} />
        <span>{formatRelative(props.session.updatedAt, props.now())}</span>
        <span class="alt-session-exact">{formatFullTime(props.session.updatedAt)}</span>
      </span>
      <span class="alt-session-row">
        <span class="alt-session-dot" data-tone={props.status().tone} aria-hidden="true" />
        <span>{props.status().label}</span>
      </span>
      <span class="alt-session-row">
        <IconHash size={12} />
        <code>{props.session.id.slice(0, 12)}</code>
      </span>
    </div>
  );
}

export function Sidebar(props: {
  sessions: SessionSummary[];
  currentId: string | null;
  open: boolean;
  modal: boolean;
  mode: string;
  nodeId: string;
  now: number;
  creating?: boolean;
  deletingId?: string | null;
  onNew: () => void;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  let rootRef: HTMLElement | undefined;
  const [listEl, setListEl] = createSignal<HTMLElement | null>(null);
  const [pendingDelete, setPendingDelete] = createSignal<string | null>(null);
  let pendingTimer: number | undefined;

  function clearPendingTimer() {
    if (pendingTimer !== undefined) {
      window.clearTimeout(pendingTimer);
      pendingTimer = undefined;
    }
  }

  function requestDelete(session: SessionSummary) {
    if (props.deletingId === session.id) return;
    if (pendingDelete() === session.id) {
      clearPendingTimer();
      setPendingDelete(null);
      props.onDelete(session.id);
      return;
    }
    clearPendingTimer();
    setPendingDelete(session.id);
    pendingTimer = window.setTimeout(() => {
      setPendingDelete(null);
      pendingTimer = undefined;
    }, 3000);
  }

  createEffect(() => {
    if (props.deletingId) {
      clearPendingTimer();
      setPendingDelete(null);
    }
  });

  // On phones/tablets the rail is a modal drawer: focus enters it, stays
  // inside, and returns to the trigger when it closes.
  createEffect(() => {
    if (!props.modal || !props.open) return;
    const root = rootRef;
    if (!root) return;
    const previous = document.activeElement as HTMLElement | null;
    const focusable = () =>
      Array.from(
        root.querySelectorAll<HTMLElement>(
          'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => element.offsetParent !== null);
    queueMicrotask(() => focusable()[0]?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const items = focusable();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    root.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      root.removeEventListener("keydown", onKeyDown);
      clearPendingTimer();
      previous?.focus?.();
    });
  });

  return (
    <aside
      ref={rootRef}
      classList={{
        sidebar: true,
        open: props.open,
        collapsed: !props.open,
      }}
      aria-label={t("Sessions")}
      aria-hidden={props.open ? undefined : "true"}
      inert={props.open ? undefined : true}
    >
      <div class="sidebar-inner">
        <div class="side-head">
          <span class="side-label">{t("Sessions")}</span>
          <button
            type="button"
            class="icon-btn side-new"
            title={props.creating ? t("Creating session…") : t("New session")}
            aria-label={props.creating ? t("Creating session…") : t("New session")}
            disabled={props.creating}
            onClick={props.onNew}
          >
            <Show when={props.creating} fallback={<IconPlus size={15} />}>
              <IconSpinner size={15} class="spin" />
            </Show>
          </button>
        </div>

        <div class="side-scroll-shell">
          <div class="session-list" ref={setListEl}>
            <Show when={props.sessions.length === 0}>
              <EmptyState
                icon={<IconMessageSquare size={20} />}
                title={t("No sessions")}
                description={t("Create a session to start a task.")}
              />
            </Show>
            <For each={props.sessions}>
              {(session) => {
                const status = createMemo(() => statusOf(session));
                const active = () => session.id === props.currentId;
                const deleting = () => props.deletingId === session.id;
                const confirming = () => pendingDelete() === session.id;
                const busy = () => {
                  const state = session.lastRunState;
                  return (
                    state === "Scheduled" || state === "Running" || state === "WaitingApproval"
                  );
                };
                return (
                  <div
                    class="session-row"
                    classList={{
                      active: active(),
                      deleting: deleting(),
                      confirm: confirming(),
                    }}
                  >
                    <Alt
                      content={
                        <SessionBubble session={session} now={() => props.now} status={status} />
                      }
                      class="alt-bubble--session"
                      delay={200}
                      offsetX={12}
                    >
                      <button
                        type="button"
                        class="session-select"
                        aria-current={active() ? "true" : undefined}
                        onClick={() => props.onSelect(session.id)}
                      >
                        <span class="session-title">{session.title || t("New session")}</span>
                      </button>
                    </Alt>
                    <button
                      type="button"
                      class="session-delete"
                      classList={{ confirm: confirming() }}
                      title={
                        busy()
                          ? t("Cancel the active run before deleting this session.")
                          : confirming()
                            ? t("Click again to delete")
                            : t("Delete session")
                      }
                      aria-label={tArgs("Delete session {title}", {
                        title: session.title || t("New session"),
                      })}
                      disabled={deleting() || busy()}
                      onClick={(event) => {
                        event.stopPropagation();
                        requestDelete(session);
                      }}
                    >
                      <Show when={deleting()} fallback={<IconTrash size={14} />}>
                        <IconSpinner size={14} class="spin" />
                      </Show>
                    </button>
                  </div>
                );
              }}
            </For>
          </div>
          <SideScrollbar host={listEl} />
        </div>

        <div class="sidebar-foot" title={props.nodeId}>
          <Show when={props.nodeId}>
            <div class="foot-row">
              <span class="mono">{tArgs("Node {id}", { id: props.nodeId.slice(0, 18) })}</span>
            </div>
            <div class="foot-row">
              <span
                classList={{
                  dot: true,
                  [`tone-${modeTone(props.mode)}`]: true,
                }}
              />
              <span>{tArgs("Mode {mode}", { mode: stateLabel(props.mode) })}</span>
            </div>
          </Show>
        </div>
      </div>
    </aside>
  );
}
