// Pure helpers for the live stream and the durable transcript.

import type { SessionDetail } from "../types";

/// One in-flight model call's streamed text. The backend journals one
/// assistant message per call, so the live view must track calls separately;
/// concatenating every call into one buffer duplicates earlier text once the
/// durable messages arrive.
export type PendingCall = {
  callId: string;
  runId: string;
  text: string;
};

export function prunePendingCalls(
  calls: PendingCall[],
  detail: SessionDetail | null,
): PendingCall[] {
  if (calls.length === 0) return calls;
  const assistantTexts = (detail?.session.messages ?? [])
    .filter((message) => message.role === "assistant")
    .map((message) =>
      message.blocks
        .map((block) => (block.type === "text" ? block.text : ""))
        .join("")
        .trim(),
    )
    .filter(Boolean);
  if (assistantTexts.length === 0) return calls;

  // Only the newest messages can belong to the calls still on screen. Matching
  // in order (not just "any message contains this text") handles repeated
  // answers across turns without dropping live text early.
  const remaining: PendingCall[] = [];
  let cursor = Math.max(0, assistantTexts.length - calls.length - 1);
  for (const call of calls) {
    const text = call.text.trim();
    if (!text) {
      remaining.push(call);
      continue;
    }
    let matched = -1;
    for (let index = cursor; index < assistantTexts.length; index += 1) {
      if (assistantTexts[index].includes(text)) {
        matched = index;
        break;
      }
    }
    if (matched === -1) {
      remaining.push(call);
    } else {
      cursor = matched + 1;
    }
  }
  return remaining;
}

/**
 * Reuse immutable message objects across refreshes. `invoke` returns fresh
 * objects every time; without this, Solid's referential `<For>` rebuilds the
 * whole transcript and resets open tool blocks and inline UI form state.
 */
export function reuseMessages(
  prev: SessionDetail | null,
  next: SessionDetail,
): SessionDetail {
  if (!prev || prev.session.id !== next.session.id) return next;
  const byId = new Map(
    prev.session.messages.map((message) => [message.id, message]),
  );
  const messages = next.session.messages.map(
    (message) => byId.get(message.id) ?? message,
  );
  return messages.every(
    (message, index) => message === next.session.messages[index],
  )
    ? next
    : { ...next, session: { ...next.session, messages } };
}
