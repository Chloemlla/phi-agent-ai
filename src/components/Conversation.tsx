// Conversation transcript: a document flow of user bubbles, agent prose and
// compact activity rows. Consecutive reasoning/read/search/list calls compress
// into one summary; writes and failures stay individual. A thin rail connects
// an activity cluster without boxing each row.

import { Index, Show, createEffect, createMemo, createSignal, type Accessor } from "solid-js";
import { formatFullTime, lastAssistantText, streamMatchesDurable } from "../format";
import { t, tArgs } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Block, Message, PendingApproval, Run, SessionDetail } from "../types";
import { ApprovalCard } from "./ApprovalCard";
import { Alt } from "./Alt";
import {
  GroupRowView,
  ResultRowView,
  ThinkingRowView,
  ToolRowView,
  isCompressibleActivity,
  type ActivityItemRow,
  type GroupRowData,
  type Rail,
  type ResultRowData,
  type ThinkingRowData,
  type ToolResultInfo,
  type ToolRowData,
} from "./activity";
import { IconAlert, IconPhi } from "./Icons";
import { CopyButton, Markdown } from "./Markdown";
import { RichHtml } from "./RichHtml";
import { createCoalesced, createInlineUiSplitter, type InlineUiSegment } from "./inlineUi";
import { SideScrollbar } from "./SideScrollbar";
import { VIRTUALIZE_THRESHOLD, VirtualTranscript } from "./VirtualTranscript";

type TextRowData = { key: string; kind: "text"; text: string; ts: string };
type RedactedRowData = { key: string; kind: "redacted" };
type UserRowData = { key: string; kind: "user"; message: Message };
type StreamRowData = { key: string; kind: "stream" };
type ApprovalRowData = {
  key: string;
  kind: "approval";
  approval: Accessor<PendingApproval>;
  update: (approval: PendingApproval) => void;
};
type RunRowData = { key: string; kind: "run" };

type ActivityRow = ActivityItemRow | GroupRowData;

type TranscriptRow =
  | UserRowData
  | TextRowData
  | RedactedRowData
  | ActivityRow
  | StreamRowData
  | ApprovalRowData
  | RunRowData;

const STREAM_ROW: StreamRowData = { key: "stream", kind: "stream" };
const RUN_ROW: RunRowData = { key: "run", kind: "run" };
const userRows = new WeakMap<Message, UserRowData>();
const blockRows = new WeakMap<
  Block,
  TextRowData | ThinkingRowData | ToolRowData | ResultRowData | RedactedRowData
>();
const groupCache = new Map<string, GroupRowData>();
const approvalRows = new Map<string, ApprovalRowData>();

function userRow(message: Message): UserRowData {
  let row = userRows.get(message);
  if (!row) {
    row = { key: `user:${message.id}`, kind: "user", message };
    userRows.set(message, row);
  }
  return row;
}

function textRow(
  message: Message,
  block: Extract<Block, { type: "text" }>,
  index: number,
): TextRowData {
  let row = blockRows.get(block) as TextRowData | undefined;
  if (!row) {
    row = {
      key: `text:${message.id}:${index}`,
      kind: "text",
      text: block.text,
      ts: message.ts,
    };
    blockRows.set(block, row);
  }
  return row;
}

function thinkingRow(
  message: Message,
  block: Extract<Block, { type: "thinking" }>,
  index: number,
  durationMs: number,
): ThinkingRowData {
  let row = blockRows.get(block) as ThinkingRowData | undefined;
  if (!row) {
    row = {
      key: `thinking:${message.id}:${index}`,
      kind: "thinking",
      text: block.thinking,
      durationMs,
    };
    blockRows.set(block, row);
  }
  return row;
}

function toolRow(block: Extract<Block, { type: "tool_use" }>): ToolRowData {
  let row = blockRows.get(block) as ToolRowData | undefined;
  if (!row) {
    row = {
      key: `tool:${block.id}`,
      kind: "tool",
      toolUseId: block.id,
      name: block.name,
      input: block.input,
    };
    blockRows.set(block, row);
  }
  return row;
}

