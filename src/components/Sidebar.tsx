// Session rail. Rows carry a status dot, the title and a quiet overflow menu
// (rename, delete) that appears on hover or keyboard focus. Hovering a row
// opens a glass bubble with the details that no longer fit. On narrow windows
// and mobile shells the same element becomes a modal drawer.

import { For, Show, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js";
import { useChat } from "../core/chat/ChatProvider";
import { formatFullTime, formatRelative } from "../format";
import { t, tArgs } from "../i18n";
import { stateLabel } from "../status";
import type { SessionSummary } from "../types";
import { Alt } from "./Alt";
import { EmptyState } from "./EmptyState";
import {
  IconCheck,
  IconChevronLeft,
  IconCircleAlert,
  IconClock,
  IconClose,
  IconEllipsis,
  IconHash,
  IconMessageSquare,
  IconPencil,
  IconPlus,
  IconSettings,
  IconSpinner,
  IconTrash,
} from "./Icons";
import { SideScrollbar } from "./SideScrollbar";
import { FloatingMenu } from "../ui/Menu";

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

interface SidebarProps {
  open: boolean;
  modal: boolean;
  settingsOpen: boolean;
  onToggle: () => void;
  onOpenSettings: () => void;
  onNavigate: () => void;
}

export function Sidebar(props: SidebarProps) {
  const chat = useChat();
  let rootRef: HTMLElement | undefined;
  const [listEl, setListEl] = createSignal<HTMLElement | null>(null);
  const [menu, setMenu] = createSignal<{ session: SessionSummary; anchor: HTMLElement } | null>(
    null,
  );
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [editValue, setEditValue] = createSignal("");
  let renameInput: HTMLInputElement | undefined;

  const sessions = () => chat.boot()?.sessions ?? [];

  function startRename(session: SessionSummary) {
    setMenu(null);
    setEditingId(session.id);
    setEditValue(session.title || "");
    queueMicrotask(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  }

  function commitRename(session: SessionSummary) {
    if (editingId() !== session.id) return;
    const value = editValue().trim();
    setEditingId(null);
    if (value && value !== session.title) void chat.renameSession(session.id, value);
  }

  function cancelRename() {
    setEditingId(null);
  }

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
      previous?.focus?.();
    });
  });

  const collapseLabel = () => (props.modal ? t("Close sidebar") : t("Collapse sidebar"));

  return (
    <aside
      ref={rootRef}
      classList={{
        sidebar: true,
        open: props.open,
        collapsed: !props.open && !props.modal,
        modal: props.modal,
      }}
      aria-label={t("Sessions")}
      aria-hidden={props.open ? undefined : "true"}
      inert={props.open ? undefined : true}
    >
      <div class="sidebar-inner">
        <div class="side-head">
          <span class="side-label">{t("Sessions")}</span>
          <div class="side-head-actions">
            <button
              type="button"
              class="icon-btn side-new"
              title={chat.creatingSession() ? t("Creating session…") : t("New session")}
              aria-label={chat.creatingSession() ? t("Creating session…") : t("New session")}
              disabled={chat.creatingSession()}
              onClick={() => {
                props.onNavigate();
                void chat.newSession();
              }}
            >
              <Show when={chat.creatingSession()} fallback={<IconPlus size={16} />}>
                <IconSpinner size={16} class="spin" />
              </Show>
            </button>
            <button
              type="button"
              class="icon-btn side-collapse"
              title={collapseLabel()}
              aria-label={collapseLabel()}
              onClick={() => props.onToggle()}
            >
              <Show when={props.modal} fallback={<IconChevronLeft size={16} />}>
                <IconClose size={16} />
              </Show>
            </button>
          </div>
        </div>

        <div class="side-scroll-shell">
          <div class="session-list" ref={setListEl}>
            <Show when={sessions().length === 0}>
              <EmptyState
                icon={<IconMessageSquare size={20} />}
                title={t("No sessions")}
                description={t("Create a session to start a task.")}
              />
            </Show>
            <For each={sessions()}>
              {(session) => {
                const status = createMemo(() => statusOf(session));
                const active = () => session.id === chat.currentId();
                const deleting = () => chat.deletingSession() === session.id;
                const editing = () => editingId() === session.id;
                const title = () => session.title || t("New session");

                return (
                  <div
                    class="session-row"
                    classList={{
                      active: active(),
                      deleting: deleting(),
                      editing: editing(),
                    }}
                  >
                    <Show
                      when={editing()}
                      fallback={
                        <>
                          <Alt
                            content={
                              <SessionBubble
                                session={session}
                                now={() => chat.now()}
                                status={status}
                              />
                            }
                            class="alt-bubble--session"
                            delay={200}
                          >
                            <button
                              type="button"
                              class="session-select"
                              aria-current={active() ? "true" : undefined}
                              onClick={() => {
                                props.onNavigate();
                                void chat.selectSession(session.id);
                              }}
                            >
                              <span
                                class="session-state-dot"
                                data-tone={status().tone}
                                title={status().label}
                                aria-hidden="true"
                              />
                              <span class="session-title">{title()}</span>
                            </button>
                          </Alt>
                          <button
                            type="button"
                            class="session-more"
                            title={t("Session actions")}
                            aria-label={tArgs("Session actions for {title}", { title: title() })}
                            aria-haspopup="menu"
                            onClick={(event) => {
                              const anchor = event.currentTarget;
                              setMenu({ session, anchor });
                            }}
                          >
                            <Show when={deleting()} fallback={<IconEllipsis size={16} />}>
                              <IconSpinner size={15} class="spin" />
                            </Show>
                          </button>
                        </>
                      }
                    >
                      <input
                        ref={(element) => (renameInput = element)}
                        class="session-rename"
                        value={editValue()}
                        aria-label={t("Rename session")}
                        onInput={(event) => setEditValue(event.currentTarget.value)}
                        onBlur={() => commitRename(session)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            event.currentTarget.blur();
                          } else if (event.key === "Escape") {
                            event.preventDefault();
                            cancelRename();
                          }
                        }}
                      />
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
          <SideScrollbar host={listEl} />
        </div>

        <div class="sidebar-foot">
          <button
            type="button"
            class="sidebar-settings"
            classList={{ active: props.settingsOpen }}
            aria-current={props.settingsOpen ? "page" : undefined}
            onClick={() => {
              props.onNavigate();
              props.onOpenSettings();
            }}
          >
            <IconSettings size={16} />
            <span>{t("Settings")}</span>
          </button>
        </div>
      </div>

      <Show when={menu()}>
        {(current) => (
          <FloatingMenu
            anchor={current().anchor}
            label={t("Session actions")}
            header={{
              title: current().session.title || t("New session"),
              subtitle: t("Session"),
              icon: <IconMessageSquare size={15} />,
            }}
            items={[
              {
                label: t("Rename"),
                icon: <IconPencil size={15} />,
                onSelect: () => startRename(current().session),
              },
              {
                label: t("Delete session"),
                icon: <IconTrash size={15} />,
                danger: true,
                disabled:
                  chat.deletingSession() === current().session.id ||
                  ["Scheduled", "Running", "WaitingApproval"].includes(
                    current().session.lastRunState ?? "",
                  ),
                onSelect: () => {
                  props.onNavigate();
                  void chat.deleteSession(current().session.id);
                },
              },
            ]}
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
    </aside>
  );
}
