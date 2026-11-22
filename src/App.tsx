import { Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { setLang, t } from "./i18n";
import { pushToast } from "./toast";
import { Conversation } from "./components/Conversation";
import { Composer } from "./components/Composer";
import { Runbar } from "./components/Runbar";
import { Sidebar } from "./components/Sidebar";
import { Titlebar } from "./components/Titlebar";
import { Toasts } from "./components/Toasts";
import { SettingsPage } from "./settings/SettingsPage";
import { isActiveState } from "./status";
import {
  isMobilePlatform,
  isThinkingLevel,
  type Bootstrap,
  type ModelSelection,
  type ModelStreamEvent,
  type ProviderView,
  type SendResult,
  type SessionDetail,
  type SessionSummary,
  type ThinkingLevel,
} from "./types";
import "./App.css";

const TERMINAL_EVENTS = new Set([
  "RunSucceeded",
  "RunFailed",
  "RunAborted",
]);

const STORAGE_PREFIX = "phi.model.";
const NARROW_QUERY = 900;

/// One in-flight model call's streamed text. The backend journals one
/// assistant message per call, so the live view must track calls separately;
/// concatenating every call into one buffer duplicates earlier text once the
/// durable messages arrive.
type PendingCall = {
  callId: string;
  runId: string;
  text: string;
};

function prunePendingCalls(
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

function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

function activeProviderOf(bootstrap: Bootstrap): ProviderView | null {
  return (
    bootstrap.providers.find((provider) => provider.id === bootstrap.activeProviderId) ??
    bootstrap.providers.find((provider) => provider.active) ??
    bootstrap.providers[0] ??
    null
  );
}

function readStoredSelection(
  providerId: string,
): { modelId: string; thinking: ThinkingLevel } | null {
  try {
    const raw = window.localStorage.getItem(`${STORAGE_PREFIX}${providerId}`);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as {
      modelId?: unknown;
      thinking?: unknown;
    };
    if (typeof parsed.modelId !== "string" || !parsed.modelId) return null;
    return {
      modelId: parsed.modelId,
      thinking: isThinkingLevel(parsed.thinking) ? parsed.thinking : "off",
    };
  } catch {
    return null;
  }
}

function writeStoredSelection(selection: ModelSelection) {
  try {
    window.localStorage.setItem(
      `${STORAGE_PREFIX}${selection.providerId}`,
      JSON.stringify({
        modelId: selection.modelId,
        thinking: selection.thinking,
      }),
    );
  } catch {
    // Persistence is best-effort; the selection still applies to this window.
  }
}

function App() {
  const [boot, setBoot] = createSignal<Bootstrap | null>(null);
  const [detail, setDetail] = createSignal<SessionDetail | null>(null);
  const [currentId, setCurrentId] = createSignal<string | null>(null);
  const [input, setInput] = createSignal("");
  const [pendingCalls, setPendingCalls] = createSignal<PendingCall[]>([]);
  const streamText = () =>
    pendingCalls()
      .map((call) => call.text)
      .join("\n\n");
  const [streamRunId, setStreamRunId] = createSignal<string | null>(null);
  const [streamSessionId, setStreamSessionId] = createSignal<string | null>(null);
  const [streamEnded, setStreamEnded] = createSignal(false);
  const [runTerminal, setRunTerminal] = createSignal(false);
  const [sidebarOpen, setSidebarOpen] = createSignal(false);
  const [creatingSession, setCreatingSession] = createSignal(false);
  const [view, setView] = createSignal<"chat" | "settings">("chat");
  const [selection, setSelection] = createSignal<ModelSelection | null>(null);
  const [narrow, setNarrow] = createSignal(
    typeof window !== "undefined" && window.innerWidth <= NARROW_QUERY,
  );
  const [now, setNow] = createSignal(Date.now());
  let transcriptRef: HTMLElement | undefined;
  let refreshTimer: number | undefined;
  let streamFallbackTimer: number | undefined;
  let inlineSendBusy = false;
  let sidebarInitialized = false;

  const mobile = () => isMobilePlatform(boot()?.platform);
  const drawerMode = () => mobile() || narrow();

  function notifyError(error: unknown) {
    pushToast("danger", errorText(error));
  }

  async function refresh() {
    const bootstrap = await invoke<Bootstrap>("get_bootstrap");
    setLang(bootstrap.language);
    setBoot(bootstrap);
    const id = currentId() ?? bootstrap.lastSessionId;
    if (id) {
      setCurrentId(id);
      const next = await invoke<SessionDetail>("get_session", { sessionId: id });
      // A slower response for another session must not overwrite the one the
      // user is looking at now.
      if (currentId() !== id) return;
      setDetail(next);
      setPendingCalls((calls) => prunePendingCalls(calls, next));
    }
  }

  function clearStreamFallback() {
    if (streamFallbackTimer) {
      window.clearTimeout(streamFallbackTimer);
      streamFallbackTimer = undefined;
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => {
      void refresh().catch(notifyError);
    }, 120);
  }

  function currentRun() {
    const runs = detail()?.session.runs ?? [];
    return runs.length > 0 ? runs[runs.length - 1] : null;
  }

  const isRunning = () => isActiveState(currentRun()?.state);
  const modeOk = () => boot()?.mode === "Normal";
  const provider = () => {
    const bootstrap = boot();
    return bootstrap ? activeProviderOf(bootstrap) : null;
  };
  const composerDisabled = () =>
    !modeOk() || isRunning() || !provider() || !selection();
  const composerDisabledHint = () => {
    if (!modeOk()) {
      return (
        boot()?.modeReason ||
        t("The node is not in normal mode; sending is disabled")
      );
    }
    if (isRunning()) return t("The model cannot be changed while a run is active.");
    if (!provider()) return t("No model provider is configured. Add one in Settings.");
    return t("Choose a model first.");
  };

  function closeDrawer() {
    if (drawerMode()) setSidebarOpen(false);
  }

  function handleSelectSession(id: string) {
    closeDrawer();
    void selectSession(id);
  }

  function handleNewSession() {
    closeDrawer();
    void newSession();
  }

  function handleOpenSettings() {
    if (drawerMode()) setSidebarOpen(false);
    setView((current) => (current === "chat" ? "settings" : "chat"));
  }

  function handleOpenUrl(url: string) {
    void openUrl(url).catch(notifyError);
  }

  function updateSelection(next: ModelSelection) {
    setSelection(next);
    writeStoredSelection(next);
  }

  onMount(() => {
    void refresh().catch(notifyError);
    const ticker = window.setInterval(() => setNow(Date.now()), 1000);
    const onResize = () => setNarrow(window.innerWidth <= NARROW_QUERY);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && sidebarOpen() && drawerMode()) {
        setSidebarOpen(false);
      }
    };
    window.addEventListener("resize", onResize);
    document.addEventListener("keydown", onKeyDown);

    let unlistenEvent: (() => void) | undefined;
    let unlistenStream: (() => void) | undefined;

    void listen<{
      payload?: { type?: string; runId?: string; callId?: string };
    }>("phi://event", (event) => {
      const payload = event.payload?.payload;
      const type = payload?.type;
      if (type === "ModelCallFailed" && payload?.callId) {
        // The failed attempt's partial text is dropped; a retry streams as a
        // fresh call and the durable message carries any kept text.
        setPendingCalls((calls) =>
          calls.filter((call) => call.callId !== payload.callId),
        );
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
            void refresh().catch(notifyError);
          }, 2000);
        }
      }
      scheduleRefresh();
    }).then((fn) => {
      unlistenEvent = fn;
    });

    void listen<ModelStreamEvent>("phi://model-stream", (event) => {
      const payload = event.payload;
      if (payload.kind === "text") {
        const text = payload.text ?? "";
        if (!text) return;
        const sourceSession = streamSessionId();
        if (sourceSession && currentId() !== sourceSession) return;
        const runId = streamRunId();
        if (runId && payload.runId !== runId) return;
        if (!runId) {
          // Accept only a run that belongs here: one we just sent, or the
          // active run of the session already on screen. Never latch onto a
          // background run from another session.
          const source = streamSessionId();
          const current = currentRun();
          const known =
            source === currentId() ||
            (current !== null && current.id === payload.runId);
          if (!known) return;
          setStreamRunId(payload.runId);
          if (!source) setStreamSessionId(currentId());
          setStreamEnded(false);
          setRunTerminal(false);
        }
        const callId = payload.callId || `${payload.runId}:${payload.index ?? 0}`;
        setPendingCalls((calls) => {
          const index = calls.findIndex((call) => call.callId === callId);
          if (index === -1) {
            return [...calls, { callId, runId: payload.runId, text }];
          }
          return calls.map((call, i) =>
            i === index ? { ...call, text: call.text + text } : call,
          );
        });
      }
      // `done` is a model-call boundary, not the end of the run. That call's
      // text stays on screen until its durable assistant message replaces it.
    }).then((fn) => {
      unlistenStream = fn;
    });

    onCleanup(() => {
      window.clearInterval(ticker);
      window.removeEventListener("resize", onResize);
      document.removeEventListener("keydown", onKeyDown);
      if (refreshTimer) window.clearTimeout(refreshTimer);
      clearStreamFallback();
      unlistenEvent?.();
      unlistenStream?.();
    });
  });

  // First bootstrap decides whether the rail starts open.
  createEffect(() => {
    const bootstrap = boot();
    if (!bootstrap || sidebarInitialized) return;
    sidebarInitialized = true;
    const drawer =
      isMobilePlatform(bootstrap.platform) ||
      window.innerWidth <= NARROW_QUERY;
    setSidebarOpen(!drawer);
  });

  // Keep the composer's model choice valid for the active provider.
  createEffect(() => {
    const bootstrap = boot();
    if (!bootstrap) return;
    const active = provider();
    if (!active) {
      if (selection()) setSelection(null);
      return;
    }

    const current = selection();
    if (current && current.providerId === active.id) {
      const model = active.models.find(
        (candidate) => candidate.id === current.modelId && candidate.enabled,
      );
      if (model) {
        return;
      }
    }

    const enabled = active.models.filter((candidate) => candidate.enabled);
    if (enabled.length === 0) {
      if (selection()) setSelection(null);
      return;
    }
    const primary =
      enabled.find((candidate) => candidate.id === active.model) ?? enabled[0];
    const stored = readStoredSelection(active.id);
    const storedModel = stored
      ? enabled.find((candidate) => candidate.id === stored.modelId)
      : null;

    const modelId = storedModel?.id ?? primary.id;
    const thinking: ThinkingLevel =
      stored && stored.modelId === modelId ? stored.thinking : "off";
    setSelection({ providerId: active.id, modelId, thinking });
  });

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

  createEffect(() => {
    const messages = detail()?.session.messages.length ?? 0;
    const approvals = detail()?.pendingApprovals.length ?? 0;
    const streaming = streamText().length;
    void messages;
    void approvals;
    void streaming;
    const element = transcriptRef;
    if (!element) return;
    // Do not yank the transcript down while the user is reading earlier output.
    const nearBottom =
      element.scrollHeight - element.scrollTop - element.clientHeight < 96;
    if (!nearBottom) return;
    queueMicrotask(() => {
      if (transcriptRef) transcriptRef.scrollTop = transcriptRef.scrollHeight;
    });
  });

  async function sendText(text: string, fromComposer: boolean) {
    const id = currentId();
    if (!text || !id) return;
    if (fromComposer) setInput("");
    setPendingCalls([]);
    clearStreamFallback();
    setStreamRunId(null);
    setStreamSessionId(id);
    setStreamEnded(false);
    setRunTerminal(false);
    const pick = selection();
    try {
      const result = await invoke<SendResult>("send_message", {
        sessionId: id,
        text,
        ...(pick
          ? { model: pick.modelId, thinking: pick.thinking }
          : {}),
      });
      setStreamRunId(result.runId);
    } catch (error) {
      notifyError(error);
      if (fromComposer) setInput(text);
    }
    scheduleRefresh();
  }

  function send() {
    const text = input().trim();
    if (text) void sendText(text, true);
  }

  /** A button or form inside an inline interface block. The run guard mirrors
   * the composer: one run at a time, and the message goes through the same
   * session turn. */
  async function handleInlineSend(text: string) {
    const value = text.trim();
    if (!value || inlineSendBusy) return;
    if (isRunning()) {
      notifyError(t("A run is already active."));
      return;
    }
    inlineSendBusy = true;
    try {
      await sendText(value, false);
    } finally {
      inlineSendBusy = false;
    }
  }

  async function resolveApproval(approvalId: string, approved: boolean) {
    try {
      await invoke("approve", { approvalId, approved, note: null });
    } catch (error) {
      notifyError(error);
    }
    scheduleRefresh();
  }

  async function cancelRun(runId: string) {
    try {
      await invoke("cancel_run", { runId });
    } catch (error) {
      notifyError(error);
    }
    scheduleRefresh();
  }

  async function newSession() {
    if (creatingSession()) return;
    setCreatingSession(true);
    setPendingCalls([]);
    clearStreamFallback();
    setStreamRunId(null);
    setStreamSessionId(null);
    setStreamEnded(false);
    setRunTerminal(false);
    try {
      const summary = await invoke<SessionSummary>("create_session", {
        title: null,
      });
      setCurrentId(summary.id);
      setDetail(
        await invoke<SessionDetail>("get_session", { sessionId: summary.id }),
      );
      await refresh();
    } catch (error) {
      notifyError(error);
    } finally {
      setCreatingSession(false);
    }
  }

  async function selectSession(id: string) {
    setPendingCalls([]);
    clearStreamFallback();
    setStreamRunId(null);
    setStreamSessionId(null);
    setStreamEnded(false);
    setRunTerminal(false);
    setCurrentId(id);
    try {
      const next = await invoke<SessionDetail>("get_session", {
        sessionId: id,
      });
      if (currentId() !== id) return;
      setDetail(next);
      setPendingCalls((calls) => prunePendingCalls(calls, next));
    } catch (error) {
      notifyError(error);
    }
  }

  return (
    <div
      classList={{
        app: true,
        mobile: mobile(),
        "drawer-mode": drawerMode(),
      }}
    >
      <Titlebar
        title={
          view() === "settings"
            ? t("Settings")
            : detail()?.session.title || t("New session")
        }
        workspace={boot()?.workspace ?? null}
        mode={boot()?.mode ?? ""}
        modeReason={boot()?.modeReason ?? ""}
        platform={boot()?.platform ?? "linux"}
        sidebarOpen={sidebarOpen()}
        settingsOpen={view() === "settings"}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onOpenSettings={handleOpenSettings}
      />
      <Toasts />
      <Show
        when={view() === "chat"}
        fallback={
          <SettingsPage
            workspace={boot()?.workspace ?? ""}
            language={boot()?.language ?? "en"}
            onBack={() => setView("chat")}
            onRefresh={refresh}
          />
        }
      >
        <div class="shell">
          <Show when={drawerMode() && sidebarOpen()}>
            <div
              class="sidebar-backdrop"
              aria-hidden="true"
              onClick={() => setSidebarOpen(false)}
            />
          </Show>
          <Sidebar
            sessions={boot()?.sessions ?? []}
            currentId={currentId()}
            open={sidebarOpen()}
            mode={boot()?.mode ?? ""}
            nodeId={boot()?.nodeId ?? ""}
            now={now()}
            onNew={handleNewSession}
            onSelect={handleSelectSession}
          />
          <main class="main">
            <section class="conversation">
              <div class="transcript" ref={(element) => (transcriptRef = element)}>
                <Conversation
                  detail={detail()}
                  run={currentRun()}
                  streamText={streamText()}
                  streamEnded={streamEnded()}
                  modeOk={modeOk()}
                  modeReason={boot()?.modeReason ?? ""}
                  modelConfigured={boot()?.model.configured ?? false}
                  now={now()}
                  onResolve={resolveApproval}
                  onOpenUrl={handleOpenUrl}
                  onSend={handleInlineSend}
                />
              </div>
              <div class="dock-wrap">
                <div class="dock">
                  <Show when={currentRun()}>
                    {(run) => (
                      <Runbar
                        run={run()}
                        now={now()}
                        onCancel={cancelRun}
                      />
                    )}
                  </Show>
                  <Composer
                    value={input()}
                    disabled={composerDisabled()}
                    disabledHint={composerDisabledHint()}
                    running={isRunning()}
                    provider={provider()}
                    fallbackModel={
                      boot()?.model.model ?? provider()?.model ?? ""
                    }
                    selection={selection()}
                    onModelChange={updateSelection}
                    onInput={setInput}
                    onSend={send}
                  />
                </div>
              </div>
            </section>
          </main>
        </div>
      </Show>
    </div>
  );
}

export default App;