function resultRow(
  message: Message,
  block: Extract<Block, { type: "tool_result" }>,
): ResultRowData {
  let row = blockRows.get(block) as ResultRowData | undefined;
  if (!row) {
    row = {
      key: `result:${message.id}:${block.tool_use_id}`,
      kind: "result",
      toolUseId: block.tool_use_id,
      name: null,
      content: block.content,
      isError: block.is_error,
    };
    blockRows.set(block, row);
  }
  return row;
}

function redactedRow(
  message: Message,
  block: Extract<Block, { type: "redacted_thinking" }>,
  index: number,
): RedactedRowData {
  let row = blockRows.get(block) as RedactedRowData | undefined;
  if (!row) {
    row = {
      key: `redacted:${message.id}:${index}`,
      kind: "redacted",
    };
    blockRows.set(block, row);
  }
  return row;
}

function isActivityItemRow(row: TranscriptRow): row is ActivityItemRow {
  return row.kind === "thinking" || row.kind === "tool" || row.kind === "result";
}

function isActivityRow(row: TranscriptRow): row is ActivityRow {
  return isActivityItemRow(row) || row.kind === "group";
}

// Approval objects are rebuilt by every projection refresh; keep one row and
// update its accessor so the card keeps local state (busy buttons, focus).
function approvalRow(approval: PendingApproval): ApprovalRowData {
  const existing = approvalRows.get(approval.id);
  if (existing) {
    existing.update(approval);
    return existing;
  }
  const [value, setValue] = createSignal(approval);
  const row: ApprovalRowData = {
    key: `approval:${approval.id}`,
    kind: "approval",
    approval: value,
    update: (next) => setValue(next),
  };
  approvalRows.set(approval.id, row);
  return row;
}

function estimateRow(row: TranscriptRow): number {
  switch (row.kind) {
    case "user":
      return 58;
    case "text":
      return 96;
    case "redacted":
      return 40;
    case "thinking":
    case "tool":
    case "result":
    case "group":
      return 34;
    case "stream":
      return 72;
    case "approval":
      return 180;
    case "run":
      return 96;
  }
}

function userText(message: Message): string {
  return message.blocks
    .filter((block) => block.type === "text" && block.text.trim())
    .map((block) => (block.type === "text" ? block.text : ""))
    .join("\n\n");
}

function MessageDot(props: { ts: string }) {
  const label = () => formatFullTime(props.ts);
  return (
    <Alt content={<span class="alt-time">{label()}</span>} class="alt-bubble--time" delay={150}>
      <span class="message-dot-hit" tabindex="0" aria-label={label()}>
        <span class="message-dot" aria-hidden="true" />
      </span>
    </Alt>
  );
}

function UserMessageRow(props: { row: UserRowData }) {
  return (
    <div class="message message--user">
      <div class="user-bubble" title={formatFullTime(props.row.message.ts)}>
        {userText(props.row.message)}
      </div>
    </div>
  );
}

