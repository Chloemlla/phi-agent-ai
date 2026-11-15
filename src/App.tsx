import { Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { setLang, t } from "./i18n";
import { Conversation } from "./components/Conversation";
import { Composer } from "./components/Composer";
import { Runbar } from "./components/Runbar";
import { Sidebar } from "./components/Sidebar";
import { Titlebar } from "./components/Titlebar";
import { WorkbenchPane } from "./components/WorkbenchPane";
import { SettingsPage } from "./settings/SettingsPage";
import { isActiveState } from "./status";
import type {
  Bootstrap,
  ModelStreamEvent,
  SendResult,
  SessionDetail,
  SessionSummary,
} from "./types";
import type { ViewAction } from "./workbench/types";
import "./App.css";

function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

function App() {
  const [boot, setBoot] = createSignal<Bootstrap | null>(null);
  const [detail, setDetail] = createSignal<SessionDetail | null>(null);
  const [currentId, setCurrentId] = createSignal<string | null>(null);
  const [input, setInput] = createSignal("");
  const [error, setError] = createSignal("");
  const [streamText, setStreamText] = createSignal("");
  const [streamRunId, setStreamRunId] = createSignal<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const [view, setView] = createSignal<"chat" | "settings">("chat");
  const [now, setNow] = createSignal(Date.now());
  let transcriptRef: HTMLElement | undefined;
  let refreshTimer: number | undefined;

  async function refresh() {
    const bootstrap = await invoke<Bootstrap>("get_bootstrap");
    setLang(bootstrap.language);
    setBoot(bootstrap);
    const id = currentId() ?? bootstrap.lastSessionId;
    if (id) {
      setCurrentId(id);
      const next = await invoke<SessionDetail>("get_session", { sessionId: id });
      setDetail(next);
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => {
      refresh().catch((e) => setError(errorText(e)));
    }, 120);
  }

  onMount(() => {
    refresh().catch((e) => setError(errorText(e)));
    const ticker = window.setInterval(() => setNow(Date.now()), 1000);

    let unlistenEvent: (() => void) | undefined;
    let unlistenStream: (() => void) | undefined;
    listen<{ payload?: { type?: string } }>("phi://event", (event) => {
      const type = event.payload?.payload?.type;
      if (
        type === "RunSucceeded" ||
        type === "RunFailed" ||
        type === "RunAborted"
      ) {
        setStreamText("");
        setStreamRunId(null);
      }
      scheduleRefresh();
    }).then((fn) => {
      unlistenEvent = fn;
    });
    listen<ModelStreamEvent>("phi://model-stream", (event) => {
      const payload = event.payload;
      if (payload.kind === "text") {
        const runId = streamRunId();
        if (runId && payload.runId !== runId) return;
        if (!runId) setStreamRunId(payload.runId);
        setStreamText((current) => current + (payload.text ?? ""));
      } else if (payload.kind === "done") {
        const runId = streamRunId();
        if (runId && payload.runId !== runId) return;
        window.setTimeout(() => {
          setStreamText("");
          setStreamRunId(null);
        }, 250);
      }
    }).then((fn) => {
      unlistenStream = fn;
    });

    onCleanup(() => {
      window.clearInterval(ticker);
      if (refreshTimer) window.clearTimeout(refreshTimer);
      unlistenEvent?.();
      unlistenStream?.();
    });
  });

  createEffect(() => {
    const messages = detail()?.session.messages.length ?? 0;
    const approvals = detail()?.pendingApprovals.length ?? 0;
    const streaming = streamText().length;
    void messages;
    void approvals;
    void streaming;
    if (transcriptRef) {
      queueMicrotask(() => {
        if (transcriptRef) transcriptRef.scrollTop = transcriptRef.scrollHeight;
      });
    }
  });

  async function sendText(text: string, fromComposer: boolean) {
    const id = currentId();
    if (!text || !id) return;
    setError("");
    if (fromComposer) setInput("");
    setStreamText("");
    setStreamRunId(null);
    try {
      const result = await invoke<SendResult>("send_message", {
        sessionId: id,
        text,
        // The agent decides whether a workbench view helps; the shell always
        // offers the tool.
        workbench: true,
      });
      setStreamRunId(result.runId);
    } catch (e) {
      setError(errorText(e));
      if (fromComposer) setInput(text);
    }
    scheduleRefresh();
  }

  function send() {
    const text = input().trim();
    if (text) void sendText(text, true);
  }

  async function handleViewAction(action: ViewAction) {
    if (action.kind === "message") {
      await sendText(action.text, false);
      return;
    }
    try {
      await openUrl(action.url);
    } catch (e) {
      setError(errorText(e));
    }
  }

  async function resolveApproval(approvalId: string, approved: boolean) {
    setError("");
    try {
      await invoke("approve", { approvalId, approved, note: null });
    } catch (e) {
      setError(errorText(e));
    }
    scheduleRefresh();
  }

  async function cancelRun(runId: string) {
    setError("");
    try {
      await invoke("cancel_run", { runId });
    } catch (e) {
      setError(errorText(e));
    }
    scheduleRefresh();
  }

  async function newSession() {
    setError("");
    setStreamText("");
    setStreamRunId(null);
    try {
      const summary = await invoke<SessionSummary>("create_session", { title: null });
      setCurrentId(summary.id);
      setDetail(await invoke<SessionDetail>("get_session", { sessionId: summary.id }));
      await refresh();
    } catch (e) {
      setError(errorText(e));
    }
  }

  async function selectSession(id: string) {
    setStreamText("");
    setStreamRunId(null);
    setCurrentId(id);
    try {
      setDetail(await invoke<SessionDetail>("get_session", { sessionId: id }));
    } catch (e) {
      setError(errorText(e));
    }
  }

  const currentRun = () => {
    const runs = detail()?.session.runs ?? [];
    return runs.length > 0 ? runs[runs.length - 1] : null;
  };
  const isRunning = () => isActiveState(currentRun()?.state);
  const modeOk = () => boot()?.mode === "Normal";
  const uiView = () => detail()?.session.uiView ?? null;
  const showWorkbench = () => !!uiView();

  return (
    <div class="app">
      <Titlebar
        title={
          view() === "settings"
            ? t("Settings")
            : detail()?.session.title || t("New session")
        }
        workspace={boot()?.workspace ?? null}
        mode={boot()?.mode ?? ""}
        modeReason={boot()?.modeReason ?? ""}
        sidebarOpen={sidebarOpen()}
        settingsOpen={view() === "settings"}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onOpenSettings={() =>
          setView((current) => (current === "chat" ? "settings" : "chat"))
        }
      />
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
          <Sidebar
            sessions={boot()?.sessions ?? []}
            currentId={currentId()}
            open={sidebarOpen()}
            mode={boot()?.mode ?? ""}
            nodeId={boot()?.nodeId ?? ""}
            now={now()}
            onNew={newSession}
            onSelect={selectSession}
          />
          <main class="main">
            <section class="conversation">
              <div class="transcript" ref={(element) => (transcriptRef = element)}>
                <Conversation
                  detail={detail()}
                  run={currentRun()}
                  streamText={streamText()}
                  modeOk={modeOk()}
                  modeReason={boot()?.modeReason ?? ""}
                  error={error()}
                  modelConfigured={boot()?.model.configured ?? false}
                  now={now()}
                  onResolve={resolveApproval}
                />
              </div>
              <div class="dock-wrap">
                <div class="dock">
                  <Runbar
                    run={currentRun()}
                    now={now()}
                    onCancel={cancelRun}
                  />
                  <Composer
                    value={input()}
                    disabled={!modeOk() || isRunning()}
                    running={isRunning()}
                    onInput={setInput}
                    onSend={send}
                  />
                </div>
              </div>
            </section>
            <Show when={showWorkbench()}>
              <WorkbenchPane view={uiView()} onAction={handleViewAction} />
            </Show>
          </main>
        </div>
      </Show>
    </div>
  );
}

export default App;
