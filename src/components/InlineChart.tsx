// Chart rendering for a `table.ui-chart` inside an inline interface block.
// The model writes ordinary tabular data; the host draws it. The table header
// names the series and the first column supplies the category labels, so the
// block has a readable fallback for assistive technology.

import { For, Show } from "solid-js";
import { t } from "../i18n";

const SERIES_SLOTS = 8;
const WIDTH = 640;

export type ChartSeries = { name: string; color: string };

export type ChartTableData = {
  kind: "bar" | "line" | "area";
  labelHeader: string;
  labels: string[];
  series: ChartSeries[];
  values: number[][];
  height: number;
  caption: string;
};

function seriesColor(index: number, count: number): string {
  if (count === 1) return "var(--chart-single)";
  return index < SERIES_SLOTS ? `var(--chart-${index + 1})` : "var(--chart-other)";
}

function cellText(cell: Element | undefined): string {
  return (cell?.textContent ?? "").replace(/\s+/g, " ").trim();
}

function parseNumber(text: string): number | null {
  const cleaned = text
    .replace(/,/g, "")
    .replace(/[\s%$€£¥]/g, "")
    .replace(/^[−–—]$/, "");
  if (!cleaned) return null;
  const match = cleaned.match(/^(-?[0-9]*\.?[0-9]+)\s*([kKmMbB])?$/);
  if (!match) return null;
  const value = Number.parseFloat(match[1]);
  if (!Number.isFinite(value)) return null;
  const suffix = match[2]?.toLowerCase();
  const scale = suffix === "k" ? 1_000 : suffix === "m" ? 1_000_000 : suffix === "b" ? 1_000_000_000 : 1;
  return value * scale;
}

/** Returns null when the table cannot be read as data; the caller then renders
 * it as an ordinary table instead of guessing. */
export function readChartTable(table: Element): ChartTableData | null {
  const headerCells = Array.from(
    table.querySelectorAll("thead tr:first-child > th"),
  );
  const headerNames = headerCells.map(cellText);
  if (headerNames.length < 2) return null;
  const seriesNames = headerNames.slice(1);
  const rows = Array.from(table.querySelectorAll("tbody tr"));
  if (rows.length === 0) return null;

  const labels: string[] = [];
  const values: number[][] = seriesNames.map(() => []);
  for (const row of rows) {
    const cells = Array.from(row.children).filter(
      (cell) => cell.tagName === "TD" || cell.tagName === "TH",
    );
    if (cells.length < 2) continue;
    labels.push(cellText(cells[0]));
    for (let index = 0; index < seriesNames.length; index += 1) {
      values[index].push(parseNumber(cellText(cells[index + 1])) ?? 0);
    }
  }
  if (labels.length === 0) return null;

  const heightAttr = Number.parseInt(table.getAttribute("data-height") ?? "", 10);
  const height = Number.isFinite(heightAttr)
    ? Math.min(420, Math.max(140, heightAttr))
    : 200;

  const type = table.getAttribute("data-type");
  return {
    kind: type === "line" ? "line" : type === "area" ? "area" : "bar",
    labelHeader: headerNames[0] || t("Label"),
    labels,
    series: seriesNames.map((name, index) => ({
      name,
      color: seriesColor(index, seriesNames.length),
    })),
    values,
    height,
    caption: cellText(table.querySelector("caption") ?? undefined),
  };
}

