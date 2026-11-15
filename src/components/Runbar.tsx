// Run bar: the three hard caps stay visible, with a meter whose color moves
// from accent to warn to danger as consumption approaches the limit.

import { Show } from "solid-js";
import { formatElapsed, formatMoney } from "../format";
import { t } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Run } from "../types";

type MeterTone = "accent" | "warn" | "danger";

function meterTone(ratio: number): MeterTone {
  if (ratio < 0.5) return "accent";
  if (ratio < 0.8) return "warn";
  return "danger";
}

function ratio(value: number, max: number): number {
  if (max <= 0) return 0;
  return Math.min(1, Math.max(0, value / max));
}

function Meter(props: {
  label: string;
  value: number;
  max: number;
  text: string;
}) {
  const fraction = () => ratio(props.value, props.max);
  return (
    <div class="meter">
      <div class="meter-head">
        <span class="meter-label">{props.label}</span>
        <span class="meter-value mono">{props.text}</span>
      </div>
      <div
        class="meter-track"
        role="progressbar"
        aria-label={props.label}
        aria-valuemin={0}
        aria-valuemax={props.max}
        aria-valuenow={props.value}
        aria-valuetext={props.text}
      >
        <div
          classList={{
            "meter-fill": true,
            [`tone-${meterTone(fraction())}`]: true,
          }}
          style={{ width: `${fraction() * 100}%` }}
        />
      </div>
    </div>
  );
}

export function Runbar(props: {
  run: Run | null;
  maxSteps: number;
  maxCostUsd: number;
  maxWallSecs: number;
  now: number;
  onCancel: (runId: string) => void;
}) {
  const maxSteps = () => props.run?.maxSteps ?? props.maxSteps;
  const maxCost = () => props.run?.maxCostUsd ?? props.maxCostUsd;
  const maxWall = () => props.run?.maxWallSecs ?? props.maxWallSecs;

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

      <div class="meters">
        <Meter
          label={t("Steps")}
          value={steps()}
          max={maxSteps()}
          text={`${steps()} / ${maxSteps()}`}
        />
        <Meter
          label={t("Cost")}
          value={cost()}
          max={maxCost()}
          text={`${formatMoney(cost())} / ${formatMoney(maxCost(), 2)}`}
        />
        <Meter
          label={t("Time")}
          value={elapsed()}
          max={maxWall()}
          text={`${formatElapsed(elapsed())} / ${formatElapsed(maxWall())}`}
        />
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
