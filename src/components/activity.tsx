// Activity rows for the transcript: thinking, tool calls, unpaired tool
// results, and the one-line summary of a consecutive activity run.
//
// The summary counts each step, names the live step while a run is active, and
// opens itself when a step failed. The full timeline stays available behind the
// disclosure, so failures are visible without keeping every successful step.

import { For, Show, createEffect, createMemo, createSignal, type JSX } from "solid-js";
import { thoughtTitle } from "../format";
import { t, tArgs } from "../i18n";
import { IconChevron, IconSpinner } from "./Icons";
import { CopyButton, Markdown } from "./Markdown";

export type ToolResultInfo = {
  content: string;
  isError: boolean;
};

export type ThinkingRowData = {
  key: string;
  kind: "thinking";
  text: string;
  durationMs: number;
};

export type ToolRowData = {
  key: string;
  kind: "tool";
  toolUseId: string;
  name: string;
  input: unknown;
};

export type ResultRowData = {
  key: string;
  kind: "result";
  toolUseId: string;
  name: string | null;
  content: string;
  isError: boolean;
};

export type ActivityItemRow = ThinkingRowData | ToolRowData | ResultRowData;

export type GroupRowData = {
  key: string;
  kind: "group";
  items: ActivityItemRow[];
};

export type ActivityRowData = ActivityItemRow | GroupRowData;

const disclosureOpen = new Map<string, boolean>();

function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}

export function toolTitle(name: string): string {
  switch (name) {
    case "fs_read":
      return t("Read file");
    case "fs_list":
      return t("List directory");
    case "fs_write":
      return t("Write file");
    case "shell_exec":
      return t("Run command");
    case "memory_search":
      return t("Search memory");
    case "memory_write":
      return t("Write memory");
    case "delegate":
      return t("Delegate task");
    default:
      return tArgs("Call {name}", { name });
  }
}

type GroupSummary = {
  title: string;
  running: boolean;
  failed: number;
};

/** One line for a whole run of thinking, tool calls and failures. */
function summarizeGroup(
  items: readonly ActivityItemRow[],
  resultFor: (toolUseId: string) => ToolResultInfo | undefined,
  runActive: boolean,
): GroupSummary {
  let thoughtMs = 0;
  let hasThought = false;
  let running = false;
  let failed = 0;
  let liveTitle = "";
  const counts = new Map<string, number>();

  const addCount = (label: string) => {
    counts.set(label, (counts.get(label) ?? 0) + 1);
  };

  for (const item of items) {
    if (item.kind === "thinking") {
      hasThought = true;
      thoughtMs += item.durationMs;
      continue;
    }
    if (item.kind === "result") {
      addCount(t("Tool result"));
      if (item.isError) failed += 1;
      continue;
    }
    if (item.kind !== "tool") continue;

    const label = toolTitle(item.name);
    addCount(label);
    const result = resultFor(item.toolUseId);
    if (result?.isError) failed += 1;
    if (!result && runActive) {
      running = true;
      liveTitle = label;
    }
  }

  if (running) {
    return { title: liveTitle || t("Thinking"), running: true, failed };
  }

  const parts: string[] = [];
  if (hasThought) parts.push(thoughtTitle(thoughtMs, false));
  for (const [label, count] of counts) {
    parts.push(tArgs("{label} ×{n}", { label, n: count }));
  }

  return {
    title: parts.length > 0 ? parts.join(" · ") : t("Tool calls"),
    running: false,
    failed,
  };
}

export function ActivityRow(props: {
  rowKey: string;
  tone: "ok" | "danger" | "muted" | "live";
  spinning?: boolean;
  label: string;
  badge?: string;
  /** Open the row when it first appears, unless the user has already chosen. */
  defaultOpen?: boolean;
  detail: JSX.Element;
  nested?: boolean;
}) {
  const [open, setOpen] = createSignal(
    disclosureOpen.get(props.rowKey) ?? props.defaultOpen ?? false,
  );
  // Keep a never-opened detail out of the DOM; once opened it stays mounted so
  // the close transition has content to animate.
  const [mounted, setMounted] = createSignal(open());

  // A failure that appears while the run is still going should surface itself,
  // but never override a choice the user already made for this group.
  createEffect(() => {
    if (!props.defaultOpen || disclosureOpen.has(props.rowKey) || open()) return;
    setMounted(true);
    setOpen(true);
  });

  function toggle() {
    const next = !open();
    if (next) setMounted(true);
    setOpen(next);
    disclosureOpen.set(props.rowKey, next);
  }

  return (
    <div
      class="activity-row"
      classList={{ "activity-row--open": open(), "activity-row--nested": props.nested }}
    >
      <button type="button" class="activity-trigger" aria-expanded={open()} onClick={toggle}>
        <span class="activity-status" data-tone={props.tone} aria-hidden="true">
          <Show when={props.spinning} fallback={<span class="status-dot" data-tone={props.tone} />}>
            <IconSpinner size={14} class="spin" />
          </Show>
        </span>
        <span class="activity-label">{props.label}</span>
        <Show when={props.badge}>
          <span class="activity-badge" data-tone={props.tone}>
            {props.badge}
          </span>
        </Show>
        <span class="activity-chevron" classList={{ open: open() }}>
          <IconChevron size={13} />
        </span>
      </button>
      <div class="activity-detail-wrap" classList={{ open: open() }} aria-hidden={!open()}>
        <div class="activity-detail-clip">
          <Show when={mounted()}>
            <div class="activity-detail">{props.detail}</div>
          </Show>
        </div>
      </div>
    </div>
  );
}

