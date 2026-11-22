// Run bar: live usage for the current run. There is no budget meter and no
// price; token counts are informational.

import { Show } from "solid-js";
import { formatCompactTokens, formatElapsed, formatTokens } from "../format";
import { t, tArgs } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Run } from "../types";

export function Runbar(props: { run: Run | null; now: number; onCancel: (runId: string) => void }) {
  const steps = () => props.run?.steps ?? 0;
  const inputTokens = () => props.run?.inputTokens ?? 0;
  const outputTokens = () => props.run?.outputTokens ?? 0;

  const elapsed = () => {
    const run = props.run;
    if (!run) return 0;
    const start = run.startedAt ? Date.parse(run.startedAt) : NaN;
    if (!Number.isFinite(start)) return 0;
    const end = isActiveState(run.state) ? props.now : Date.parse(run.updatedAt);
    if (!Number.isFinite(end)) return 0;
    return Math.max(0, (end - start) / 1000);
  };

  const spinning = () => props.run?.state === "Scheduled" || props.run?.state === "Running";

  const tokenTitle = () =>
    tArgs("Input {input} · Output {output} tokens", {
      input: formatTokens(inputTokens()),
      output: formatTokens(outputTokens()),
    });

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
        <span class="run-stat" title={tokenTitle()}>
          <span class="run-stat-label">{t("Tokens")}</span>
          <span class="mono token-pair">
            <span>
              {formatCompactTokens(inputTokens())} <span class="token-arrow">↑</span>
            </span>
            <span class="token-sep">/</span>
            <span>
              {formatCompactTokens(outputTokens())} <span class="token-arrow">↓</span>
            </span>
          </span>
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
