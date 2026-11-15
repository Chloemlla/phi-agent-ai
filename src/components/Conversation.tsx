// Conversation transcript: document flow with role rows, collapsible tool
// blocks and risk-graded approval cards. No left/right bubbles.

import { For, Show, type JSX } from "solid-js";
import { formatClock } from "../format";
import { t, tArgs } from "../i18n";
import { isActiveState, runTone, stateLabel } from "../status";
import type { Block, PendingApproval, Run, SessionDetail } from "../types";
import { ApprovalCard } from "./ApprovalCard";
import {
  IconAlert,
  IconCheck,
  IconChevron,
  IconClose,
  IconPhi,
  IconWrench,
} from "./Icons";

function Banner(props: { tone: "warn" | "danger"; children: JSX.Element }) {
  return (
    <div classList={{ banner: true, [props.tone]: true }} role={props.tone === "danger" ? "alert" : "status"}>
      <IconAlert size={15} />
      <div>{props.children}</div>
    </div>
  );
}

function ToolBlock(props: { block: Block }) {
  switch (props.block.type) {
    case "text":
      return <div class="text">{props.block.text}</div>;
    case "tool_use":
      return (
        <details class="tool">
          <summary>
            <span class="tool-glyph ok">
              <IconWrench size={13} />
            </span>
            <span class="tool-name mono">
              {tArgs("Call {name}", { name: props.block.name })}
            </span>
            <span class="chevron">
              <IconChevron size={13} />
            </span>
          </summary>
          <pre class="mono">{JSON.stringify(props.block.input, null, 2)}</pre>
        </details>
      );
    case "tool_result":
      return (
        <details class="tool" open={props.block.is_error}>
          <summary>
            <span classList={{ "tool-glyph": true, error: props.block.is_error, ok: !props.block.is_error }}>
              <Show
                when={props.block.is_error}
                fallback={<IconCheck size={13} />}
              >
                <IconClose size={12} />
              </Show>
            </span>
            <span class="tool-name mono">
              {props.block.is_error ? t("Tool error") : t("Tool result")}
            </span>
            <span class="chevron">
              <IconChevron size={13} />
            </span>
          </summary>
          <pre classList={{ mono: true, error: props.block.is_error }}>
            {props.block.content}
          </pre>
        </details>
      );
  }
}

function MessageRow(props: { message: { role: "user" | "assistant"; ts: string; blocks: Block[] } }) {
  const user = () => props.message.role === "user";
  return (
    <article classList={{ msg: true, user: user(), assistant: !user() }}>
      <header class="msg-head">
        <span classList={{ avatar: true, user: user(), agent: !user() }}>
          <Show when={!user()} fallback={<span>{t("You").slice(0, 1)}</span>}>
            <IconPhi size={13} />
          </Show>
        </span>
        <span class="msg-name">{user() ? t("You") : t("Agent")}</span>
        <time class="msg-time mono">{formatClock(props.message.ts)}</time>
      </header>
      <div class="msg-body">
        <For each={props.message.blocks}>
          {(block) => <ToolBlock block={block} />}
        </For>
      </div>
    </article>
  );
}

function EmptyState(props: { modelConfigured: boolean }) {
  return (
    <div class="empty-state">
      <div class="empty-icon">
        <IconPhi size={28} />
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
          {t(
            "No provider is configured. Add one in Settings to start a task.",
          )}
        </p>
      </Show>
    </div>
  );
}

const FAILED_STATES = ["AwaitingReconciliation", "Failed", "Aborted"];

export function Conversation(props: {
  detail: SessionDetail | null;
  run: Run | null;
  streamText: string;
  modeOk: boolean;
  modeReason: string;
  error: string;
  modelConfigured: boolean;
  now: number;
  onResolve: (approvalId: string, approved: boolean) => Promise<void>;
}) {
  const failedRun = () =>
    props.run && FAILED_STATES.includes(props.run.state) ? props.run : null;

  return (
    <div class="conversation-inner">
      <Show when={!props.modeOk}>
        <Banner tone="warn">
          {props.modeReason || t("The node is not in normal mode; sending is disabled")}
        </Banner>
      </Show>
      <Show when={props.error}>
        <Banner tone="danger">{props.error}</Banner>
      </Show>

      <Show
        when={props.detail}
        fallback={<div class="loading">{t("Loading…")}</div>}
      >
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

            <For each={detail().session.messages}>
              {(message) => <MessageRow message={message} />}
            </For>

            <Show when={props.streamText}>
              <article class="msg assistant streaming">
                <header class="msg-head">
                  <span class="avatar agent">
                    <IconPhi size={13} />
                  </span>
                  <span class="msg-name">{t("Agent")}</span>
                </header>
                <div class="msg-body">
                  <div class="text">{props.streamText}</div>
                </div>
              </article>
            </Show>

            <For each={detail().pendingApprovals}>
              {(approval: PendingApproval) => (
                <ApprovalCard
                  approval={approval}
                  now={props.now}
                  onResolve={(approved) => props.onResolve(approval.id, approved)}
                />
              )}
            </For>

            <Show when={failedRun()}>
              {(run) => (
                <div
                  classList={{
                    notice: true,
                    danger: runTone(run().state) === "danger",
                  }}
                  role="status"
                >
                  <div class="notice-title">
                    <span classList={{ dot: true, [`tone-${runTone(run().state)}`]: true }} />
                    {stateLabel(run().state)}
                  </div>
                  <Show when={run().error}>
                    <p>{run().error}</p>
                  </Show>
                  <Show when={run().stopConfirmation}>
                    <p class="mono">{run().stopConfirmation}</p>
                  </Show>
                </div>
              )}
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
