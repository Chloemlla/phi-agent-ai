// Context meter: a quiet ring in the composer shows how much of the model's
// window the latest prompt occupies. The pinned bubble breaks the prompt into
// its assembled segments and reports session-wide throughput.
//
// `usedTokens` and `contextWindow` are provider readings. The per-segment
// token split is estimated from the character counts the backend journals;
// the estimate is labelled in the panel instead of being presented as exact.

import { createMemo, createSignal, Show } from "solid-js";
import { formatCompactTokens } from "../format";
import { t } from "../i18n";
import type { ContextUsageView } from "../types";
import { Alt } from "./Alt";
import { IconChevron } from "./Icons";

function clampPercent(value: number): number {
  return Math.max(0, Math.min(100, value));
}

function percentOf(value: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null;
  return clampPercent((value / total) * 100);
}

function formatPercent(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(1)}%`;
}

export function ContextMeter(props: { usage: ContextUsageView | null }) {
  const [expanded, setExpanded] = createSignal(true);

  const window = () => props.usage?.contextWindow ?? null;
  const used = () => props.usage?.usedTokens ?? 0;
  const percent = () => percentOf(used(), window());
  const hasUsage = () => percent() !== null;

  const tone = () => {
    const value = percent();
    if (value === null) return "neutral";
    if (value >= 95) return "danger";
    if (value >= 80) return "warn";
    return "accent";
  };

  const ringLabel = () => {
    if (!hasUsage()) return "—";
    const value = percent() ?? 0;
    return value < 1 ? "<1" : String(Math.round(value));
  };

  const segments = createMemo(() => {
    const usage = props.usage;
    if (!usage) return null;
    const chars = usage.historyChars + usage.toolsChars + usage.systemChars;
    if (chars <= 0) {
      return { history: 0, tools: 0, system: 0, remaining: window() ?? 0 };
    }
    const total = usage.usedTokens;
    const history = Math.round((total * usage.historyChars) / chars);
    const tools = Math.round((total * usage.toolsChars) / chars);
    const system = Math.max(0, total - history - tools);
    const remaining = window() !== null ? Math.max(0, (window() ?? 0) - total) : 0;
    return { history, tools, system, remaining };
  });

  const shares = createMemo(() => {
    const split = segments();
    if (!split) return null;
    const scale = window() ?? used();
    return {
      history: percentOf(split.history, scale) ?? 0,
      tools: percentOf(split.tools, scale) ?? 0,
      system: percentOf(split.system, scale) ?? 0,
      remaining: percentOf(split.remaining, scale) ?? 0,
    };
  });

  const averageSpeed = () => {
    const value = props.usage?.averageTps ?? 0;
    if (!Number.isFinite(value) || value <= 0) return "—";
    return `${value.toFixed(1)} tok/s`;
  };

  const cacheHit = () => {
    const usage = props.usage;
    if (!usage || usage.totalTokens <= 0) return "—";
    return `${(usage.cacheHit * 100).toFixed(1)}%`;
  };

  const panel = (
    <div class="context-panel">
      <button
        type="button"
        class="context-panel-head"
        aria-expanded={expanded()}
        onClick={() => setExpanded((value) => !value)}
      >
        <span class="context-panel-title">{t("Context window")}</span>
        <span class="context-panel-value">
          <span class="mono">
            {formatCompactTokens(used())} /{" "}
            {window() !== null ? formatCompactTokens(window() ?? 0) : "—"}
          </span>
          <Show when={hasUsage()}>
            <span class="context-panel-percent">({formatPercent(percent())})</span>
          </Show>
          <span class="context-chevron" classList={{ open: expanded() }}>
            <IconChevron size={12} />
          </span>
        </span>
      </button>

      <Show when={props.usage} fallback={<p class="context-note">{t("No model calls yet.")}</p>}>
        <div class="context-collapsible" classList={{ open: expanded() }} aria-hidden={!expanded()}>
          <div class="context-collapsible-clip">
            <div class="context-bar" aria-hidden="true">
              <span
                class="context-bar-seg seg-history"
                style={{ width: `${shares()?.history ?? 0}%` }}
              />
              <span
                class="context-bar-seg seg-tools"
                style={{ width: `${shares()?.tools ?? 0}%` }}
              />
              <span
                class="context-bar-seg seg-system"
                style={{ width: `${shares()?.system ?? 0}%` }}
              />
              <span
                class="context-bar-seg seg-remaining"
                style={{ width: `${shares()?.remaining ?? 0}%` }}
              />
            </div>

            <ul class="context-legend">
              <li>
                <span class="context-swatch seg-history" aria-hidden="true" />
                <span class="context-legend-label">{t("Messages")}</span>
                <span class="mono context-legend-value">
                  {formatCompactTokens(segments()?.history ?? 0)}
                </span>
                <span class="context-legend-percent">
                  {formatPercent(shares()?.history ?? null)}
                </span>
              </li>
              <li>
                <span class="context-swatch seg-tools" aria-hidden="true" />
                <span class="context-legend-label">{t("System tools")}</span>
                <span class="mono context-legend-value">
                  {formatCompactTokens(segments()?.tools ?? 0)}
                </span>
                <span class="context-legend-percent">{formatPercent(shares()?.tools ?? null)}</span>
              </li>
              <li>
                <span class="context-swatch seg-system" aria-hidden="true" />
                <span class="context-legend-label">{t("System prompt")}</span>
                <span class="mono context-legend-value">
                  {formatCompactTokens(segments()?.system ?? 0)}
                </span>
                <span class="context-legend-percent">
                  {formatPercent(shares()?.system ?? null)}
                </span>
              </li>
              <li>
                <span class="context-swatch seg-remaining" aria-hidden="true" />
                <span class="context-legend-label">{t("Remaining")}</span>
                <span class="mono context-legend-value">
                  {window() !== null ? formatCompactTokens(segments()?.remaining ?? 0) : "—"}
                </span>
                <span class="context-legend-percent">
                  {formatPercent(shares()?.remaining ?? null)}
                </span>
              </li>
            </ul>
            <p class="context-note">{t("Estimated from character share.")}</p>
          </div>
        </div>

        <div class="context-divider" />

        <div class="context-panel-title">{t("Session stats")}</div>
        <dl class="context-stats">
          <div>
            <dt>{t("Turns / steps")}</dt>
            <dd class="mono">
              {props.usage?.turns ?? 0} / {props.usage?.steps ?? 0}
            </dd>
          </div>
          <div>
            <dt>{t("Total tokens")}</dt>
            <dd class="mono">{formatCompactTokens(props.usage?.totalTokens ?? 0)}</dd>
          </div>
          <div>
            <dt>{t("Average speed")}</dt>
            <dd class="mono">{averageSpeed()}</dd>
          </div>
          <div>
            <dt>{t("Cache hit")}</dt>
            <dd class="mono">{cacheHit()}</dd>
          </div>
        </dl>
      </Show>
    </div>
  );

  return (
    <Alt
      content={panel}
      class="alt-bubble--context"
      clickToOpen
      interactive
      delay={120}
      offsetX={-14}
    >
      <button
        type="button"
        class="context-ring"
        data-tone={tone()}
        aria-haspopup="dialog"
        aria-label={
          hasUsage() ? `${t("Context usage")} ${formatPercent(percent())}` : t("Context usage")
        }
        title={t("Context usage")}
        style={{
          "--context-progress": `${(percent() ?? 0) * 3.6}deg`,
        }}
      >
        <span class="context-ring-value">{ringLabel()}</span>
      </button>
    </Alt>
  );
}
