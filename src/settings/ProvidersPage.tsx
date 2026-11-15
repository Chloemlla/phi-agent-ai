// Providers page: list + editor orchestration. All persistence goes through
// the node's provider commands; the page never writes files directly.

import { For, Show, createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t, tArgs } from "../i18n";
import { IconPlus, IconServer } from "../components/Icons";
import { ProviderEditor, protocolLabel } from "./ProviderEditor";
import { PRESETS, type ProviderPreset } from "./presets";
import {
  cloneModel,
  modelFromId,
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
    modelMap: { ...(view.modelMap ?? {}) },
    auth: view.auth,
    headers: { ...(view.headers ?? {}) },
    pricing: { ...view.pricing },
    maxOutputTokens: view.maxOutputTokens,
    temperature: view.temperature,
    enabled: view.enabled,
    sortIndex: view.sortIndex,
    websiteUrl: view.websiteUrl ?? "",
    notes: view.notes ?? "",
    // apiKey is intentionally omitted so the stored secret is preserved.
  };
}

function emptyDraft(): ProviderInput {
  return {
    name: "",
    protocol: "anthropic_messages",
    baseUrl: "",
    model: "",
    models: [],
    modelMap: {},
    auth: "x_api_key",
    headers: {},
    enabled: true,
    websiteUrl: "",
    notes: "",
  };
}

function buildInput(draft: ProviderInput, isNew: boolean): ProviderInput {
  const input: ProviderInput = {
    name: draft.name ?? "",
    protocol: draft.protocol,
    baseUrl: draft.baseUrl ?? "",
    model: draft.model ?? "",
    models: draft.models ?? [],
    modelMap: draft.modelMap ?? {},
    auth: draft.auth,
    headers: draft.headers ?? {},
    enabled: draft.enabled ?? true,
    sortIndex: draft.sortIndex,
    websiteUrl: draft.websiteUrl ?? "",
    notes: draft.notes ?? "",
  };
  if (!isNew && draft.id) input.id = draft.id;
  if (draft.pricing) {
    input.pricing = draft.pricing;
  } else {
    input.clearPricing = true;
  }
  if (draft.maxOutputTokens != null) {
    input.maxOutputTokens = draft.maxOutputTokens;
  } else {
    input.clearMaxOutputTokens = true;
  }
  if (draft.temperature != null) {
    input.temperature = draft.temperature;
  } else {
    input.clearTemperature = true;
  }
  if (draft.apiKey !== undefined) input.apiKey = draft.apiKey;
  return input;
}

