// Run bar: live usage for the current run. There is no budget meter; these
// numbers are informational, and the run ends when the task is done or the
// user cancels.

import { Show } from "solid-js";
import { formatElapsed, formatMoney } from "../format";
import { t } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Run } from "../types";

export function Runbar(props: {
  run: Run | null;
  now: number;
  onCancel: (runId: string) => void;
}) {
  const steps = () => props.run?.steps ?? 0;
  const cost = () => props.run?.costUsd ?? 0;

  const elapsed = () => {
    const run = props.run;
    if (!run) return 0;
    const start = run.startedAt ? Date.parse(run.startedAt) : NaN;
    if (!Number.isFinite(start)) return 0;
    const end = isActiveState(run.state) ? props.now : Date.parse(run.updatedAt);
    if (!Number.isFinite(end)) return 0;
    return Math.max(0, (end - start) / 1000);
  };

  const spinning = () =>
    props.run?.state === "Scheduled" || props.run?.state === "Running";

  return (
    <div class="runbar">
      <Show when={props.run}>
        {(run) => (
          <span
            classList={{
              "run-state": true,
              live: spinning(),
              waiting: run().state === "WaitingApproval",
            }}
          >
            <Show
              when={spinning()}
              fallback={
                <span
                  classList={{
                    dot: true,
                    [`tone-${runTone(run().state)}`]: true,
                  }}
                />
              }
            >
              <span class="ring" />
            </Show>
            {stateLabel(run().state)}
          </span>
        )}
      </Show>

      <div class="run-stats">
        <span class="run-stat">
          <span class="run-stat-label">{t("Steps")}</span>
          <span class="mono">{steps()}</span>
        </span>
        <span class="run-stat">
          <span class="run-stat-label">{t("Cost")}</span>
          <span class="mono">{formatMoney(cost())}</span>
        </span>
        <span class="run-stat">
          <span class="run-stat-label">{t("Time")}</span>
          <span class="mono">{formatElapsed(elapsed())}</span>
        </span>
      </div>

      <Show when={props.run && isActiveState(props.run.state)}>
        <button
          class="btn danger small cancel"
          onClick={() => props.run && props.onCancel(props.run.id)}
        >
          {t("Cancel")}
        </button>
      </Show>
    </div>
  );
}
