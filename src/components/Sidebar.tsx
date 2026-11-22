// Session rail. The active item is a quiet surface with a 3px accent
// indicator, not a filled blue row: selection should be legible without
// shouting. On narrow windows the same element becomes a drawer.

import { For, Show, createEffect, onCleanup } from "solid-js";
import { formatRelative } from "../format";
import { t, tArgs } from "../i18n";
import { modeTone, runTone, stateLabel } from "../status";
import type { SessionSummary } from "../types";
import { IconPlus } from "./Icons";

export function Sidebar(props: {
  sessions: SessionSummary[];
  currentId: string | null;
  open: boolean;
  modal: boolean;
  mode: string;
  nodeId: string;
  now: number;
  onNew: () => void;
  onSelect: (id: string) => void;
}) {
  let rootRef: HTMLElement | undefined;

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
        <button
          type="button"
          class="btn primary block new-session"
          onClick={props.onNew}
        >
          <IconPlus size={15} />
          {t("New session")}
        </button>

        <div class="side-label">{t("Sessions")}</div>

        <div class="session-list">
          <Show when={props.sessions.length === 0}>
            <p class="session-empty">{t("No sessions yet.")}</p>
          </Show>
          <For each={props.sessions}>
            {(session) => (
              <button
                type="button"
                classList={{
                  session: true,
                  active: session.id === props.currentId,
                }}
                aria-current={session.id === props.currentId ? "true" : undefined}
                onClick={() => props.onSelect(session.id)}
              >
                <span class="session-title">
                  {session.title || t("New session")}
                </span>
                <span class="session-meta">
                  <Show when={session.lastRunState}>
                    <span
                      classList={{
                        dot: true,
                        [`tone-${runTone(session.lastRunState)}`]: true,
                      }}
                    />
                    <span>{stateLabel(session.lastRunState)}</span>
                  </Show>
                  <Show when={!session.lastRunState}>
                    <span class="dot tone-idle" />
                    <span>{t("Draft")}</span>
                  </Show>
                  <span class="session-time mono">
                    {formatRelative(session.updatedAt, props.now)}
                  </span>
                </span>
              </button>
            )}
          </For>
        </div>

        <div class="sidebar-foot" title={props.nodeId}>
          <Show when={props.nodeId}>
            <div class="foot-row">
              <span class="mono">
                {tArgs("Node {id}", { id: props.nodeId.slice(0, 18) })}
              </span>
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