export function ProvidersPage(props: { onRefresh: () => Promise<void> }) {
  const [providers, setProviders] = createSignal<ProviderView[]>([]);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal<ProviderInput | null>(null);
  const [isNew, setIsNew] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [testResult, setTestResult] = createSignal<ProviderTestView | null>(null);

  function selectView(view: ProviderView) {
    setSelectedId(view.id);
    setIsNew(false);
    setDraft(viewToDraft(view));
  }

  async function load(preferredId?: string) {
    const list = await invoke<ProviderView[]>("list_providers");
    setProviders(list);
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

  function startNew() {
    setSelectedId(null);
    setIsNew(true);
    setDraft(emptyDraft());
    setTestResult(null);
    setError("");
    setNotice("");
  }

  function cancelNew() {
    setIsNew(false);
    setError("");
    setNotice("");
    void load().catch((e) => setError(errorText(e)));
  }

  async function applyPreset(preset: ProviderPreset) {
    let specs: ModelSpec[] = preset.models.map(modelFromId);
    if (preset.models.length > 0) {
      try {
        specs = await invoke<ModelSpec[]>("describe_models", {
          modelIds: preset.models,
        });
      } catch {
        // Keep id-only rows; save-time enrichment runs in the node.
      }
    }
    setSelectedId(null);
    setIsNew(true);
    setTestResult(null);
    setError("");
    setNotice("");
    setDraft({
      name: preset.name,
      protocol: preset.protocol,
      baseUrl: preset.baseUrl,
      auth: preset.auth,
      model: specs[0]?.id ?? "",
      models: specs,
      modelMap: {},
      headers: {},
      enabled: true,
      websiteUrl: preset.websiteUrl ?? "",
      notes: "",
    });
  }

  /// Persist the current draft. Returns the saved view, or null when the node
  /// rejected it. Busy state stays with the caller.
  async function persist(): Promise<ProviderView | null> {
    const current = draft();
    if (!current) return null;
    setError("");
    setNotice("");
    try {
      const saved = await invoke<ProviderView>("save_provider", {
        input: buildInput(current, isNew()),
      });
      setSelectedId(saved.id);
      setIsNew(false);
      setDraft(viewToDraft(saved));
      setTestResult(null);
      await load(saved.id);
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
    await persist();
    setBusy(false);
  }

  async function handleTest() {
    setBusy(true);
    try {
      const saved = await persist();
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

  async function handleFetch() {
    setBusy(true);
    try {
      const saved = await persist();
      if (!saved) return;
      const ids = await invoke<string[]>("fetch_models", { providerId: saved.id });
      await load(saved.id);
      setNotice(
        ids.length > 0
          ? tArgs("Fetched {count} models.", { count: ids.length })
          : t("The provider returned no models."),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleActivate() {
    const id = selectedId();
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
    const id = selectedId();
    const view = providers().find((provider) => provider.id === id);
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
    <div class="settings-section providers-page">
      <div class="settings-page-head">
        <div>
          <h1>{t("Model providers")}</h1>
          <p class="settings-subtitle">
            {t(
              "Configure endpoints, credentials and the models each provider offers. One provider is active at a time.",
            )}
          </p>
        </div>
        <button class="btn primary" onClick={startNew}>
          <IconPlus size={15} />
          {t("Add provider")}
        </button>
      </div>

      <Show when={!loading()} fallback={<div class="loading">{t("Loading…")}</div>}>
        <div class="providers-layout">
          <div class="providers-list">
            <Show
              when={providers().length > 0}
              fallback={
                <div class="providers-list-empty">
                  <p class="hint">{t("No providers yet.")}</p>
                </div>
              }
            >
              <For each={providers()}>
                {(provider) => (
                  <button
                    classList={{
                      "provider-item": true,
                      active: !isNew() && provider.id === selectedId(),
                    }}
                    onClick={() => {
                      setTestResult(null);
                      setError("");
                      setNotice("");
                      selectView(provider);
                    }}
                  >
                    <span class="provider-item-head">
                      <span class="provider-name">{provider.name}</span>
                      <Show when={provider.active}>
                        <span class="provider-active-badge">{t("Active")}</span>
                      </Show>
                    </span>
                    <span class="provider-meta">
                      <span class="mono">{protocolLabel(provider.protocol)}</span>
                      <span>·</span>
                      <span>{tArgs("{count} models", { count: provider.models.length })}</span>
                    </span>
                    <span class="provider-meta">
                      <span class={provider.hasKey ? "dot tone-ok" : "dot tone-warn"} />
                      <span>{provider.hasKey ? t("Key configured") : t("No key")}</span>
                      <Show when={!provider.enabled}>
                        <span class="provider-disabled">{t("Disabled")}</span>
                      </Show>
                    </span>
                  </button>
                )}
              </For>
            </Show>
            <Show when={PRESETS.length > 0 && providers().length === 0 && !isNew()}>
              <button class="link-button providers-start" onClick={startNew}>
                {t("Start from a preset")}
              </button>
            </Show>
          </div>

          <div class="provider-detail">
            <Show
              when={draft()}
              fallback={
                <div class="providers-empty">
                  <div class="empty-icon">
                    <IconServer size={24} />
                  </div>
                  <h2>{t("No providers yet")}</h2>
                  <p class="hint">
                    {t(
                      "Add a provider to call a real model. Without one, runs use the built-in demo model.",
                    )}
                  </p>
                  <button class="btn primary" onClick={startNew}>
                    <IconPlus size={15} />
                    {t("Add provider")}
                  </button>
                </div>
              }
            >
              {(current) => (
                <ProviderEditor
                  selectionKey={isNew() ? "new" : selectedId() ?? "none"}
                  draft={current()}
                  view={providers().find((provider) => provider.id === selectedId()) ?? null}
                  isNew={isNew()}
                  busy={busy()}
                  error={error()}
                  notice={notice()}
                  testResult={testResult()}
                  onChange={setDraft}
                  onSave={() => void handleSave()}
                  onTest={() => void handleTest()}
                  onFetch={() => void handleFetch()}
                  onDelete={() => void handleDelete()}
                  onActivate={() => void handleActivate()}
                  onCancelNew={cancelNew}
                  onPreset={(preset) => void applyPreset(preset)}
                />
              )}
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
