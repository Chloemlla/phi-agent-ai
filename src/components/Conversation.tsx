// Conversation transcript: document flow with role rows, collapsible tool
// blocks and risk-graded approval cards. No left/right bubbles.

import { For, Index, Show, createEffect, createMemo, createSignal, type Accessor } from "solid-js";
import { formatClock, lastAssistantText, streamMatchesDurable } from "../format";
import { t, tArgs } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Block, Message, PendingApproval, Run, SessionDetail } from "../types";
import { ApprovalCard } from "./ApprovalCard";
import {
  IconAlert,
  IconBrain,
  IconCheck,
  IconChevron,
  IconClose,
  IconPhi,
  IconWrench,
} from "./Icons";
import { CopyButton, Markdown } from "./Markdown";
import { RichHtml } from "./RichHtml";
import { createCoalesced, createInlineUiSplitter, type InlineUiSegment } from "./inlineUi";
import { VIRTUALIZE_THRESHOLD, VirtualTranscript } from "./VirtualTranscript";

// Native <details> open state lives in the DOM. Windowed rows are unmounted
// when they leave the viewport, so remember the user's choice by block (the
// durable block objects are reused across refreshes). The initial `open`
// expression is deliberately non-reactive: it restores the choice on mount
// without fighting the browser's own toggling afterwards.
const blockOpen = new WeakMap<Block, boolean>();
const runDetailsOpen = new Map<string, boolean>();

function rememberBlockOpen(block: Block, element: HTMLDetailsElement) {
  blockOpen.set(block, element.open);
}

