// Chat workspace state: bootstrap, the selected session, live streaming and
// the actions the shell and page share. This is framework-reactive but has no
// DOM or Tauri dependency of its own; platform capabilities enter through
// `bridge` and the notification store.

import {
  createContext,
  createSignal,
  onCleanup,
  onMount,
  useContext,
  type Accessor,
  type JSX,
  type Setter,
} from "solid-js";
import {
  addImageAttachment,
  approve as approveCommand,
  cancelRun as cancelRunCommand,
  createSession as createSessionCommand,
  deleteSession as deleteSessionCommand,
  getBootstrap,
  getSession,
  renameSession as renameSessionCommand,
  sendMessage,
} from "../../bridge";
import { errorText } from "../../errors";
import { setLang, t } from "../../i18n";
import { setPlatform } from "../../layout/environment";
import { platform } from "../../platform";
import { notify } from "../../ui/notifications";
import { isActiveState } from "../../status";
import type {
  Bootstrap,
  ImageAsset,
  ModelSelection,
  ProviderView,
  Run,
  SessionDetail,
} from "../../types";
import { activeProviderOf, useModelSelection } from "./modelSelection";
import { prunePendingCalls, reuseMessages } from "./sessionStream";
import { usePhiStream } from "./usePhiStream";

export interface ChatStore {
  boot: Accessor<Bootstrap | null>;
  detail: Accessor<SessionDetail | null>;
  currentId: Accessor<string | null>;
  now: Accessor<number>;
  input: Accessor<string>;
  setInput: Setter<string>;
  attachments: Accessor<ImageAsset[]>;
  attaching: Accessor<boolean>;
  attachImages: () => Promise<void>;
  removeAttachment: (assetId: string) => void;
  creatingSession: Accessor<boolean>;
  deletingSession: Accessor<string | null>;
  selection: Accessor<ModelSelection | null>;
  updateSelection: (selection: ModelSelection) => void;
  provider: Accessor<ProviderView | null>;
  currentRun: Accessor<Run | null>;
  isRunning: Accessor<boolean>;
  modeOk: Accessor<boolean>;
  composerDisabled: Accessor<boolean>;
  composerDisabledHint: Accessor<string>;
  streamText: Accessor<string>;
  streamEnded: Accessor<boolean>;
  refresh: () => Promise<void>;
  newSession: () => Promise<void>;
  selectSession: (id: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  send: () => void;
  handleInlineSend: (text: string) => Promise<void>;
  resolveApproval: (approvalId: string, approved: boolean) => Promise<void>;
  cancelRun: (runId: string) => Promise<void>;
  stopCurrentRun: () => void;
  notifyError: (error: unknown) => void;
}

const ChatContext = createContext<ChatStore>();

export function ChatProvider(props: { children: JSX.Element }) {
  const [boot, setBoot] = createSignal<Bootstrap | null>(null);
  const [detail, setDetail] = createSignal<SessionDetail | null>(null);
  const [currentId, setCurrentId] = createSignal<string | null>(null);
  const [input, setInput] = createSignal("");
  const [attachments, setAttachments] = createSignal<ImageAsset[]>([]);
  const [attaching, setAttaching] = createSignal(false);
  const [creatingSession, setCreatingSession] = createSignal(false);
  const [deletingSession, setDeletingSession] = createSignal<string | null>(null);
  const [now, setNow] = createSignal(Date.now());
  let inlineSendBusy = false;
  let composerSendBusy = false;

  const notifyError = (error: unknown) => {
    notify(errorText(error), { tone: "danger" });
  };

  const stream = usePhiStream({
    currentId,
    currentRun: () => currentRun(),
    refresh: () => refresh(),
    notifyError,
  });
  const { selection, updateSelection } = useModelSelection(boot);

  async function refresh() {
    const bootstrap = await getBootstrap();
    setLang(bootstrap.language);
    setPlatform(bootstrap.platform);
    setBoot(bootstrap);
    const id = currentId() ?? bootstrap.lastSessionId;
    if (!id) return;
    setCurrentId(id);
    const next = await getSession(id);
    // A slower response for another session must not overwrite the one the
    // user is looking at now.
    if (currentId() !== id) return;
    setDetail((prev) => reuseMessages(prev, next));
    stream.setPendingCalls((calls) => prunePendingCalls(calls, next));
  }

  function currentRun(): Run | null {
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

  async function attachImages() {
    if (attaching()) return;
    const id = currentId();
    if (!id) {
      notify(t("Open or create a session before attaching images."), { tone: "warning" });
      return;
    }

    let paths: string[];
    try {
      paths = await platform.openImages(t("Images"));
    } catch (error) {
      notifyError(error);
      return;
    }
    if (paths.length === 0) return;

    setAttaching(true);
    const added: ImageAsset[] = [];
    try {
      for (const path of paths) {
        // Stop uploading when the user switches away; the attachment list
        // belongs to the session that is open now.
        if (currentId() !== id) break;
        try {
          added.push(await addImageAttachment(id, path));
        } catch (error) {
          notifyError(error);
        }
      }
      if (added.length > 0 && currentId() === id) {
        setAttachments((prev) => {
          const seen = new Set(prev.map((asset) => asset.assetId));
          return [...prev, ...added.filter((asset) => !seen.has(asset.assetId))];
        });
      }
    } finally {
      setAttaching(false);
    }
  }

  function removeAttachment(assetId: string) {
    setAttachments((prev) => prev.filter((asset) => asset.assetId !== assetId));
  }

  async function sendText(text: string, fromComposer: boolean) {
    const id = currentId();
    const outgoing = fromComposer ? attachments() : [];
    const attachmentIds = outgoing.map((asset) => asset.assetId);
    if (!id || (!text && attachmentIds.length === 0)) return;
    if (fromComposer) setInput("");
    stream.resetStream(id);
    const pick = selection();
    try {
      const result = await sendMessage({
        sessionId: id,
        text,
        ...(pick ? { model: pick.modelId, thinking: pick.thinking } : {}),
        ...(attachmentIds.length > 0 ? { attachmentIds } : {}),
      });
      stream.setStreamRunId(result.runId);
      if (fromComposer) setAttachments([]);
    } catch (error) {
      notifyError(error);
      if (fromComposer && currentId() === id) setInput(text);
    }
    stream.scheduleRefresh();
  }

  function send() {
    if (composerSendBusy || attaching()) return;
    const text = input().trim();
    if (!text && attachments().length === 0) return;
    composerSendBusy = true;
    void sendText(text, true).finally(() => {
      composerSendBusy = false;
    });
  }

  /** A button or form inside an inline interface block. The run guard mirrors
   * the composer: one run at a time, and the message goes through the same
   * session turn. */
  async function handleInlineSend(text: string) {
    const value = text.trim();
    if (!value || inlineSendBusy) return;
    if (isRunning()) {
      notify(t("A run is already active."), { tone: "warning" });
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
      await approveCommand(approvalId, approved, null);
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
    setAttachments([]);
    stream.resetStream(null);
    try {
      const summary = await createSessionCommand(null);
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

  async function deleteSession(id: string) {
    if (deletingSession()) return;
    setDeletingSession(id);
    try {
      await deleteSessionCommand(id);
      if (currentId() === id) {
        stream.resetStream(null);
        setCurrentId(null);
        setDetail(null);
        setAttachments([]);
      }
      // Bootstrap re-projects the session list and picks the next session (or
      // creates a fresh one when the last session was removed).
      await refresh();
      notify(t("Session deleted."), { tone: "success" });
    } catch (error) {
      notifyError(error);
    } finally {
      setDeletingSession(null);
    }
  }

  async function renameSession(id: string, title: string) {
    const next = title.trim();
    const existing = boot()?.sessions.find((session) => session.id === id);
    if (!next || existing?.title === next) return;
    try {
      await renameSessionCommand(id, next);
      const current = detail();
      if (current?.session.id === id) {
        setDetail({ ...current, session: { ...current.session, title: next } });
      }
      await refresh();
    } catch (error) {
      notifyError(error);
    }
  }

  function stopCurrentRun() {
    const run = currentRun();
    if (run) void cancelRun(run.id);
  }

  async function selectSession(id: string) {
    stream.resetStream(null);
    setCurrentId(id);
    setAttachments([]);
    try {
      const next = await getSession(id);
      if (currentId() !== id) return;
      setDetail((prev) => reuseMessages(prev, next));
      stream.setPendingCalls((calls) => prunePendingCalls(calls, next));
    } catch (error) {
      notifyError(error);
    }
  }

  onMount(() => {
    void refresh().catch(notifyError);
    const ticker = window.setInterval(() => setNow(Date.now()), 1000);
    const stopStream = stream.start();
    onCleanup(() => {
      window.clearInterval(ticker);
      stopStream();
    });
  });

  const store: ChatStore = {
    boot,
    detail,
    currentId,
    now,
    input,
    setInput,
    attachments,
    attaching,
    attachImages,
    removeAttachment,
    creatingSession,
    deletingSession,
    selection,
    updateSelection,
    provider,
    currentRun,
    isRunning,
    modeOk,
    composerDisabled,
    composerDisabledHint,
    streamText: stream.streamText,
    streamEnded: stream.streamEnded,
    refresh,
    newSession,
    selectSession,
    deleteSession,
    renameSession,
    send,
    handleInlineSend,
    resolveApproval,
    cancelRun,
    stopCurrentRun,
    notifyError,
  };

  return <ChatContext.Provider value={store}>{props.children}</ChatContext.Provider>;
}

export function useChat(): ChatStore {
  const store = useContext(ChatContext);
  if (!store) throw new Error("useChat must be used inside a ChatProvider");
  return store;
}
