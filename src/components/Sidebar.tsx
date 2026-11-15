// Session rail. The active item is a white card with a 3px accent indicator,
// not a filled blue row: selection should be legible without shouting.

import { For, Show } from "solid-js";
import { formatRelative } from "../format";
import { t, tArgs } from "../i18n";
import { modeTone, runTone, stateLabel } from "../status";
import type { SessionSummary } from "../types";
import { IconPlus } from "./Icons";

export function Sidebar(props: {
  sessions: SessionSummary[];
  currentId: string | null;
  open: boolean;
  mode: string;
  nodeId: string;
  now: number;
  onNew: () => void;
  onSelect: (id: string) => void;
}) {
  return (
    <aside classList={{ sidebar: true, collapsed: !props.open }}>
      <div class="sidebar-inner">
        <button class="btn primary block new-session" onClick={props.onNew}>
          <IconPlus size={15} />
          {t("New session")}
        </button>

        <div class="side-label">{t("Sessions")}</div>

        <div class="session-list">
          <For each={props.sessions}>
            {(session) => (
              <button
                classList={{ session: true, active: session.id === props.currentId }}
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
