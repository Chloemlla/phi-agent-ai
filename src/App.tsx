import { Show, Suspense, createEffect, createSignal, lazy, onCleanup, onMount } from "solid-js";
import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  approve,
  cancelRun as cancelRunCommand,
  createSession,
  getBootstrap,
  getSession,
  sendMessage,
} from "./bridge";
import { setLang, t } from "./i18n";
import { errorText } from "./errors";
import { pushToast } from "./toast";
import { Conversation } from "./components/Conversation";
import { Composer } from "./components/Composer";
import { Runbar } from "./components/Runbar";
import { Sidebar } from "./components/Sidebar";
import { Titlebar } from "./components/Titlebar";
import { Toasts } from "./components/Toasts";
import { isActiveState } from "./status";
import { isMobilePlatform, type Bootstrap, type SessionDetail } from "./types";
import { prunePendingCalls, reuseMessages } from "./app/sessionStream";
import { activeProviderOf, useModelSelection } from "./app/modelSelection";
import { usePhiStream } from "./app/usePhiStream";
import "./App.css";

const SettingsPage = lazy(() =>
  import("./settings/SettingsPage").then((module) => ({
    default: module.SettingsPage,
  })),
);

const NARROW_QUERY = 900;

function App() {
  const [boot, setBoot] = createSignal<Bootstrap | null>(null);
  const [detail, setDetail] = createSignal<SessionDetail | null>(null);
  const [currentId, setCurrentId] = createSignal<string | null>(null);
  const [input, setInput] = createSignal("");
  const [sidebarOpen, setSidebarOpen] = createSignal(false);
  const [creatingSession, setCreatingSession] = createSignal(false);
  const [view, setView] = createSignal<"chat" | "settings">("chat");
  const [narrow, setNarrow] = createSignal(
    typeof window !== "undefined" && window.innerWidth <= NARROW_QUERY,
  );
  const [now, setNow] = createSignal(Date.now());
  let inlineSendBusy = false;
  let sidebarInitialized = false;

  const stream = usePhiStream({
    currentId,
    currentRun: () => currentRun(),
    refresh: () => refresh(),
    notifyError,
  });
  const { selection, updateSelection } = useModelSelection(boot);

  const mobile = () => isMobilePlatform(boot()?.platform);
  const drawerMode = () => mobile() || narrow();

  function notifyError(error: unknown) {
    pushToast("danger", errorText(error));
  }

  async function refresh() {
    const bootstrap = await getBootstrap();
    setLang(bootstrap.language);
    setBoot(bootstrap);
    const id = currentId() ?? bootstrap.lastSessionId;
    if (id) {
      setCurrentId(id);
      const next = await getSession(id);
      // A slower response for another session must not overwrite the one the
      // user is looking at now.
      if (currentId() !== id) return;
      setDetail((prev) => reuseMessages(prev, next));
      stream.setPendingCalls((calls) => prunePendingCalls(calls, next));
    }
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
  const composerDisabled = () => !modeOk() || isRunning() || !provider() || !selection();
  const composerDisabledHint = () => {
    if (!modeOk()) {
      return boot()?.modeReason || t("The node is not in normal mode; sending is disabled");
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

  function requestCloseSettings() {
    // Settings may have an unsaved provider draft and can cancel this event.
    if (window.dispatchEvent(new CustomEvent("phi:settings-leave", { cancelable: true }))) {
      setView("chat");
    }
  }

  function handleOpenSettings() {
    if (drawerMode()) setSidebarOpen(false);
    if (view() === "settings") {
      requestCloseSettings();
      return;
    }
    setView("settings");
  }

  function handleOpenUrl(url: string) {
    void openUrl(url).catch(notifyError);
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

    let unlistenBack: (() => void) | undefined;

    void onBackButtonPress(({ canGoBack }) => {
      if (view() === "settings") {
        // Let a settings pane handle one level (provider detail) first, then
        // let the settings shell guard unsaved drafts.
        if (window.dispatchEvent(new CustomEvent("phi:settings-back", { cancelable: true }))) {
          requestCloseSettings();
        }
        return;
      }
      if (sidebarOpen() && drawerMode()) {
        setSidebarOpen(false);
        return;
      }
      if (canGoBack) {
        window.history.back();
        return;
      }
      void exit(0).catch(() => undefined);
    })
      .then((listener) => {
        unlistenBack = () => {
          void listener.unregister();
        };
      })
      .catch(() => undefined);

    const stopStream = stream.start();

    onCleanup(() => {
      window.clearInterval(ticker);
      window.removeEventListener("resize", onResize);
      document.removeEventListener("keydown", onKeyDown);
      stopStream();
      unlistenBack?.();
    });
  });

  // First bootstrap decides whether the rail starts open.
  createEffect(() => {
    const bootstrap = boot();
    if (!bootstrap || sidebarInitialized) return;
    sidebarInitialized = true;
    const drawer = isMobilePlatform(bootstrap.platform) || window.innerWidth <= NARROW_QUERY;
    setSidebarOpen(!drawer);
  });

  async function sendText(text: string, fromComposer: boolean) {
    const id = currentId();
    if (!text || !id) return;
    if (fromComposer) setInput("");
    stream.resetStream(id);
    const pick = selection();
    try {
      const result = await sendMessage({
        sessionId: id,
        text,
        ...(pick ? { model: pick.modelId, thinking: pick.thinking } : {}),
      });
      stream.setStreamRunId(result.runId);
    } catch (error) {
      notifyError(error);
      if (fromComposer) setInput(text);
    }
    stream.scheduleRefresh();
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
      await approve(approvalId, approved, null);
    } catch (error) {
      notifyError(error);
    }
    stream.scheduleRefresh();
  }

  async function cancelRun(runId: string) {
    try {
      await cancelRunCommand(runId);
    } catch (error) {
      notifyError(error);
    }
    stream.scheduleRefresh();
  }

  async function newSession() {
    if (creatingSession()) return;
    setCreatingSession(true);
    stream.resetStream(null);
    try {
      const summary = await createSession(null);
      setCurrentId(summary.id);
      const next = await getSession(summary.id);
      setDetail((prev) => reuseMessages(prev, next));
      await refresh();
    } catch (error) {
      notifyError(error);
    } finally {
      setCreatingSession(false);
    }
  }

  async function selectSession(id: string) {
    stream.resetStream(null);
    setCurrentId(id);
    try {
      const next = await getSession(id);
      if (currentId() !== id) return;
      setDetail((prev) => reuseMessages(prev, next));
      stream.setPendingCalls((calls) => prunePendingCalls(calls, next));
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
        title={view() === "settings" ? t("Settings") : detail()?.session.title || t("New session")}
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
          <Suspense fallback={<div class="loading">{t("Loading…")}</div>}>
            <SettingsPage
              workspace={boot()?.workspace ?? ""}
              language={boot()?.language ?? "en"}
              onBack={() => setView("chat")}
              onRefresh={refresh}
            />
          </Suspense>
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
            modal={drawerMode()}
            mode={boot()?.mode ?? ""}
            nodeId={boot()?.nodeId ?? ""}
            now={now()}
            onNew={handleNewSession}
            onSelect={handleSelectSession}
          />
          <main class="main" inert={drawerMode() && sidebarOpen() ? true : undefined}>
            <section class="conversation">
              <Conversation
                detail={detail()}
                run={currentRun()}
                streamText={stream.streamText()}
                streamEnded={stream.streamEnded()}
                modeOk={modeOk()}
                modeReason={boot()?.modeReason ?? ""}
                modelConfigured={boot()?.model.configured ?? false}
                now={now()}
                onResolve={resolveApproval}
                onOpenUrl={handleOpenUrl}
                onSend={handleInlineSend}
              />
              <div class="dock-wrap">
                <div class="dock">
                  <Show when={currentRun()}>
                    {(run) => <Runbar run={run()} now={now()} onCancel={cancelRun} />}
                  </Show>
                  <Composer
                    value={input()}
                    disabled={composerDisabled()}
                    disabledHint={composerDisabledHint()}
                    running={isRunning()}
                    provider={provider()}
                    fallbackModel={boot()?.model.model ?? provider()?.model ?? ""}
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