function ToolBlock(props: {
  block: Block;
  assistant: boolean;
  toolName?: string;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  switch (props.block.type) {
    case "text": {
      const block = props.block;
      return props.assistant ? (
        <Markdown text={block.text} onOpenUrl={props.onOpenUrl} onSend={props.onSend} />
      ) : (
        <div class="text">{block.text}</div>
      );
    }
    case "thinking": {
      const block = props.block;
      return (
        <details
          class="thinking-block"
          open={blockOpen.get(block) ?? false}
          onToggle={(event) => rememberBlockOpen(block, event.currentTarget)}
        >
          <summary>
            <span class="thinking-glyph">
              <IconBrain size={13} />
            </span>
            <span class="thinking-label">{t("Thinking")}</span>
            <span class="chevron">
              <IconChevron size={13} />
            </span>
          </summary>
          <div class="thinking-body">
            <Markdown text={block.thinking || t("(empty)")} onOpenUrl={props.onOpenUrl} />
          </div>
        </details>
      );
    }
    case "redacted_thinking":
      return (
        <div class="thinking-block redacted">
          <span class="thinking-glyph">
            <IconBrain size={13} />
          </span>
          <span class="thinking-label">{t("Reasoning hidden by the provider")}</span>
        </div>
      );
    case "tool_use": {
      const block = props.block;
      return (
        <details
          class="tool"
          open={blockOpen.get(block) ?? false}
          onToggle={(event) => rememberBlockOpen(block, event.currentTarget)}
        >
          <summary>
            <span class="tool-glyph ok">
              <IconWrench size={13} />
            </span>
            <span class="tool-name mono">{tArgs("Call {name}", { name: block.name })}</span>
            <span class="chevron">
              <IconChevron size={13} />
            </span>
          </summary>
          <pre class="mono">{JSON.stringify(block.input, null, 2)}</pre>
        </details>
      );
    }
    case "tool_result": {
      const block = props.block;
      const label = () => {
        if (block.is_error) {
          return props.toolName
            ? tArgs("Tool failed: {name}", { name: props.toolName })
            : t("Tool failed");
        }
        return props.toolName
          ? tArgs("Tool result: {name}", { name: props.toolName })
          : t("Tool result");
      };
      return (
        <details
          class="tool"
          open={blockOpen.get(block) ?? block.is_error}
          onToggle={(event) => rememberBlockOpen(block, event.currentTarget)}
        >
          <summary>
            <span
              classList={{
                "tool-glyph": true,
                error: block.is_error,
                ok: !block.is_error,
              }}
            >
              <Show when={block.is_error} fallback={<IconCheck size={13} />}>
                <IconClose size={12} />
              </Show>
            </span>
            <span class="tool-name mono">{label()}</span>
            <span class="chevron">
              <IconChevron size={13} />
            </span>
          </summary>
          <Show when={block.is_error}>
            <div class="tool-actions">
              <CopyButton text={block.content} />
            </div>
          </Show>
          <pre classList={{ mono: true, error: block.is_error }}>{block.content}</pre>
        </details>
      );
    }
  }
}

function MessageRow(props: {
  message: Message;
  toolNames: Map<string, string>;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  const user = () => props.message.role === "user";
  // The backend journals tool results as user-role messages. They are tool
  // output, not something the human typed, so they get no "You" header.
  const toolOnly = () =>
    user() &&
    props.message.blocks.length > 0 &&
    props.message.blocks.every((block) => block.type === "tool_result");

  return (
    <article
      classList={{
        msg: true,
        user: user() && !toolOnly(),
        assistant: !user(),
        "tool-only": toolOnly(),
      }}
    >
      <Show when={!toolOnly()}>
        <header class="msg-head">
          <span classList={{ avatar: true, user: user(), agent: !user() }}>
            <Show when={!user()} fallback={<span>{t("You").slice(0, 1)}</span>}>
              <IconPhi size={13} />
            </Show>
          </span>
          <span class="msg-name">{user() ? t("You") : t("Agent")}</span>
          <time class="msg-time mono">{formatClock(props.message.ts)}</time>
        </header>
      </Show>
      <div class="msg-body">
        <For each={props.message.blocks}>
          {(block) => (
            <ToolBlock
              block={block}
              assistant={!user()}
              toolName={
                block.type === "tool_result" ? props.toolNames.get(block.tool_use_id) : undefined
              }
              onOpenUrl={props.onOpenUrl}
              onSend={props.onSend}
            />
          )}
        </For>
      </div>
    </article>
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
    <article class="msg assistant streaming">
      <header class="msg-head">
        <span class="avatar agent">
          <IconPhi size={13} />
        </span>
        <span class="msg-name">{t("Agent")}</span>
      </header>
      <div class="msg-body">
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

type MessageRowData = { key: string; kind: "message"; message: Message };
type StreamRowData = { key: string; kind: "stream" };
type ApprovalRowData = {
  key: string;
  kind: "approval";
  approval: Accessor<PendingApproval>;
  update: (approval: PendingApproval) => void;
};
type RunRowData = { key: string; kind: "run" };
type TranscriptRow = MessageRowData | StreamRowData | ApprovalRowData | RunRowData;

const STREAM_ROW: StreamRowData = { key: "stream", kind: "stream" };
const RUN_ROW: RunRowData = { key: "run", kind: "run" };
const messageRows = new WeakMap<Message, MessageRowData>();
const approvalRows = new Map<string, ApprovalRowData>();

function messageRow(message: Message): MessageRowData {
  let row = messageRows.get(message);
  if (!row) {
    row = { key: `message:${message.id}`, kind: "message", message };
    messageRows.set(message, row);
  }
  return row;
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
    case "message":
      return 120;
    case "stream":
      return 72;
    case "approval":
      return 180;
    case "run":
      return 96;
  }
}

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

  const toolNames = createMemo(() => {
    const names = new Map<string, string>();
    for (const message of props.detail?.session.messages ?? []) {
      for (const block of message.blocks) {
        if (block.type === "tool_use") names.set(block.id, block.name);
      }
    }
    return names;
  });

  const durableText = createMemo(() => lastAssistantText(props.detail));
  const durableStream = () => streamMatchesDurable(props.streamText, durableText());
  // Boolean memo: token-rate text changes do not rebuild the row list while
  // the live row stays on screen.
  const streamVisible = createMemo(() => props.streamText.length > 0 && !durableStream());

  const rows = createMemo<TranscriptRow[]>(() => {
    const detail = props.detail;
    if (!detail) return [];
    const result: TranscriptRow[] = [];
    for (const message of detail.session.messages) {
      result.push(messageRow(message));
    }
    if (streamVisible()) result.push(STREAM_ROW);
    const liveApprovals = new Set<string>();
    for (const approval of detail.pendingApprovals) {
      liveApprovals.add(approval.id);
      result.push(approvalRow(approval));
    }
    for (const id of approvalRows.keys()) {
      if (!liveApprovals.has(id)) approvalRows.delete(id);
    }
    if (failedRun()) result.push(RUN_ROW);
    return result;
  });

  // Pinning follows the coalesced live text, which is also the DOM cadence.
  const revision = () => `${coalescedText().length}:${props.streamEnded ? 1 : 0}`;

  function renderRow(row: TranscriptRow) {
    switch (row.kind) {
      case "message":
        return (
          <MessageRow
            message={row.message}
            toolNames={toolNames()}
            onOpenUrl={props.onOpenUrl}
            onSend={props.onSend}
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
  );
}
