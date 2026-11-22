// Providers page: list + editor orchestration. All persistence goes through
// the node's provider commands; the page never writes files directly.
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
import { t, tArgs } from "../i18n";
import { errorText } from "../errors";
import { pushToast } from "../toast";
import { IconArrowLeft, IconCircleAlert, IconPlus } from "../components/Icons";
import { ProviderEditor, protocolLabel } from "./ProviderEditor";
import {
  cloneModel,
  normalizeModelId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";

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
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal<ProviderInput | null>(null);
  const [isNew, setIsNew] = createSignal(false);
  const [createdInWizard, setCreatedInWizard] = createSignal(false);
  const [newToken, setNewToken] = createSignal(0);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [testResult, setTestResult] = createSignal<ProviderTestView | null>(null);
  const [narrow, setNarrow] = createSignal(
    typeof window !== "undefined" && window.matchMedia("(max-width: 760px)").matches,
  );
  const [detailOpen, setDetailOpen] = createSignal(false);
  const [pendingSwitch, setPendingSwitch] = createSignal<ProviderView | "new" | null>(null);

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

  const visibleProviders = () => {
    if (!isNew()) return providers();
    const draftId = draft()?.id;
    return draftId ? providers().filter((provider) => provider.id !== draftId) : providers();
  };

  function selectView(view: ProviderView) {
    setPendingSwitch(null);
    setSelectedId(view.id);
    setIsNew(false);
    setCreatedInWizard(false);
    setDraft(viewToDraft(view));
  }

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

  async function load(preferredId?: string) {
    const list = await refreshList();
    if (isNew() && !preferredId) return;
    const targetId = preferredId ?? selectedId();
    if (targetId) {
      const view = list.find((provider) => provider.id === targetId);
      if (view) {
        selectView(view);
        return;
      }
    }
    const fallback = list.find((provider) => provider.active) ?? list[0];
    if (fallback) {
      selectView(fallback);
    } else {
      setSelectedId(null);
      setDraft(null);
      setIsNew(false);
      setCreatedInWizard(false);
    }
  }

  onMount(async () => {
    try {
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  });

  onMount(() => {
    const query = window.matchMedia("(max-width: 760px)");
    const update = () => {
      const matches = query.matches;
      // Crossing into the narrow layout with a draft must keep that editor
      // visible instead of falling back to the list and re-seeding it.
      if (matches && !narrow() && draft()) setDetailOpen(true);
      setNarrow(matches);
    };
    query.addEventListener("change", update);
    onCleanup(() => query.removeEventListener("change", update));
  });

  // Android hardware back while Settings is open: close the provider detail
  // first so App can close Settings only on the second press.
  onMount(() => {
    const handleBack = (event: Event) => {
      if (!props.active || !narrow() || !detailOpen()) return;
      event.preventDefault();
      void closeDetail();
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

  /// Guard switching away from an edited provider. The draft is only replaced
  /// after the user explicitly discards it.
  function requestNew() {
    if (isDirty()) {
      setDetailOpen(true);
      setPendingSwitch("new");
      return;
    }
    void startNew();
  }

  function requestOpen(view: ProviderView) {
    if (!isNew() && view.id === selectedId()) {
      setPendingSwitch(null);
      setDetailOpen(true);
      return;
    }
    if (isDirty()) {
      setDetailOpen(true);
      setPendingSwitch(view);
      return;
    }
    void openProvider(view);
  }

  function discardPending() {
    const action = pendingSwitch();
    setPendingSwitch(null);
    if (action === "new") {
      void startNew();
    } else if (action) {
      void openProvider(action);
    }
  }

  /// Mobile Back. A wizard has a persisted connection (and possibly a
  /// zero-model provider), so closing it must also clean that up.
  async function closeDetail() {
    setPendingSwitch(null);
    if (isNew()) {
      await cancelNew();
      return;
    }
    setDetailOpen(false);
  }

  async function startNew() {
    setPendingSwitch(null);
    setDetailOpen(true);
    const staleId = isNew() && createdInWizard() ? draft()?.id : undefined;
    if (staleId) {
      try {
        await removeProvider(staleId);
      } catch {
        // A leftover empty draft is harmless; continue with a fresh form.
      }
    }
    setSelectedId(null);
    setIsNew(true);
    setCreatedInWizard(false);
    setNewToken((token) => token + 1);
    setDraft(emptyDraft());
    setTestResult(null);
    setError("");
    setNotice("");
    if (staleId) {
      try {
        await refreshList();
      } catch {
        // The list refreshes again on the next navigation.
      }
    }
  }

  async function openProvider(view: ProviderView) {
    setPendingSwitch(null);
    setDetailOpen(true);
    if (isNew() && createdInWizard()) {
      const staleId = draft()?.id;
      if (staleId && staleId !== view.id) {
        try {
          await removeProvider(staleId);
          await refreshList();
        } catch {
          // Keep going: the user asked for a different provider.
        }
      }
    }
    setTestResult(null);
    setError("");
    setNotice("");
    const fresh = providers().find((provider) => provider.id === view.id) ?? view;
    selectView(fresh);
  }

  async function cancelNew() {
    setPendingSwitch(null);
    setDetailOpen(false);
    const cleanupId = createdInWizard() ? (draft()?.id ?? selectedId()) : null;
    setIsNew(false);
    setCreatedInWizard(false);
    setSelectedId(null);
    setDraft(null);
    setTestResult(null);
    setError("");
    setNotice("");
    if (cleanupId) {
      try {
        await removeProvider(cleanupId);
      } catch {
        // If removal fails, the draft simply stays in the provider list.
      }
    }
    try {
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  }

  /// Persist connection fields only. The node accepts a provider without a
  /// primary model, which is what preview_models needs for step 2.
  async function persistConnection(): Promise<ProviderView | null> {
    const current = draft();
    if (!current) return null;
    setError("");
    setNotice("");
    try {
      const input = buildConnection(current, selectedView(), true);
      const saved = await saveProvider(input);
      if (!current.id) setCreatedInWizard(true);
      setPendingSwitch(null);
      upsertProvider(saved);
      setSelectedId(saved.id);
      // The secret is now stored; never keep the typed value in the draft.
      setDraft({ ...current, id: saved.id, apiKey: undefined });
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      return saved;
    } catch (e) {
      const message = errorText(e);
      setError(message);
      pushToast("danger", message);
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
    setError("");
    setNotice("");
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
      setIsNew(false);
      setCreatedInWizard(false);
      setPendingSwitch(null);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider saved."));
      return saved;
    } catch (e) {
      const message = errorText(e);
      setError(message);
      pushToast("danger", message);
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
      setError(t("Save the provider before adding models."));
      return false;
    }
    const wasNew = isNew();
    setError("");
    setNotice("");
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
      setIsNew(false);
      setCreatedInWizard(false);
      setPendingSwitch(null);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      setNotice(
        wasNew ? tArgs("Added {count} models.", { count: wanted.length }) : t("Provider saved."),
      );
      return true;
    } catch (e) {
      const message = errorText(e);
      setError(message);
      pushToast("danger", message);
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
        pushToast("danger", result.error || t("Something went wrong."));
      }
    } catch (e) {
      const message = errorText(e);
      setTestResult({ ok: false, latencyMs: 0, modelCount: 0, error: message });
      pushToast("danger", message);
    } finally {
      setBusy(false);
    }
  }

  async function handleActivate() {
    const id = selectedId() ?? draft()?.id;
    if (!id) return;
    setBusy(true);
    setError("");
    try {
      await setActiveProvider(id);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider is now active."));
    } catch (e) {
      const message = errorText(e);
      setError(message);
      pushToast("danger", message);
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    const view = selectedView();
    if (!view) return;
    setBusy(true);
    setError("");
    try {
      await removeProvider(view.id);
      setPendingSwitch(null);
      setDetailOpen(false);
      setSelectedId(null);
      setDraft(null);
      setIsNew(false);
      setCreatedInWizard(false);
      setTestResult(null);
      await load();
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider deleted."));
    } catch (e) {
      const message = errorText(e);
      setError(message);
      pushToast("danger", message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="providers-page">
      <div class="settings-page-head">
        <div>
          <h1>{t("Model providers")}</h1>
          <p class="settings-subtitle">
            {t(
              "Configure endpoints, credentials and the models each provider offers. One provider is active at a time.",
            )}
          </p>
        </div>
        <Show when={!narrow() || !detailOpen()}>
          <button class="btn primary" onClick={requestNew}>
            <IconPlus size={15} />
            {t("Add provider")}
          </button>
        </Show>
      </div>

      <Show when={error() && !draft()}>
        <div class="inline-error page-error">
          <IconCircleAlert size={14} />
          <span>{error()}</span>
        </div>
      </Show>

      <Show when={!loading()} fallback={<div class="loading">{t("Loading…")}</div>}>
        <div
          class="providers-layout"
          classList={{
            "detail-open": narrow() && (detailOpen() || visibleProviders().length === 0),
          }}
        >
          <div class="providers-list">
            <Show
              when={visibleProviders().length > 0}
              fallback={
                <Show when={!isNew()}>
                  <p class="providers-list-empty hint">{t("No providers yet.")}</p>
                </Show>
              }
            >
              <For each={visibleProviders()}>
                {(provider) => (
                  <button
                    classList={{
                      "provider-item": true,
                      active: !isNew() && provider.id === selectedId(),
                    }}
                    aria-current={!isNew() && provider.id === selectedId() ? "true" : undefined}
                    onClick={() => requestOpen(provider)}
                  >
                    <span class="provider-item-head">
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
                      <span>{tArgs("{count} models", { count: provider.models.length })}</span>
                      <span class="provider-sep">·</span>
                      <span>
                        {provider.auth === "none"
                          ? t("No authentication")
                          : provider.hasKey
                            ? t("Key configured")
                            : t("No key")}
                      </span>
                    </span>
                  </button>
                )}
              </For>
            </Show>
          </div>

          <div class="provider-detail">
            <Show when={narrow() && detailOpen() && !isNew()}>
              <button
                type="button"
                class="settings-back provider-back"
                onClick={() => void closeDetail()}
              >
                <IconArrowLeft size={15} />
                {t("Back")}
              </button>
            </Show>
            <Show when={pendingSwitch()}>
              <div class="draft-guard" role="alert">
                <span>{t("Discard unsaved changes?")}</span>
                <div class="draft-guard-actions">
                  <button class="btn danger" onClick={discardPending}>
                    {t("Discard")}
                  </button>
                  <button class="btn" onClick={() => setPendingSwitch(null)}>
                    {t("Cancel")}
                  </button>
                </div>
              </div>
            </Show>
            <Show
              when={draft()}
              fallback={
                <div class="providers-empty">
                  <p class="empty-lead">{t("No providers yet")}</p>
                  <p class="hint">
                    {t("Add a provider to call a model. The agent cannot run without one.")}
                  </p>
                  <button class="btn primary" onClick={requestNew}>
                    <IconPlus size={15} />
                    {t("Add provider")}
                  </button>
                </div>
              }
            >
              {(current) => (
                <ProviderEditor
                  selectionKey={isNew() ? `new-${newToken()}` : (selectedId() ?? "none")}
                  draft={current()}
                  view={selectedView()}
                  isNew={isNew()}
                  busy={busy()}
                  dirty={isDirty()}
                  error={error()}
                  notice={notice()}
                  testResult={testResult()}
                  onChange={setDraft}
                  onPersistDraft={handleContinue}
                  onSave={() => void handleSave()}
                  onSaveModels={handleSaveModels}
                  onTest={() => void handleTest()}
                  onDelete={() => void handleDelete()}
                  onActivate={() => void handleActivate()}
                  onCancelNew={() => void cancelNew()}
                />
              )}
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
