// Providers page: a list of providers, and a separate editor surface reached
// by adding or opening one. Persistence goes through the node's provider
// commands; the page never writes files directly.
//
// A new provider is a two-step wizard. Step 1 saves the connection fields
// without a model so the node can preview the live model list; step 2 saves
// only the models the user selected. If a draft provider was created by the
// wizard and the user leaves before finishing, the page removes that empty
// draft so the provider list has no residue.

import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import {
  listProviders,
  removeProvider,
  saveProvider,
  setActiveProvider,
  testProvider,
} from "../bridge";
import { t, tPlural } from "../i18n";
import { errorText } from "../errors";
import { IconArrowLeft, IconChevron, IconCircleAlert, IconPlus } from "../components/Icons";
import { notify } from "../ui/notifications";
import { ProviderEditor, protocolLabel } from "./ProviderEditor";
import {
  cloneModel,
  normalizeModelId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";

type ProviderMode = "list" | "new" | "edit";

function viewToDraft(view: ProviderView): ProviderInput {
  return {
    id: view.id,
    name: view.name,
    protocol: view.protocol,
    baseUrl: view.baseUrl,
    model: view.model,
    models: view.models.map(cloneModel),
    headers: { ...(view.headers ?? {}) },
    websiteUrl: view.websiteUrl ?? "",
    notes: view.notes ?? "",
    // apiKey stays undefined: leaving it alone preserves the stored secret.
  };
}

function emptyDraft(): ProviderInput {
  return {
    name: "",
    protocol: "openai_chat",
    baseUrl: "",
    model: "",
    models: [],
    headers: {},
    websiteUrl: "",
    notes: "",
  };
}

/// Fields that participate in the unsaved-draft check. `id` and the secret are
/// handled separately: an untouched `apiKey` is undefined, while "" clears it.
function draftSnapshot(input: ProviderInput): string {
  return JSON.stringify({
    name: input.name ?? "",
    protocol: input.protocol ?? "openai_chat",
    baseUrl: input.baseUrl ?? "",
    model: input.model ?? "",
    models: input.models ?? [],
    headers: input.headers ?? {},
    websiteUrl: input.websiteUrl ?? "",
    notes: input.notes ?? "",
  });
}

function hasDraftContent(input: ProviderInput): boolean {
  return (
    (input.name ?? "").trim() !== "" ||
    (input.baseUrl ?? "").trim() !== "" ||
    (input.websiteUrl ?? "").trim() !== "" ||
    (input.notes ?? "").trim() !== "" ||
    (input.model ?? "").trim() !== "" ||
    (input.models?.length ?? 0) > 0 ||
    Object.keys(input.headers ?? {}).length > 0 ||
    (input.apiKey ?? "") !== "" ||
    (input.protocol ?? "openai_chat") !== "openai_chat"
  );
}

function apiKeyDirty(input: ProviderInput, view: ProviderView | null): boolean {
  if (input.apiKey === undefined) return false;
  // "" means "clear the stored secret"; only a change when one is stored.
  return input.apiKey !== "" || Boolean(view?.hasKey);
}

/// Build the connection-only part of a provider input.
///
/// Auth is derived, never chosen. A non-empty key leaves `auth` omitted so the
/// node applies the protocol default for the selected request format. An empty
/// key means no auth. An untouched key on a saved provider keeps the stored
/// secret, so it also omits `auth`.
function buildConnection(
  draft: ProviderInput,
  view: ProviderView | null,
  includeId: boolean,
): ProviderInput {
  const input: ProviderInput = {
    name: (draft.name ?? "").trim(),
    protocol: draft.protocol ?? "openai_chat",
    baseUrl: (draft.baseUrl ?? "").trim(),
    websiteUrl: (draft.websiteUrl ?? "").trim(),
    notes: (draft.notes ?? "").trim(),
    headers: draft.headers ?? {},
    // Providers are always enabled from this UI; there is no switch.
    enabled: true,
  };
  if (includeId && draft.id) input.id = draft.id;
  if (draft.apiKey !== undefined) {
    // Whitespace-only input means "no key", not a one-space secret.
    input.apiKey = draft.apiKey.trim() === "" ? "" : draft.apiKey;
  }

  const key = draft.apiKey;
  if (key !== undefined && key.trim() !== "") {
    // A typed key always re-enables authentication, even if an earlier save
    // stored an explicit "none" for this provider. The protocol decides which
    // header the gateway sends.
    input.auth =
      (draft.protocol ?? "openai_chat") === "anthropic_messages" ? "x_api_key" : "bearer";
  } else if (key === "") {
    input.auth = "none";
  } else if (!view?.hasKey) {
    input.auth = "none";
  }
  return input;
}

/// Add selected models without replacing models that are already configured.
function mergeModelSpecs(existing: ModelSpec[], selected: ModelSpec[]): ModelSpec[] {
  const seen = new Set(existing.map((model) => normalizeModelId(model.id)));
  const merged = existing.map(cloneModel);
  for (const model of selected) {
    const key = normalizeModelId(model.id);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    merged.push({ ...cloneModel(model), enabled: true });
  }
  return merged;
}

export function ProvidersPage(props: {
  active: boolean;
  onDirtyChange?: (dirty: boolean) => void;
  onRefresh: () => Promise<void>;
}) {
  const [providers, setProviders] = createSignal<ProviderView[]>([]);
  const [mode, setMode] = createSignal<ProviderMode>("list");
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal<ProviderInput | null>(null);
  const [createdInWizard, setCreatedInWizard] = createSignal(false);
  const [newToken, setNewToken] = createSignal(0);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [loadError, setLoadError] = createSignal("");
  const [testResult, setTestResult] = createSignal<ProviderTestView | null>(null);
  const [pendingExit, setPendingExit] = createSignal(false);

  const isNew = () => mode() === "new";
  const selectedView = () => providers().find((provider) => provider.id === selectedId()) ?? null;

  /// Draft differs from the selected saved provider, or a new form has content.
  function isDirty(): boolean {
    const current = draft();
    if (!current) return false;
    if (isNew()) return hasDraftContent(current);
    const view = selectedView();
    if (!view) return false;
    return (
      draftSnapshot(current) !== draftSnapshot(viewToDraft(view)) || apiKeyDirty(current, view)
    );
  }

  createEffect(() => props.onDirtyChange?.(isDirty()));

  async function refreshList(): Promise<ProviderView[]> {
    const list = await listProviders();
    setProviders(list);
    return list;
  }

  /// Keep the derived view in sync immediately after a save, before the
  /// authoritative list refresh completes. Later saves need `view.hasKey`.
  function upsertProvider(view: ProviderView) {
    setProviders((list) => {
      const index = list.findIndex((provider) => provider.id === view.id);
      if (index === -1) return [...list, view];
      const next = [...list];
      next[index] = view;
      return next;
    });
  }

  onMount(async () => {
    try {
      await refreshList();
      setLoadError("");
    } catch (error) {
      // The list surfaces a contextual retry; a toast would duplicate it.
      setLoadError(errorText(error));
    } finally {
      setLoading(false);
    }
  });

  // Android hardware back: leave the editor before leaving the section.
  onMount(() => {
    const handleBack = (event: Event) => {
      if (!props.active || mode() === "list") return;
      event.preventDefault();
      requestExit();
    };
    window.addEventListener("phi:settings-back", handleBack);
    onCleanup(() => window.removeEventListener("phi:settings-back", handleBack));
  });

  // Leaving Settings (or the app) with an unfinished wizard must not leave a
  // zero-model draft behind.
  onCleanup(() => {
    const id = isNew() && createdInWizard() ? draft()?.id : undefined;
    if (id) {
      void removeProvider(id).catch(() => undefined);
    }
  });

  function resetSurface() {
    setTestResult(null);
    setPendingExit(false);
  }

  function exitToList() {
    const cleanupId = isNew() && createdInWizard() ? (draft()?.id ?? selectedId()) : null;
    setMode("list");
    setSelectedId(null);
    setDraft(null);
    setCreatedInWizard(false);
    resetSurface();
    if (cleanupId) {
      void refreshList().catch(() => undefined);
    }
  }

  async function requestExit() {
    if (isDirty()) {
      setPendingExit(true);
      return;
    }
    if (isNew() && createdInWizard()) {
      const id = draft()?.id ?? selectedId();
      if (id) {
        try {
          await removeProvider(id);
        } catch {
          // A leftover empty draft is harmless; the list refresh hides it.
        }
      }
    }
    exitToList();
  }

  async function discardAndExit() {
    if (isNew() && createdInWizard()) {
      const id = draft()?.id ?? selectedId();
      if (id) {
        try {
          await removeProvider(id);
        } catch {
          // Keep going: the user asked to leave.
        }
      }
    }
    exitToList();
  }

  async function startNew() {
    setPendingExit(false);
    setSelectedId(null);
    setMode("new");
    setCreatedInWizard(false);
    setNewToken((token) => token + 1);
    setDraft(emptyDraft());
    resetSurface();
  }

  function openProvider(view: ProviderView) {
    setPendingExit(false);
    const fresh = providers().find((provider) => provider.id === view.id) ?? view;
    setSelectedId(fresh.id);
    setMode("edit");
    setDraft(viewToDraft(fresh));
    resetSurface();
  }

  /// Persist connection fields only. The node accepts a provider without a
  /// primary model, which is what preview_models needs for step 2.
  async function persistConnection(): Promise<ProviderView | null> {
    const current = draft();
    if (!current) return null;
    try {
      const input = buildConnection(current, selectedView(), true);
      const saved = await saveProvider(input);
      if (!current.id) setCreatedInWizard(true);
      upsertProvider(saved);
      setSelectedId(saved.id);
      // The secret is now stored; never keep the typed value in the draft.
      setDraft({ ...current, id: saved.id, apiKey: undefined });
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      return saved;
    } catch (error) {
      notify(errorText(error), { tone: "danger" });
      return null;
    }
  }

  async function handleContinue(): Promise<ProviderView | null> {
    setBusy(true);
    try {
      return await persistConnection();
    } finally {
      setBusy(false);
    }
  }

  async function persistEdit(): Promise<ProviderView | null> {
    const current = draft();
    if (!current) return null;
    try {
      const input = buildConnection(current, selectedView(), true);
      if (!input.id) {
        const id = selectedId();
        if (id) input.id = id;
      }
      const nextModels = (current.models ?? []).map((model) => ({
        ...cloneModel(model),
        enabled: true,
      }));
      input.models = nextModels;
      input.model = (current.model ?? "").trim() || nextModels[0]?.id || "";

      const saved = await saveProvider(input);
      upsertProvider(saved);
      setSelectedId(saved.id);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      notify(t("Provider saved."), { tone: "success" });
      return saved;
    } catch (error) {
      notify(errorText(error), { tone: "danger" });
      return null;
    }
  }

  async function handleSave() {
    setBusy(true);
    try {
      await persistEdit();
    } finally {
      setBusy(false);
    }
  }

  async function persistModels(selected: ModelSpec[]): Promise<boolean> {
    const current = draft();
    const id = current?.id ?? selectedId();
    if (!current || !id) {
      notify(t("Save the provider before adding models."), { tone: "warning" });
      return false;
    }
    const wasNew = isNew();
    try {
      const wanted = selected.map((model) => ({
        ...cloneModel(model),
        enabled: true,
      }));
      const existing = current.models ?? [];
      const merged = mergeModelSpecs(existing, wanted);
      const selectedPrimary = wanted[0]?.id ?? "";
      let primary = "";
      if (wasNew) {
        primary = selectedPrimary || merged[0]?.id || "";
      } else {
        const currentPrimary = (current.model ?? "").trim();
        primary =
          currentPrimary &&
          merged.some((model) => normalizeModelId(model.id) === normalizeModelId(currentPrimary))
            ? currentPrimary
            : selectedPrimary || merged[0]?.id || "";
      }

      const input = buildConnection(current, selectedView(), true);
      input.id = id;
      input.models = merged;
      input.model = primary;

      const saved = await saveProvider(input);
      if (wasNew) {
        // The user just finished onboarding this provider; make it the one the
        // composer uses instead of silently keeping the previous provider.
        try {
          await setActiveProvider(saved.id);
        } catch {
          // Activation can fail for a keyless remote provider; the explicit
          // "Set as active" action remains available.
        }
      }
      upsertProvider(saved);
      setSelectedId(saved.id);
      setDraft(viewToDraft(saved));
      setCreatedInWizard(false);
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      notify(
        wasNew
          ? tPlural("Added {n} model.", "Added {n} models.", wanted.length)
          : t("Provider saved."),
        { tone: "success" },
      );
      if (wasNew) {
        setMode("list");
        setSelectedId(null);
        setDraft(null);
      }
      return true;
    } catch (error) {
      notify(errorText(error), { tone: "danger" });
      return false;
    }
  }

  async function handleSaveModels(selected: ModelSpec[]): Promise<boolean> {
    setBusy(true);
    try {
      return await persistModels(selected);
    } finally {
      setBusy(false);
    }
  }

  async function handleTest() {
    setBusy(true);
    try {
      const saved = await persistEdit();
      if (!saved) return;
      const result = await testProvider(saved.id);
      setTestResult(result);
      if (!result.ok) {
        notify(result.error || t("Something went wrong."), { tone: "danger" });
      }
    } catch (error) {
      const message = errorText(error);
      setTestResult({ ok: false, latencyMs: 0, modelCount: 0, error: message });
      notify(message, { tone: "danger" });
    } finally {
      setBusy(false);
    }
  }

  async function handleActivate() {
    const id = selectedId() ?? draft()?.id;
    if (!id) return;
    setBusy(true);
    try {
      await setActiveProvider(id);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      notify(t("Provider is now active."), { tone: "success" });
    } catch (error) {
      notify(errorText(error), { tone: "danger" });
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    const view = selectedView();
    if (!view) return;
    setBusy(true);
    try {
      await removeProvider(view.id);
      exitToList();
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      notify(t("Provider deleted."), { tone: "success" });
    } catch (error) {
      notify(errorText(error), { tone: "danger" });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="providers-page">
      <Show
        when={mode() === "list"}
        fallback={
          <div class="provider-editor-view">
            <button
              type="button"
              class="settings-back provider-back"
              onClick={() => void requestExit()}
            >
              <IconArrowLeft size={16} />
              {t("Back")}
            </button>
            <Show when={pendingExit()}>
              <div class="draft-guard" role="alert">
                <span>{t("Discard unsaved changes?")}</span>
                <div class="draft-guard-actions">
                  <button type="button" class="btn danger" onClick={() => void discardAndExit()}>
                    {t("Discard")}
                  </button>
                  <button type="button" class="btn" onClick={() => setPendingExit(false)}>
                    {t("Cancel")}
                  </button>
                </div>
              </div>
            </Show>
            <Show when={draft()} fallback={<div class="loading">{t("Loading…")}</div>}>
              {(current) => (
                <ProviderEditor
                  selectionKey={isNew() ? `new-${newToken()}` : (selectedId() ?? "none")}
                  draft={current()}
                  view={selectedView()}
                  isNew={isNew()}
                  busy={busy()}
                  dirty={isDirty()}
                  error=""
                  notice=""
                  testResult={testResult()}
                  onChange={setDraft}
                  onPersistDraft={handleContinue}
                  onSave={() => void handleSave()}
                  onSaveModels={handleSaveModels}
                  onTest={() => void handleTest()}
                  onDelete={() => void handleDelete()}
                  onActivate={() => void handleActivate()}
                  onCancelNew={() => void discardAndExit()}
                />
              )}
            </Show>
          </div>
        }
      >
        <div class="settings-page-head">
          <div>
            <h1>{t("Model providers")}</h1>
            <p class="settings-subtitle">
              {t(
                "Configure endpoints, credentials and the models each provider offers. One provider is active at a time.",
              )}
            </p>
          </div>
          <Show when={providers().length > 0}>
            <button class="btn primary" type="button" onClick={() => void startNew()}>
              <IconPlus size={15} />
              {t("Add provider")}
            </button>
          </Show>
        </div>

        <Show when={loadError()}>
          <div class="inline-error page-error">
            <IconCircleAlert size={14} />
            <span>{loadError()}</span>
            <button
              type="button"
              class="btn"
              onClick={() => {
                setLoading(true);
                void refreshList()
                  .then(() => setLoadError(""))
                  .catch((error) => setLoadError(errorText(error)))
                  .finally(() => setLoading(false));
              }}
            >
              {t("Retry")}
            </button>
          </div>
        </Show>

        <Show when={!loading()} fallback={<div class="loading">{t("Loading…")}</div>}>
          <Show
            when={providers().length > 0}
            fallback={
              <div class="providers-empty">
                <p class="empty-lead">{t("No providers yet")}</p>
                <p class="hint">
                  {t("Add a provider to call a model. The agent cannot run without one.")}
                </p>
                <button class="btn primary" type="button" onClick={() => void startNew()}>
                  <IconPlus size={15} />
                  {t("Add provider")}
                </button>
              </div>
            }
          >
            <ul class="provider-list">
              <For each={providers()}>
                {(provider) => (
                  <li>
                    <button
                      type="button"
                      class="provider-card"
                      onClick={() => openProvider(provider)}
                    >
                      <span class="provider-card-main">
                        <span class="provider-card-title">
                          <span class="provider-name" title={provider.name}>
                            {provider.name}
                          </span>
                          <Show when={provider.active}>
                            <span class="provider-active">{t("Active")}</span>
                          </Show>
                        </span>
                        <span class="provider-meta">
                          <span>{protocolLabel(provider.protocol)}</span>
                          <span class="provider-sep">·</span>
                          <span>{tPlural("{n} model", "{n} models", provider.models.length)}</span>
                          <span class="provider-sep">·</span>
                          <span>
                            {provider.auth === "none"
                              ? t("No authentication")
                              : provider.hasKey
                                ? t("Key configured")
                                : t("No key")}
                          </span>
                        </span>
                      </span>
                      <IconChevron size={17} class="provider-card-chevron" />
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