export function ThinkingRowView(props: {
  row: ThinkingRowData;
  nested?: boolean;
  onOpenUrl?: (url: string) => void;
}) {
  return (
    <ActivityRow
      rowKey={props.row.key}
      tone="muted"
      label={thoughtTitle(props.row.durationMs, false)}
      nested={props.nested}
      detail={
        <div class="thought-body">
          <Markdown text={props.row.text || t("(empty)")} onOpenUrl={props.onOpenUrl} />
        </div>
      }
    />
  );
}

export function ToolRowView(props: {
  row: ToolRowData;
  result: () => ToolResultInfo | undefined;
  runActive: () => boolean;
  nested?: boolean;
}) {
  const status = () => {
    const result = props.result();
    if (result) return result.isError ? "failure" : "success";
    return props.runActive() ? "running" : "muted";
  };
  const tone = () => {
    switch (status()) {
      case "failure":
        return "danger" as const;
      case "success":
        return "ok" as const;
      case "running":
        return "live" as const;
      default:
        return "muted" as const;
    }
  };

  const detail = () => (
    <div class="tool-detail">
      <div class="detail-section">
        <span class="detail-label">{t("Input")}</span>
        <pre class="detail-pre">{prettyJson(props.row.input)}</pre>
      </div>
      <Show
        when={props.result()}
        fallback={
          <span class="detail-note">
            {props.runActive() ? t("Running…") : t("No result recorded.")}
          </span>
        }
      >
        {(result) => (
          <div class="detail-section">
            <div class="detail-head">
              <span class="detail-label">{result().isError ? t("Error") : t("Output")}</span>
              <CopyButton text={result().content} class="detail-copy" />
            </div>
            <pre classList={{ "detail-pre": true, error: result().isError }}>
              {result().content}
            </pre>
          </div>
        )}
      </Show>
    </div>
  );

  return (
    <ActivityRow
      rowKey={props.row.key}
      tone={tone()}
      spinning={status() === "running"}
      label={toolTitle(props.row.name)}
      badge={status() === "failure" ? t("Failed") : undefined}
      nested={props.nested}
      detail={detail()}
    />
  );
}

export function ResultRowView(props: { row: ResultRowData; nested?: boolean }) {
  const label = () => {
    const name = props.row.name;
    if (props.row.isError) {
      return name ? tArgs("Tool failed: {name}", { name }) : t("Tool failed");
    }
    return name ? tArgs("Tool result: {name}", { name }) : t("Tool result");
  };

  return (
    <ActivityRow
      rowKey={props.row.key}
      tone={props.row.isError ? "danger" : "ok"}
      label={label()}
      nested={props.nested}
      detail={
        <div class="tool-detail">
          <div class="detail-section">
            <div class="detail-head">
              <span class="detail-label">{props.row.isError ? t("Error") : t("Output")}</span>
              <CopyButton text={props.row.content} class="detail-copy" />
            </div>
            <pre classList={{ "detail-pre": true, error: props.row.isError }}>
              {props.row.content}
            </pre>
          </div>
        </div>
      }
    />
  );
}

export function GroupRowView(props: {
  row: GroupRowData;
  resultFor: (toolUseId: string) => ToolResultInfo | undefined;
  runActive: () => boolean;
  onOpenUrl?: (url: string) => void;
}) {
  const summary = createMemo(() =>
    summarizeGroup(props.row.items, props.resultFor, props.runActive()),
  );

  return (
    <ActivityRow
      rowKey={props.row.key}
      tone={summary().failed > 0 ? "danger" : summary().running ? "live" : "ok"}
      spinning={summary().running}
      label={summary().title}
      badge={
        summary().failed > 0
          ? tArgs("{n} failed", { n: summary().failed })
          : summary().running
            ? t("Running…")
            : undefined
      }
      defaultOpen={summary().failed > 0}
      detail={
        <div class="activity-group-detail">
          <For each={props.row.items}>
            {(item) => (
              <ActivityItemView
                item={item}
                resultFor={props.resultFor}
                runActive={props.runActive}
                nested
                onOpenUrl={props.onOpenUrl}
              />
            )}
          </For>
        </div>
      }
    />
  );
}

export function ActivityItemView(props: {
  item: ActivityItemRow;
  resultFor: (toolUseId: string) => ToolResultInfo | undefined;
  runActive: () => boolean;
  nested?: boolean;
  onOpenUrl?: (url: string) => void;
}) {
  const item = props.item;
  switch (item.kind) {
    case "thinking":
      return <ThinkingRowView row={item} nested={props.nested} onOpenUrl={props.onOpenUrl} />;
    case "tool":
      return (
        <ToolRowView
          row={item}
          result={() => props.resultFor(item.toolUseId)}
          runActive={props.runActive}
          nested={props.nested}
        />
      );
    case "result":
      return <ResultRowView row={item} nested={props.nested} />;
  }
}

/** A run of activity becomes one summary row. */
export function isCompressibleActivity(item: ActivityItemRow): boolean {
  // A run of thinking and tool steps is one unit, regardless of tool kind or
  // outcome. Failures stay inside the group so its summary can count them and
  // open itself; hiding a failure behind an untouched group would be worse.
  return item.kind === "thinking" || item.kind === "tool" || item.kind === "result";
}
