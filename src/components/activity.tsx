// Activity rows for the transcript: thinking, tool calls, unpaired tool
// results, and the compressed summary of consecutive low-noise calls.
//
// The summary mirrors the reference project: a run of reasoning plus read,
// search and list calls becomes one line such as
// "Thought for 49s, searched for 3 patterns, read 6 files, and listed 2
// directories". Writes, failures and unknown commands stay as individual rows.

import { For, Show, createMemo, createSignal, type JSX } from "solid-js";
import { formatList, sentenceCase, thoughtTitle } from "../format";
import { t, tArgs, tPlural } from "../i18n";
import { IconChevron, IconSpinner } from "./Icons";
import { CopyButton, Markdown } from "./Markdown";
import { analyzeToolActivity, type ToolActivityKind } from "./toolActivity";

export type Rail = "none" | "start" | "middle" | "end" | "only";

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

function activityPhrase(kind: ToolActivityKind, count: number, live: boolean): string {
  switch (kind) {
    case "read":
      return live
        ? tPlural("Reading {n} file", "Reading {n} files", count)
        : tPlural("Read {n} file", "Read {n} files", count);
    case "search":
      return live
        ? tPlural("Searching for {n} pattern", "Searching for {n} patterns", count)
        : tPlural("Searched for {n} pattern", "Searched for {n} patterns", count);
    case "list":
      return live
        ? tPlural("Listing {n} directory", "Listing {n} directories", count)
        : tPlural("Listed {n} directory", "Listed {n} directories", count);
  }
}

/** Title and status for a compressed activity group. */
function summarizeGroup(
  items: readonly ActivityItemRow[],
  resultFor: (toolUseId: string) => ToolResultInfo | undefined,
  runActive: boolean,
): { title: string; running: boolean } {
  let thoughtMs = 0;
  let hasThought = false;
  let running = false;
  const counts: Record<ToolActivityKind, { count: number; live: boolean }> = {
    read: { count: 0, live: false },
    search: { count: 0, live: false },
    list: { count: 0, live: false },
  };

  for (const item of items) {
    if (item.kind === "thinking") {
      hasThought = true;
      thoughtMs += item.durationMs;
      continue;
    }
    if (item.kind !== "tool") continue;
    const result = resultFor(item.toolUseId);
    const live = !result && runActive;
    if (live) running = true;
    const activity = analyzeToolActivity(item.name, item.input) ?? [];
    for (const entry of activity) {
      counts[entry.kind].count += entry.count;
      counts[entry.kind].live ||= live;
    }
  }

  const parts: string[] = [];
  if (hasThought) parts.push(thoughtTitle(thoughtMs, false));
  for (const kind of ["read", "search", "list"] as ToolActivityKind[]) {
    const entry = counts[kind];
    if (entry.count > 0) parts.push(activityPhrase(kind, entry.count, entry.live));
  }

  const title = parts.length > 0 ? sentenceCase(formatList(parts)) : t("Tool calls");
  return { title, running };
}

export function ActivityRow(props: {
  rowKey: string;
  rail: () => Rail;
  tone: "ok" | "danger" | "muted" | "live";
  spinning?: boolean;
  label: string;
  badge?: string;
  detail: JSX.Element;
  nested?: boolean;
}) {
  const [open, setOpen] = createSignal(disclosureOpen.get(props.rowKey) ?? false);
  // Keep a never-opened detail out of the DOM; once opened it stays mounted so
  // the close transition has content to animate.
  const [mounted, setMounted] = createSignal(open());

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
      data-rail={props.nested ? "none" : props.rail()}
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
  rail: () => Rail;
  nested?: boolean;
  onOpenUrl?: (url: string) => void;
}) {
  return (
    <ActivityRow
      rowKey={props.row.key}
      rail={props.rail}
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
  rail: () => Rail;
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
      rail={props.rail}
      tone={tone()}
      spinning={status() === "running"}
      label={toolTitle(props.row.name)}
      badge={status() === "failure" ? t("Failed") : undefined}
      nested={props.nested}
      detail={detail()}
    />
  );
}

export function ResultRowView(props: { row: ResultRowData; rail: () => Rail; nested?: boolean }) {
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
      rail={props.rail}
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
  rail: () => Rail;
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
      rail={props.rail}
      tone={summary().running ? "live" : "ok"}
      spinning={summary().running}
      label={summary().title}
      detail={
        <div class="activity-group-detail">
          <For each={props.row.items}>
            {(item) => (
              <ActivityItemView
                item={item}
                rail={() => "none"}
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
  rail: () => Rail;
  resultFor: (toolUseId: string) => ToolResultInfo | undefined;
  runActive: () => boolean;
  nested?: boolean;
  onOpenUrl?: (url: string) => void;
}) {
  const item = props.item;
  switch (item.kind) {
    case "thinking":
      return (
        <ThinkingRowView
          row={item}
          rail={props.rail}
          nested={props.nested}
          onOpenUrl={props.onOpenUrl}
        />
      );
    case "tool":
      return (
        <ToolRowView
          row={item}
          rail={props.rail}
          result={() => props.resultFor(item.toolUseId)}
          runActive={props.runActive}
          nested={props.nested}
        />
      );
    case "result":
      return <ResultRowView row={item} rail={props.rail} nested={props.nested} />;
  }
}

/** Compressible means: reasoning, or a read/search/list call that succeeded. */
export function isCompressibleActivity(
  item: ActivityItemRow,
  resultFor: (toolUseId: string) => ToolResultInfo | undefined,
): boolean {
  if (item.kind === "thinking") return true;
  if (item.kind !== "tool") return false;
  if (resultFor(item.toolUseId)?.isError) return false;
  return analyzeToolActivity(item.name, item.input) !== null;
}