function AssistantTextRow(props: {
  row: TextRowData;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  return (
    <div class="message message--assistant">
      <MessageDot ts={props.row.ts} />
      <div class="message-body">
        <Markdown text={props.row.text} onOpenUrl={props.onOpenUrl} onSend={props.onSend} />
      </div>
    </div>
  );
}

function RedactedRow() {
  return (
    <div class="message message--assistant message--notice">
      <span class="message-dot static" aria-hidden="true" />
      <div class="message-body notice-text">{t("Reasoning hidden by the provider")}</div>
    </div>
  );
}

function segmentText(segment: InlineUiSegment): string {
  return segment.kind === "text" ? segment.text : "";
}

function segmentHtml(segment: InlineUiSegment): string {
  return segment.kind === "html" ? segment.html : "";
}

function StreamingMessage(props: {
  segments: Accessor<InlineUiSegment[]>;
  ended: boolean;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  return (
    <article class="message message--assistant message--streaming">
      <span class="message-dot live" aria-hidden="true" />
      <div class="message-body">
        {/* Plain text while streaming: reparsing Markdown on every token
            recreates blocks and loses code-copy state. An `html` fence is
            rendered live because its content is self-contained, then the
            durable message renders the whole reply once. Index keeps existing
            segments mounted while the tail grows. */}
        <Index each={props.segments()}>
          {(segment) => (
            <Show
              when={segment().kind === "html"}
              fallback={
                <Show when={segmentText(segment()).trim()}>
                  <div class="streaming-text">{segmentText(segment())}</div>
                </Show>
              }
            >
              <RichHtml
                html={segmentHtml(segment())}
                onOpenUrl={props.onOpenUrl}
                onSend={props.onSend}
              />
            </Show>
          )}
        </Index>
        <Show when={props.ended}>
          <div class="stream-stopped">
            <span class="dot tone-idle" />
            {t("Stopped")}
          </div>
        </Show>
      </div>
    </article>
  );
}

function EmptyState(props: { modelConfigured: boolean }) {
  return (
    <div class="empty-state">
      <div class="empty-mark">
        <IconPhi size={22} />
      </div>
      <h1 class="empty-title">{t("Start a task")}</h1>
      <p class="empty-text">
        {t(
          "Describe what you want the agent to do. It can read your files, edit them and run commands — always with your approval.",
        )}
      </p>
      <div class="empty-examples">
        <code>{t("“Map the repository and explain how the app is structured.”")}</code>
        <code>{t("“Find the failing test and fix it.”")}</code>
      </div>
      <Show when={!props.modelConfigured}>
        <p class="empty-note">
          {t("No provider is configured. Add one in Settings to start a task.")}
        </p>
      </Show>
    </div>
  );
}

function firstLine(text: string | null | undefined): string {
  if (!text) return "";
  const line = text.split(/\r?\n/).find((value) => value.trim());
  return line?.trim() ?? "";
}

const runDetailsOpen = new Map<string, boolean>();

function RunNotice(props: { run: Run }) {
  const danger = () => runTone(props.run.state) === "danger";
  const details = () =>
    [props.run.error, props.run.stopConfirmation]
      .map((value) => value?.trim())
      .filter((value): value is string => !!value)
      .join("\n\n");
  const summary = () =>
    firstLine(props.run.error) ||
    firstLine(props.run.stopConfirmation) ||
    t("No details were recorded.");

  return (
    <section
      classList={{ "run-notice": true, danger: danger() }}
      role={danger() ? "alert" : "status"}
    >
      <div class="run-notice-head">
        <span
          classList={{
            dot: true,
            [`tone-${runTone(props.run.state)}`]: true,
          }}
        />
        <span class="run-notice-title">{stateLabel(props.run.state)}</span>
        <span class="run-notice-summary" title={props.run.error ?? undefined}>
          {summary()}
        </span>
      </div>
      <Show when={details()}>
        <details
          class="run-notice-details"
          open={runDetailsOpen.get(props.run.id) ?? false}
          onToggle={(event) => runDetailsOpen.set(props.run.id, event.currentTarget.open)}
        >
          <summary>{t("Details")}</summary>
          <div class="run-notice-body">
            <CopyButton text={details()} />
            <pre class="mono">{details()}</pre>
          </div>
        </details>
      </Show>
    </section>
  );
}

const FAILED_STATES = ["AwaitingReconciliation", "Failed", "Aborted"];

export function Conversation(props: {
  detail: SessionDetail | null;
  run: Run | null;
  streamText: string;
  streamEnded: boolean;
  modeOk: boolean;
  modeReason: string;
  modelConfigured: boolean;
  now: number;
  onResolve: (approvalId: string, approved: boolean) => Promise<void>;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  const [transcript, setTranscript] = createSignal<HTMLDivElement>();
  const coalescedText = createCoalesced(() => props.streamText, 64);
  const splitter = createInlineUiSplitter();
  const streamSegments = createMemo(() => splitter.update(coalescedText()));

  const failedRun = () => (props.run && FAILED_STATES.includes(props.run.state) ? props.run : null);
  // A tool only counts as running while the model loop is actually executing;
  // WaitingApproval is represented by its approval card, not a spinner.
  const runActive = () => props.run?.state === "Scheduled" || props.run?.state === "Running";

  const toolResults = createMemo(() => {
    const map = new Map<string, ToolResultInfo>();
    for (const message of props.detail?.session.messages ?? []) {
      for (const block of message.blocks) {
        if (block.type === "tool_result") {
          map.set(block.tool_use_id, {
            content: block.content,
            isError: block.is_error,
          });
        }
      }
    }
    return map;
  });

  const durableText = createMemo(() => lastAssistantText(props.detail));
  const durableStream = () => streamMatchesDurable(props.streamText, durableText());
  // Boolean memo: token-rate text changes do not rebuild the row list while
  // the live row stays on screen.
  const streamVisible = createMemo(() => props.streamText.length > 0 && !durableStream());

  const rows = createMemo<TranscriptRow[]>(() => {
    const detail = props.detail;
    if (!detail) return [];
    const messages = detail.session.messages;
    const results = toolResults();

    const toolUseIds = new Set<string>();
    for (const message of messages) {
      for (const block of message.blocks) {
        if (block.type === "tool_use") toolUseIds.add(block.id);
      }
    }

    const raw: TranscriptRow[] = [];
    let previousTs: number | null = null;
    for (const message of messages) {
      const messageTs = Date.parse(message.ts);
      const delta =
        previousTs !== null && Number.isFinite(messageTs) ? Math.max(0, messageTs - previousTs) : 0;
      if (Number.isFinite(messageTs)) previousTs = messageTs;

      if (message.role === "user") {
        if (userText(message)) raw.push(userRow(message));
        for (const block of message.blocks) {
          if (block.type === "tool_result" && !toolUseIds.has(block.tool_use_id)) {
            raw.push(resultRow(message, block));
          }
        }
        continue;
      }

      let durationAssigned = false;
      for (const [index, block] of message.blocks.entries()) {
        switch (block.type) {
          case "text":
            if (block.text.trim()) raw.push(textRow(message, block, index));
            break;
          case "thinking": {
            const durationMs = durationAssigned ? 0 : delta;
            durationAssigned = true;
            if (block.thinking.trim()) {
              raw.push(thinkingRow(message, block, index, durationMs));
            }
            break;
          }
          case "redacted_thinking":
            raw.push(redactedRow(message, block, index));
            break;
          case "tool_use":
            raw.push(toolRow(block));
            break;
          case "tool_result":
            if (!toolUseIds.has(block.tool_use_id)) {
              raw.push(resultRow(message, block));
            }
            break;
        }
      }
    }

    // Compress consecutive low-noise activity into one summary row. The cache
    // keeps a group's identity while its members are unchanged so an open
    // group keeps its DOM (and scroll position) across projection refreshes.
    const compressed: TranscriptRow[] = [];
    const resultFor = (toolUseId: string) => results.get(toolUseId);
    let index = 0;
    while (index < raw.length) {
      const row = raw[index];
      if (!isActivityItemRow(row) || !isCompressibleActivity(row, resultFor)) {
        compressed.push(row);
        index += 1;
        continue;
      }
      let end = index + 1;
      while (end < raw.length) {
        const next = raw[end];
        if (!isActivityItemRow(next) || !isCompressibleActivity(next, resultFor)) break;
        end += 1;
      }
      const items = raw.slice(index, end) as ActivityItemRow[];
      if (items.length >= 2) {
        const key = `group:${items.map((item) => item.key).join("|")}`;
        let group = groupCache.get(key);
        if (!group) {
          group = { key, kind: "group", items };
          groupCache.set(key, group);
        }
        compressed.push(group);
      } else {
        compressed.push(row);
      }
      index = end;
    }

    const usedGroups = new Set(
      compressed.filter((row) => row.kind === "group").map((row) => row.key),
    );
    for (const key of groupCache.keys()) {
      if (!usedGroups.has(key)) groupCache.delete(key);
    }

    if (streamVisible()) compressed.push(STREAM_ROW);
    const liveApprovals = new Set<string>();
    for (const approval of detail.pendingApprovals) {
      liveApprovals.add(approval.id);
      compressed.push(approvalRow(approval));
    }
    for (const id of approvalRows.keys()) {
      if (!liveApprovals.has(id)) approvalRows.delete(id);
    }
    if (failedRun()) compressed.push(RUN_ROW);
    return compressed;
  });

  // The rail joins consecutive activity rows. A single row draws no line;
  // the first and last rows stop at the status dot so the line never dangles.
  const rails = createMemo(() => {
    const map = new Map<string, Rail>();
    const list = rows();
    let index = 0;
    while (index < list.length) {
      if (!isActivityRow(list[index])) {
        index += 1;
        continue;
      }
      let end = index;
      while (end < list.length && isActivityRow(list[end])) end += 1;
      const length = end - index;
      for (let cursor = index; cursor < end; cursor += 1) {
        const row = list[cursor];
        const rail: Rail =
          length === 1
            ? "only"
            : cursor === index
              ? "start"
              : cursor === end - 1
                ? "end"
                : "middle";
        map.set(row.key, rail);
      }
      index = end;
    }
    return map;
  });

  // Pinning follows the coalesced live text, which is also the DOM cadence.
  const revision = () => `${coalescedText().length}:${props.streamEnded ? 1 : 0}`;

  function renderRow(row: TranscriptRow) {
    switch (row.kind) {
      case "user":
        return <UserMessageRow row={row} />;
      case "text":
        return <AssistantTextRow row={row} onOpenUrl={props.onOpenUrl} onSend={props.onSend} />;
      case "redacted":
        return <RedactedRow />;
      case "thinking":
        return (
          <ThinkingRowView
            row={row}
            rail={() => rails().get(row.key) ?? "none"}
            onOpenUrl={props.onOpenUrl}
          />
        );
      case "tool":
        return (
          <ToolRowView
            row={row}
            rail={() => rails().get(row.key) ?? "none"}
            result={() => toolResults().get(row.toolUseId)}
            runActive={runActive}
          />
        );
      case "result":
        return <ResultRowView row={row} rail={() => rails().get(row.key) ?? "none"} />;
      case "group":
        return (
          <GroupRowView
            row={row}
            rail={() => rails().get(row.key) ?? "none"}
            resultFor={(toolUseId) => toolResults().get(toolUseId)}
            runActive={runActive}
            onOpenUrl={props.onOpenUrl}
          />
        );
      case "stream":
        return (
          <StreamingMessage
            segments={streamSegments}
            ended={props.streamEnded}
            onOpenUrl={props.onOpenUrl}
            onSend={props.onSend}
          />
        );
      case "approval":
        return (
          <ApprovalCard
            approval={row.approval()}
            now={props.now}
            onResolve={(approved) => props.onResolve(row.approval().id, approved)}
          />
        );
      case "run":
        return <Show when={failedRun()}>{(run) => <RunNotice run={run()} />}</Show>;
    }
  }

  // Announce pending approvals once per count change; the transcript itself is
  // not a live region.
  const approvalCount = createMemo(() => props.detail?.pendingApprovals.length ?? 0);
  const [approvalNotice, setApprovalNotice] = createSignal("");
  createEffect(() => {
    const count = approvalCount();
    setApprovalNotice(count > 0 ? tArgs("Approval required ({count})", { count }) : "");
  });

  return (
    <div class="scroll-shell">
      <div
        class="transcript"
        tabindex={rows().length > VIRTUALIZE_THRESHOLD ? 0 : undefined}
        ref={(element) => setTranscript(element)}
      >
        <div class="conversation-inner">
          <div class="sr-only" role="status" aria-live="polite" aria-atomic="true">
            {approvalNotice()}
          </div>
          <Show when={!props.modeOk}>
            <div class="banner warn" role="status">
              <IconAlert size={15} />
              <div>
                {props.modeReason || t("The node is not in normal mode; sending is disabled")}
              </div>
            </div>
          </Show>

          <Show when={props.detail} fallback={<div class="loading">{t("Loading…")}</div>}>
            {(detail) => (
              <>
                <Show
                  when={
                    detail().session.messages.length === 0 &&
                    !props.streamText &&
                    detail().pendingApprovals.length === 0 &&
                    !(props.run && isActiveState(props.run.state))
                  }
                >
                  <EmptyState modelConfigured={props.modelConfigured} />
                </Show>

                <Show when={rows().length > 0}>
                  <VirtualTranscript
                    rows={rows()}
                    scroll={() => transcript()}
                    estimate={estimateRow}
                    render={renderRow}
                    revision={revision}
                  />
                </Show>
              </>
            )}
          </Show>
        </div>
      </div>
      <SideScrollbar host={() => transcript() ?? null} />
    </div>
  );
}
