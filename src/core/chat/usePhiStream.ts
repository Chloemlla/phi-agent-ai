// Live model stream state and the durable-event subscriptions that drive it.

import { createEffect, createSignal, type Accessor } from "solid-js";
import { onModelStream, onPhiEvent } from "../../bridge";
import type { Run } from "../../types";
import type { PendingCall } from "./sessionStream";

const TERMINAL_EVENTS = new Set(["RunSucceeded", "RunFailed", "RunAborted"]);

export type PhiStreamOptions = {
  currentId: Accessor<string | null>;
  currentRun: Accessor<Run | null>;
  refresh: () => Promise<void>;
  notifyError: (error: unknown) => void;
};

export function usePhiStream(options: PhiStreamOptions) {
  const [pendingCalls, setPendingCalls] = createSignal<PendingCall[]>([]);
  const streamText = () =>
    pendingCalls()
      .map((call) => call.text)
      .join("\n\n");
  const [streamRunId, setStreamRunId] = createSignal<string | null>(null);
  const [streamSessionId, setStreamSessionId] = createSignal<string | null>(null);
  const [streamEnded, setStreamEnded] = createSignal(false);
  const [runTerminal, setRunTerminal] = createSignal(false);
  let refreshTimer: number | undefined;
  let streamFallbackTimer: number | undefined;

  function clearStreamFallback() {
    if (streamFallbackTimer) {
      window.clearTimeout(streamFallbackTimer);
      streamFallbackTimer = undefined;
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => {
      void options.refresh().catch(options.notifyError);
    }, 120);
  }

  /** Clear the live view before switching sessions or starting a new turn. */
  function resetStream(sessionId: string | null) {
    setPendingCalls([]);
    clearStreamFallback();
    setStreamRunId(null);
    setStreamSessionId(sessionId);
    setStreamEnded(false);
    setRunTerminal(false);
  }

  // Once every live call has a durable message, drop the stream bookkeeping
  // for the finished run.
  createEffect(() => {
    if (pendingCalls().length > 0) return;
    if (!runTerminal() && !streamEnded()) return;
    clearStreamFallback();
    setStreamRunId(null);
    setStreamSessionId(null);
    setStreamEnded(false);
    setRunTerminal(false);
  });

  /**
   * Register the two Tauri event listeners. Returns the teardown that also
   * clears the debounce and handoff timers.
   */
  function start() {
    let unlistenEvent: (() => void) | undefined;
    let unlistenStream: (() => void) | undefined;

    void onPhiEvent((payload) => {
      const type = payload?.type;
      if (type === "ModelCallFailed" && payload?.callId) {
        // The failed attempt's partial text is dropped; a retry streams as a
        // fresh call and the durable message carries any kept text.
        setPendingCalls((calls) => calls.filter((call) => call.callId !== payload.callId));
      }
      if (type && TERMINAL_EVENTS.has(type)) {
        // Ignore another session's run finishing while this one streams.
        const activeStream = streamRunId();
        if (!activeStream || !payload?.runId || payload.runId === activeStream) {
          // Keep the live text until the durable message arrives. One delayed
          // refresh covers the case where the first projection missed it; only
          // if the handoff really missed do we mark the stream as stopped.
          setRunTerminal(true);
          clearStreamFallback();
          streamFallbackTimer = window.setTimeout(() => {
            if (pendingCalls().length > 0) setStreamEnded(true);
            void options.refresh().catch(options.notifyError);
          }, 2000);
        }
      }
      scheduleRefresh();
    }).then((fn) => {
      unlistenEvent = fn;
    });

    void onModelStream((payload) => {
      if (payload.kind === "text") {
        const text = payload.text ?? "";
        if (!text) return;
        const sourceSession = streamSessionId();
        if (sourceSession && options.currentId() !== sourceSession) return;
        const runId = streamRunId();
        if (runId && payload.runId !== runId) return;
        if (!runId) {
          // Accept only a run that belongs here: one we just sent, or the
          // active run of the session already on screen. Never latch onto a
          // background run from another session.
          const source = streamSessionId();
          const current = options.currentRun();
          const known =
            source === options.currentId() || (current !== null && current.id === payload.runId);
          if (!known) return;
          setStreamRunId(payload.runId);
          if (!source) setStreamSessionId(options.currentId());
          setStreamEnded(false);
          setRunTerminal(false);
        }
        const callId = payload.callId || `${payload.runId}:${payload.index ?? 0}`;
        setPendingCalls((calls) => {
          const index = calls.findIndex((call) => call.callId === callId);
          if (index === -1) {
            return [...calls, { callId, runId: payload.runId, text }];
          }
          return calls.map((call, i) => (i === index ? { ...call, text: call.text + text } : call));
        });
      }
      // `done` is a model-call boundary, not the end of the run. That call's
      // text stays on screen until its durable assistant message replaces it.
    }).then((fn) => {
      unlistenStream = fn;
    });

    return () => {
      if (refreshTimer) window.clearTimeout(refreshTimer);
      clearStreamFallback();
      unlistenEvent?.();
      unlistenStream?.();
    };
  }

  return {
    pendingCalls,
    setPendingCalls,
    streamText,
    streamRunId,
    setStreamRunId,
    streamEnded,
    resetStream,
    scheduleRefresh,
    start,
  };
}