function formatValue(value: number): string {
  const absolute = Math.abs(value);
  if (absolute >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (absolute >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function truncate(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
}

export function InlineChart(props: { data: ChartTableData }) {
  const data = () => props.data;
  const top = 14;
  const left = 8;
  const plotWidth = WIDTH - left * 2;
  const plotHeight = data().height - top - 26;

  // Bars always start at zero. Lines and areas use the data range so a small
  // change does not collapse into a flat line at the top of the plot.
  const bounds = () => {
    const values = data().values.flat().filter((value) => Number.isFinite(value));
    if (data().kind === "bar") {
      return { low: 0, high: Math.max(1, ...values, 0) * 1.05 };
    }
    if (values.length === 0) return { low: 0, high: 1 };
    let low = Math.min(...values);
    let high = Math.max(...values);
    const pad = high === low ? Math.abs(high) * 0.1 || 1 : (high - low) * 0.08;
    low -= pad;
    high += pad;
    return { low, high };
  };

  const yFor = (value: number) => {
    const { low, high } = bounds();
    const span = high - low || 1;
    return top + plotHeight - ((value - low) / span) * plotHeight;
  };

  const barRects = () => {
    const labels = data().labels;
    const series = data().series;
    if (labels.length === 0 || series.length === 0) return [];
    const slot = plotWidth / labels.length;
    const barWidth = Math.max(2, (slot * 0.7) / series.length);
    const baseline = yFor(0);
    const rects: {
      x: number;
      y: number;
      width: number;
      height: number;
      fill: string;
      label: string;
      name: string;
      value: number;
    }[] = [];
    labels.forEach((label, rowIndex) => {
      const groupLeft = left + rowIndex * slot + slot * 0.15;
      series.forEach((line, lineIndex) => {
        const value = data().values[lineIndex]?.[rowIndex] ?? 0;
        const y = yFor(Math.max(0, value));
        rects.push({
          x: groupLeft + lineIndex * barWidth,
          y,
          width: barWidth * 0.9,
          height: Math.max(0, baseline - y),
          fill: line.color,
          label,
          name: line.name,
          value,
        });
      });
    });
    return rects;
  };

  const linePoints = () => {
    const count = data().labels.length;
    return data().series.map((line, lineIndex) => {
      const points = (data().values[lineIndex] ?? [])
        .slice(0, count)
        .map((value, index) => {
          const x =
            count <= 1 ? left + plotWidth / 2 : left + (index / (count - 1)) * plotWidth;
          return { x, y: yFor(value), value, label: data().labels[index] ?? "" };
        });
      return { name: line.name, color: line.color, points };
    });
  };

  const labelTexts = () => {
    const labels = data().labels;
    const slot = labels.length > 0 ? plotWidth / labels.length : plotWidth;
    return labels.map((label, index) => ({
      x: left + index * slot + slot / 2,
      y: top + plotHeight + 18,
      text: truncate(label, 14),
    }));
  };

  return (
    <figure class="ui-chart-figure">
      <svg
        class="ui-chart-svg"
        viewBox={`0 0 ${WIDTH} ${data().height}`}
        preserveAspectRatio="xMidYMid meet"
        aria-hidden="true"
      >
        <For each={[0.25, 0.5, 0.75]}>
          {(fraction) => (
            <line
              x1={left}
              y1={top + plotHeight * (1 - fraction)}
              x2={WIDTH - left}
              y2={top + plotHeight * (1 - fraction)}
              class="chart-grid"
            />
          )}
        </For>
        <line
          x1={left}
          y1={top + plotHeight}
          x2={WIDTH - left}
          y2={top + plotHeight}
          class="chart-baseline"
        />
        <text x={left} y={top + 8} class="chart-axis-label">
          {formatValue(bounds().high)}
        </text>

        <Show when={data().kind === "bar"}>
          <For each={barRects()}>
            {(rect) => (
              <rect
                x={rect.x}
                y={rect.y}
                width={rect.width}
                height={Math.max(0, rect.height)}
                rx="3"
                fill={rect.fill}
              >
                <title>{`${rect.label} · ${rect.name}: ${formatValue(rect.value)}`}</title>
              </rect>
            )}
          </For>
          <Show when={data().series.length === 1 && data().labels.length <= 12}>
            <For each={barRects()}>
              {(rect) => (
                <text
                  x={rect.x + rect.width / 2}
                  y={Math.max(10, rect.y - 4)}
                  text-anchor="middle"
                  class="chart-value"
                >
                  {formatValue(rect.value)}
                </text>
              )}
            </For>
          </Show>
        </Show>

        <Show when={data().kind === "area"}>
          <For each={linePoints()}>
            {(line) => (
              <polygon
                points={[
                  `${left},${top + plotHeight}`,
                  ...line.points.map(
                    (point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`,
                  ),
                  `${WIDTH - left},${top + plotHeight}`,
                ].join(" ")}
                fill={line.color}
                fill-opacity="0.14"
                stroke="none"
              />
            )}
          </For>
        </Show>

        <Show when={data().kind === "line" || data().kind === "area"}>
          <For each={linePoints()}>
            {(line) => (
              <>
                <polyline
                  points={line.points
                    .map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`)
                    .join(" ")}
                  fill="none"
                  stroke={line.color}
                  stroke-width="2"
                  stroke-linejoin="round"
                  stroke-linecap="round"
                />
                <For each={line.points}>
                  {(point) => (
                    <circle cx={point.x} cy={point.y} r="3.5" fill={line.color} stroke="var(--bg-surface)" stroke-width="2">
                      <title>{`${point.label} · ${line.name}: ${formatValue(point.value)}`}</title>
                    </circle>
                  )}
                </For>
              </>
            )}
          </For>
        </Show>

        <For each={labelTexts()}>
          {(label) => (
            <text x={label.x} y={label.y} text-anchor="middle" class="chart-axis-label">
              {label.text}
            </text>
          )}
        </For>
      </svg>

      <Show when={data().series.length > 1}>
        <div class="ui-legend">
          <For each={data().series}>
            {(series) => (
              <span class="ui-legend-item">
                <span class="ui-legend-swatch" style={{ background: series.color }} />
                {series.name}
              </span>
            )}
          </For>
        </div>
      </Show>

      <Show when={data().caption}>
        <figcaption>{data().caption}</figcaption>
      </Show>

      <table class="sr-only">
        <caption>{data().caption || data().labelHeader}</caption>
        <thead>
          <tr>
            <th scope="col">{data().labelHeader}</th>
            <For each={data().series}>{(series) => <th scope="col">{series.name}</th>}</For>
          </tr>
        </thead>
        <tbody>
          <For each={data().labels}>
            {(label, index) => (
              <tr>
                <th scope="row">{label}</th>
                <For each={data().values}>
                  {(values) => <td>{formatValue(values[index()] ?? 0)}</td>}
                </For>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </figure>
  );
}
