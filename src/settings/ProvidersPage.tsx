// Providers page: list + editor orchestration. All persistence goes through
// the node's provider commands; the page never writes files directly.
//
// A new provider is a two-step wizard. Step 1 saves the connection fields
// without a model so the node can preview the live model list; step 2 saves
// only the models the user selected. If a draft provider was created by the
// wizard and the user leaves before finishing, the page removes that empty
// draft so the provider list has no residue.

import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t, tArgs } from "../i18n";
import { IconCircleAlert, IconPlus } from "../components/Icons";
import { ProviderEditor, protocolLabel } from "./ProviderEditor";
import {
  cloneModel,
  normalizeModelId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";

function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

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
      (draft.protocol ?? "openai_chat") === "anthropic_messages"
        ? "x_api_key"
        : "bearer";
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

export function ProvidersPage(props: { onRefresh: () => Promise<void> }) {
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

  const selectedView = () =>
    providers().find((provider) => provider.id === selectedId()) ?? null;

  const visibleProviders = () => {
    if (!isNew()) return providers();
    const draftId = draft()?.id;
    return draftId
      ? providers().filter((provider) => provider.id !== draftId)
      : providers();
  };

  function selectView(view: ProviderView) {
    setSelectedId(view.id);
    setIsNew(false);
    setCreatedInWizard(false);
    setDraft(viewToDraft(view));
  }

  async function refreshList(): Promise<ProviderView[]> {
    const list = await invoke<ProviderView[]>("list_providers");
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

  // Leaving Settings (or the app) with an unfinished wizard must not leave a
  // zero-model draft behind.
  onCleanup(() => {
    const id = isNew() && createdInWizard() ? draft()?.id : undefined;
    if (id) {
      void invoke("remove_provider", { providerId: id }).catch(() => undefined);
    }
  });

  async function startNew() {
    const staleId = isNew() && createdInWizard() ? draft()?.id : undefined;
    if (staleId) {
      try {
        await invoke("remove_provider", { providerId: staleId });
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
    if (isNew() && createdInWizard()) {
      const staleId = draft()?.id;
      if (staleId && staleId !== view.id) {
        try {
          await invoke("remove_provider", { providerId: staleId });
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
    const cleanupId =
      createdInWizard() ? draft()?.id ?? selectedId() : null;
    setIsNew(false);
    setCreatedInWizard(false);
    setSelectedId(null);
    setDraft(null);
    setTestResult(null);
    setError("");
    setNotice("");
    if (cleanupId) {
      try {
        await invoke("remove_provider", { providerId: cleanupId });
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
      const saved = await invoke<ProviderView>("save_provider", { input });
      if (!current.id) setCreatedInWizard(true);
      upsertProvider(saved);
      setSelectedId(saved.id);
      // The secret is now stored; never keep the typed value in the draft.
      setDraft({ ...current, id: saved.id, apiKey: undefined });
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      return saved;
    } catch (e) {
      setError(errorText(e));
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

      const saved = await invoke<ProviderView>("save_provider", { input });
      upsertProvider(saved);
      setSelectedId(saved.id);
      setIsNew(false);
      setCreatedInWizard(false);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider saved."));
      return saved;
    } catch (e) {
      setError(errorText(e));
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
          merged.some(
            (model) =>
              normalizeModelId(model.id) === normalizeModelId(currentPrimary),
          )
            ? currentPrimary
            : selectedPrimary || merged[0]?.id || "";
      }

      const input = buildConnection(current, selectedView(), true);
      input.id = id;
      input.models = merged;
      input.model = primary;

      const saved = await invoke<ProviderView>("save_provider", { input });
      if (wasNew) {
        // The user just finished onboarding this provider; make it the one the
        // composer uses instead of silently keeping the previous provider.
        try {
          await invoke("set_active_provider", { providerId: saved.id });
        } catch {
          // Activation can fail for a keyless remote provider; the explicit
          // "Set as active" action remains available.
        }
      }
      upsertProvider(saved);
      setSelectedId(saved.id);
      setIsNew(false);
      setCreatedInWizard(false);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await refreshList();
      await props.onRefresh().catch(() => undefined);
      setNotice(
        wasNew
          ? tArgs("Added {count} models.", { count: wanted.length })
          : t("Provider saved."),
      );
      return true;
    } catch (e) {
      setError(errorText(e));
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
      const result = await invoke<ProviderTestView>("test_provider", {
        providerId: saved.id,
      });
      setTestResult(result);
    } catch (e) {
      setTestResult({ ok: false, latencyMs: 0, modelCount: 0, error: errorText(e) });
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
      await invoke("set_active_provider", { providerId: id });
      await load(id);
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider is now active."));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    const view = selectedView();
    if (!view) return;
    if (
      !window.confirm(
        tArgs("Delete provider {name}? This cannot be undone.", { name: view.name }),
      )
    ) {
      return;
    }
    setBusy(true);
    setError("");
    try {
      await invoke("remove_provider", { providerId: view.id });
      setSelectedId(null);
      setDraft(null);
      setIsNew(false);
      setCreatedInWizard(false);
      setTestResult(null);
      await load();
      await props.onRefresh().catch(() => undefined);
      setNotice(t("Provider deleted."));
    } catch (e) {
      setError(errorText(e));
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
        <button class="btn primary" onClick={() => void startNew()}>
          <IconPlus size={15} />
          {t("Add provider")}
        </button>
      </div>

      <Show when={error() && !draft()}>
        <div class="inline-error page-error">
          <IconCircleAlert size={14} />
          <span>{error()}</span>
        </div>
      </Show>

      <Show when={!loading()} fallback={<div class="loading">{t("Loading…")}</div>}>
        <div class="providers-layout">
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
                    aria-current={
                      !isNew() && provider.id === selectedId()
                        ? "true"
                        : undefined
                    }
                    onClick={() => void openProvider(provider)}
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
                      <span>
                        {tArgs("{count} models", { count: provider.models.length })}
                      </span>
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
            <Show
              when={draft()}
              fallback={
                <div class="providers-empty">
                  <p class="empty-lead">{t("No providers yet")}</p>
                  <p class="hint">
                    {t("Add a provider to call a model. The agent cannot run without one.")}
                  </p>
                  <button class="btn primary" onClick={() => void startNew()}>
                    <IconPlus size={15} />
                    {t("Add provider")}
                  </button>
                </div>
              }
            >
              {(current) => (
                <ProviderEditor
                  selectionKey={
                    isNew() ? `new-${newToken()}` : selectedId() ?? "none"
                  }
                  draft={current()}
                  view={selectedView()}
                  isNew={isNew()}
                  busy={busy()}
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
