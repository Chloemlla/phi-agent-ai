// Provider editor. A new provider is a two-step wizard (connection, then model
// selection); an existing provider uses the same connection form plus a model
// list and a collapsible advanced section. All persistence goes through the
// page's callbacks; this component never writes files directly.

import {
  For,
  Index,
  Show,
  createEffect,
  createSignal,
  on,
  onMount,
} from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t, tArgs } from "../i18n";
import {
  IconCheck,
  IconCircleAlert,
  IconClose,
  IconEye,
  IconEyeOff,
  IconPlus,
  IconRefresh,
  IconTrash,
} from "../components/Icons";
import { formatTokens } from "../format";
import {
  cloneModel,
  mergeSuggestion,
  modelFromId,
  normalizeModelId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";

const PROTOCOL_OPTIONS = ["openai_chat", "openai_responses", "anthropic_messages"] as const;

function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

function entries(map: Record<string, string> | undefined): { key: string; value: string }[] {
  return Object.entries(map ?? {}).map(([key, value]) => ({ key, value }));
}

function formatWindow(value: number | null | undefined): string {
  return value != null && value > 0 ? formatTokens(value) : "";
}

function parseModelIds(raw: string): string[] {
  const seen = new Set<string>();
  const ids: string[] = [];
  for (const part of raw.split(/[\n,]+/)) {
    const id = part.trim();
    const key = normalizeModelId(id);
    if (!id || seen.has(key)) continue;
    seen.add(key);
    ids.push(id);
  }
  return ids;
}

function urlMessage(kind: "base" | "website"): string {
  return kind === "base"
    ? t("Base URL must start with http:// or https://.")
    : t("Website must start with http:// or https://.");
}

function urlError(raw: string, kind: "base" | "website"): string {
  const value = raw.trim();
  if (!value) return "";
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? ""
      : urlMessage(kind);
  } catch {
    return urlMessage(kind);
  }
}

type FieldErrors = {
  name?: string;
  baseUrl?: string;
  website?: string;
};

function validateConnection(draft: ProviderInput): FieldErrors {
  const errors: FieldErrors = {};
  if (!(draft.name ?? "").trim()) errors.name = t("Name is required.");
  if (!(draft.baseUrl ?? "").trim()) {
    errors.baseUrl = t("Base URL is required.");
  } else {
    const baseError = urlError(draft.baseUrl ?? "", "base");
    if (baseError) errors.baseUrl = baseError;
  }
  const websiteError = urlError(draft.websiteUrl ?? "", "website");
  if (websiteError) errors.website = websiteError;
  return errors;
}

export function protocolLabel(protocol: string): string {
  switch (protocol) {
    case "anthropic_messages":
      return t("Anthropic Messages");
    case "openai_chat":
      return t("OpenAI Chat Completions");
    case "openai_responses":
      return t("OpenAI Responses");
    default:
      return protocol;
  }
}

function WizardSteps(props: { step: 1 | 2 }) {
  return (
    <div class="wizard-steps" aria-label={t("Progress")}>
      <span
        classList={{
          "wizard-step": true,
          current: props.step === 1,
          done: props.step === 2,
        }}
      >
        1 {t("Connection")}
      </span>
      <span class="wizard-step-sep">—</span>
      <span
        classList={{
          "wizard-step": true,
          current: props.step === 2,
        }}
      >
        2 {t("Models")}
      </span>
    </div>
  );
}

export function ProviderEditor(props: {
  selectionKey: string;
  draft: ProviderInput;
  view: ProviderView | null;
  isNew: boolean;
  busy: boolean;
  error: string;
  notice: string;
  testResult: ProviderTestView | null;
  onChange: (draft: ProviderInput) => void;
  /** Persist the connection fields only (no model or models) for step 2. */
  onPersistDraft: () => Promise<ProviderView | null>;
  onSave: () => void;
  /** Save the selected models; returns true when the provider was persisted. */
  onSaveModels: (selected: ModelSpec[]) => Promise<boolean>;
  onTest: () => void;
  onDelete: () => void;
  onActivate: () => void;
  onCancelNew: () => void;
}) {
  const [step, setStep] = createSignal<1 | 2>(1);
  const [keyInput, setKeyInput] = createSignal("");
  const [showKey, setShowKey] = createSignal(false);
  const [headers, setHeaders] = createSignal<{ key: string; value: string }[]>(
    entries(props.draft.headers),
  );
  const [manualModel, setManualModel] = createSignal("");
  const [attempted, setAttempted] = createSignal(false);
  const [addingModel, setAddingModel] = createSignal(false);

  createEffect(
    on(
      () => props.selectionKey,
      () => {
        setStep(1);
        setKeyInput("");
        setShowKey(false);
        setHeaders(entries(props.draft.headers));
        setManualModel("");
        setAttempted(false);
      },
      { defer: true },
    ),
  );

  // A successful save changes updatedAt; never keep a typed secret on screen.
  createEffect(
    on(
      () => props.view?.updatedAt,
      () => {
        setKeyInput("");
        setShowKey(false);
      },
      { defer: true },
    ),
  );

  const models = () => props.draft.models ?? [];
  const providerId = () => props.draft.id ?? props.view?.id ?? "";
  const primaryId = () => (props.draft.model ?? "").trim() || models()[0]?.id || "";
  const errors = () => validateConnection(props.draft);
  const hasErrors = () => Object.keys(errors()).length > 0;

  function update(patch: Partial<ProviderInput>) {
    props.onChange({ ...props.draft, ...patch });
  }

  function isPrimary(model: ModelSpec): boolean {
    return normalizeModelId(primaryId()) === normalizeModelId(model.id);
  }

  function removeModel(index: number) {
    const removed = models()[index];
    const next = models().filter((_, i) => i !== index);
    const patch: Partial<ProviderInput> = { models: next };
    if (removed && normalizeModelId(removed.id) === normalizeModelId(props.draft.model ?? "")) {
      patch.model = next[0]?.id ?? "";
    }
    update(patch);
  }

  async function addManualModel() {
    const id = manualModel().trim();
    if (!id || addingModel()) return;
    const key = normalizeModelId(id);
    if (models().some((model) => normalizeModelId(model.id) === key)) {
      setManualModel("");
      return;
    }
    setAddingModel(true);
    let spec = modelFromId(id);
    try {
      const [suggested] = await invoke<ModelSpec[]>("describe_models", { modelIds: [id] });
      if (suggested) spec = mergeSuggestion(spec, suggested);
    } catch {
      // Catalogue metadata is best-effort; the id alone is a valid spec.
    }
    spec = { ...spec, enabled: true };
    const currentPrimary = (props.draft.model ?? "").trim();
    update({
      models: [...models(), spec],
      ...(currentPrimary ? {} : { model: spec.id }),
    });
    setManualModel("");
    setAddingModel(false);
  }

  function commitHeaders(next: { key: string; value: string }[]) {
    setHeaders(next);
    update({
      headers: Object.fromEntries(
        next
          .filter((row) => row.key.trim() !== "")
          .map((row) => [row.key.trim(), row.value]),
      ),
    });
  }

  function handleKeyInput(value: string) {
    setKeyInput(value);
    update({ apiKey: value });
  }

  function clearKey() {
    setKeyInput("");
    update({ apiKey: props.view?.keyHint ? "" : undefined });
  }

  async function handleContinue() {
    setAttempted(true);
    if (hasErrors()) return;
    const saved = await props.onPersistDraft();
    if (!saved) return;
    setKeyInput("");
    setShowKey(false);
    setStep(2);
  }

  function handleSave() {
    setAttempted(true);
    if (hasErrors()) return;
    props.onSave();
  }

  function handleTestConnection() {
    setAttempted(true);
    if (hasErrors()) return;
    props.onTest();
  }

  /// Persist the connection before previewing models so the fetch uses the
  /// endpoint and key currently shown in the form.
  async function handleFetchModels() {
    setAttempted(true);
    if (hasErrors()) return;
    const saved = await props.onPersistDraft();
    if (!saved) return;
    setKeyInput("");
    setShowKey(false);
    setStep(2);
  }

  function handleBack() {
    setAttempted(false);
    setStep(1);
  }

  return (
    <div class="provider-editor provider-form">
      <Show
        when={step() === 1}
        fallback={
          <ModelPicker
            providerId={providerId()}
            isWizard={props.isNew}
            existing={props.view?.models.map((model) => model.id) ?? []}
            busy={props.busy}
            error={props.error}
            onBack={handleBack}
            onCancel={props.onCancelNew}
            onSave={props.onSaveModels}
          />
        }
      >
        <div class="editor-head">
          <div class="editor-head-main">
            <Show
              when={props.isNew}
              fallback={
                <>
                  <h2>{props.view?.name || t("Provider")}</h2>
                  <div class="editor-meta">
                    <Show when={props.view?.active}>
                      <span class="provider-active">{t("Active")}</span>
                    </Show>
                    <span class="mono">
                      {protocolLabel(props.draft.protocol ?? "openai_chat")}
                    </span>
                  </div>
                </>
              }
            >
              <h2>{t("New provider")}</h2>
              <WizardSteps step={1} />
              <p class="hint">{t("Connect an endpoint, then choose the models to add.")}</p>
            </Show>
          </div>
        </div>

        <Show when={props.error || props.notice || props.testResult}>
          <div class="editor-status">
            <Show when={props.error}>
              <div class="inline-error">
                <IconCircleAlert size={14} />
                <span>{props.error}</span>
              </div>
            </Show>
            <Show when={props.notice}>
              <div class="settings-note">
                <IconCheck size={14} />
                <span>{props.notice}</span>
              </div>
            </Show>
            <Show when={props.testResult}>
              {(result) => (
                <div
                  classList={{
                    "inline-note": true,
                    ok: result().ok,
                    bad: !result().ok,
                  }}
                >
                  <Show when={result().ok} fallback={<IconCircleAlert size={14} />}>
                    <IconCheck size={14} />
                  </Show>
                  <span>
                    {result().ok
                      ? tArgs("Connected in {ms} ms, {count} models available.", {
                          ms: result().latencyMs,
                          count: result().modelCount,
                        })
                      : tArgs("Connection failed: {error}", {
                          error: result().error ?? errorText(result()),
                        })}
                  </span>
                </div>
              )}
            </Show>
          </div>
        </Show>

        <section class="editor-section">
          <h3>{t("Connection")}</h3>
          <div class="connection-fields">
            <label class="field">
              <span>{t("Name")}</span>
              <input
                value={props.draft.name ?? ""}
                onInput={(event) => update({ name: event.currentTarget.value })}
                placeholder={t("My provider")}
                autocomplete="off"
                required
                aria-invalid={attempted() && Boolean(errors().name)}
              />
              <Show when={attempted() && errors().name}>
                <span class="field-error">{errors().name}</span>
              </Show>
            </label>

            <label class="field">
              <span>{t("Notes")}</span>
              <input
                value={props.draft.notes ?? ""}
                onInput={(event) => update({ notes: event.currentTarget.value })}
                placeholder={t("Optional note, one line")}
                autocomplete="off"
              />
            </label>

            <label class="field">
              <span>{t("Website")}</span>
              <input
                value={props.draft.websiteUrl ?? ""}
                onInput={(event) => update({ websiteUrl: event.currentTarget.value })}
                placeholder="https://example.com"
                autocomplete="off"
                aria-invalid={attempted() && Boolean(errors().website)}
              />
              <Show when={attempted() && errors().website}>
                <span class="field-error">{errors().website}</span>
              </Show>
            </label>

            <label class="field">
              <span>{t("API key")}</span>
              <div class="field-row">
                <input
                  type={showKey() ? "text" : "password"}
                  value={keyInput()}
                  onInput={(event) => handleKeyInput(event.currentTarget.value)}
                  placeholder={
                    props.view?.keyHint
                      ? "••••••••"
                      : t("API key (optional)")
                  }
                  autocomplete="off"
                  spellcheck={false}
                />
                <button
                  type="button"
                  class="icon-btn"
                  title={showKey() ? t("Hide key") : t("Show key")}
                  aria-label={showKey() ? t("Hide key") : t("Show key")}
                  onClick={() => setShowKey((shown) => !shown)}
                >
                  <Show when={showKey()} fallback={<IconEye size={16} />}>
                    <IconEyeOff size={16} />
                  </Show>
                </button>
              </div>
              <Show when={props.view?.keyHint}>
                <span class="field-hint mono">
                  {tArgs("Current key: {hint}", { hint: props.view?.keyHint ?? "" })}
                </span>
                <span class="field-hint">{t("Leave empty to keep the current key.")}</span>
              </Show>
              <Show when={!props.view?.keyHint}>
                <span class="field-hint">
                  {t(
                    "Optional. Leave empty for endpoints that do not require authentication.",
                  )}
                </span>
              </Show>
              <Show when={props.isNew && props.view?.keyHint}>
                <button type="button" class="link-button" onClick={clearKey}>
                  {t("Clear key")}
                </button>
              </Show>
              <Show when={props.view?.keyHint && props.draft.apiKey === ""}>
                <span class="field-hint danger-text">
                  {t("The key will be cleared when you save.")}
                </span>
                <button
                  type="button"
                  class="link-button"
                  onClick={() => update({ apiKey: undefined })}
                >
                  {t("Keep current key")}
                </button>
              </Show>
            </label>

            <label class="field">
              <span>{t("Base URL")}</span>
              <input
                class="mono"
                value={props.draft.baseUrl ?? ""}
                onInput={(event) => update({ baseUrl: event.currentTarget.value })}
                placeholder="https://api.example.com/v1"
                autocomplete="off"
                spellcheck={false}
                required
                aria-invalid={attempted() && Boolean(errors().baseUrl)}
              />
              <Show when={attempted() && errors().baseUrl}>
                <span class="field-error">{errors().baseUrl}</span>
              </Show>
            </label>

            <label class="field">
              <span>{t("Request format")}</span>
              <select
                value={props.draft.protocol ?? "openai_chat"}
                onChange={(event) => update({ protocol: event.currentTarget.value })}
              >
                <For each={PROTOCOL_OPTIONS}>
                  {(protocol) => <option value={protocol}>{protocolLabel(protocol)}</option>}
                </For>
              </select>
            </label>
          </div>
        </section>

        <Show when={!props.isNew}>
          <section class="editor-section">
            <div class="section-head">
              <h3>{t("Models")}</h3>
              <button
                class="btn"
                disabled={props.busy || !providerId()}
                onClick={() => void handleFetchModels()}
              >
                <IconRefresh size={14} />
                {t("Fetch models")}
              </button>
            </div>

            <Show
              when={models().length > 0}
              fallback={
                <p class="hint">
                  {t("No models yet. Add one manually or fetch the provider's model list.")}
                </p>
              }
            >
              <ul class="provider-model-list">
                <For each={models()}>
                  {(model, index) => (
                    <li class="provider-model-row">
                      <span class="provider-model-main">
                        <span class="provider-model-id mono" title={model.id}>
                          {model.id}
                        </span>
                        <Show when={model.name && model.name !== model.id}>
                          <span class="provider-model-name" title={model.name}>
                            {model.name}
                          </span>
                        </Show>
                      </span>
                      <Show when={isPrimary(model)}>
                        <span class="model-tag primary">{t("Primary")}</span>
                      </Show>
                      <span class="model-tag" title={t("Model scale (1-5)")}>
                        S{model.scale ?? 3}
                      </span>
                      <span
                        class="provider-model-ctx mono"
                        title={formatWindow(model.contextWindow)}
                      >
                        {formatWindow(model.contextWindow) || "—"}
                      </span>
                      <button
                        class="icon-btn"
                        title={t("Remove model")}
                        aria-label={t("Remove model")}
                        disabled={props.busy}
                        onClick={() => removeModel(index())}
                      >
                        <IconTrash size={14} />
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            </Show>

            <form
              class="model-add-row"
              onSubmit={(event) => {
                event.preventDefault();
                void addManualModel();
              }}
            >
              <input
                class="mono"
                value={manualModel()}
                onInput={(event) => setManualModel(event.currentTarget.value)}
                placeholder={t("Add model by id")}
                autocomplete="off"
                spellcheck={false}
              />
              <button
                class="btn"
                type="submit"
                disabled={props.busy || addingModel() || !manualModel().trim()}
              >
                <IconPlus size={14} />
                {t("Add model")}
              </button>
            </form>
          </section>

          <details class="editor-section collapsible">
            <summary>{t("Advanced")}</summary>
            <div class="advanced-body">
              <h4 class="advanced-title">{t("Custom headers")}</h4>
              <p class="hint">
                {t(
                  "Headers are sent with every request. Authorization and x-api-key are managed by the gateway and cannot be overridden here.",
                )}
              </p>
              <div class="map-editor">
                <Index each={headers()}>
                  {(header, index) => (
                    <div class="map-row">
                      <input
                        class="mono"
                        value={header().key}
                        onInput={(event) => {
                          const next = headers().map((row, i) =>
                            i === index ? { ...row, key: event.currentTarget.value } : row,
                          );
                          commitHeaders(next);
                        }}
                        placeholder={t("Header")}
                        autocomplete="off"
                        spellcheck={false}
                      />
                      <input
                        class="mono"
                        value={header().value}
                        onInput={(event) => {
                          const next = headers().map((row, i) =>
                            i === index ? { ...row, value: event.currentTarget.value } : row,
                          );
                          commitHeaders(next);
                        }}
                        placeholder={t("Value")}
                        autocomplete="off"
                        spellcheck={false}
                      />
                      <button
                        type="button"
                        class="icon-btn"
                        title={t("Remove header")}
                        aria-label={t("Remove header")}
                        onClick={() =>
                          commitHeaders(headers().filter((_, i) => i !== index))
                        }
                      >
                        <IconClose size={14} />
                      </button>
                    </div>
                  )}
                </Index>
                <button
                  type="button"
                  class="btn"
                  onClick={() => setHeaders([...headers(), { key: "", value: "" }])}
                >
                  <IconPlus size={14} />
                  {t("Add header")}
                </button>
              </div>
              <Show when={props.view?.keyHint || (props.draft.apiKey ?? "") !== ""}>
                <div class="advanced-actions">
                  <button type="button" class="link-button" onClick={clearKey}>
                    {t("Clear key")}
                  </button>
                </div>
              </Show>
            </div>
          </details>
        </Show>

        <div class="editor-foot">
          <div class="editor-foot-group">
            <Show
              when={props.isNew}
              fallback={
                <>
                  <button class="btn primary" disabled={props.busy} onClick={handleSave}>
                    {props.busy ? t("Saving…") : t("Save")}
                  </button>
                  <button class="btn" disabled={props.busy} onClick={handleTestConnection}>
                    {t("Test connection")}
                  </button>
                  <Show when={props.view && !props.view.active && primaryId()}>
                    <button class="btn" disabled={props.busy} onClick={props.onActivate}>
                      {t("Set as active")}
                    </button>
                  </Show>
                </>
              }
            >
              <button class="btn" disabled={props.busy} onClick={props.onCancelNew}>
                {t("Cancel")}
              </button>
              <button
                class="btn primary"
                disabled={props.busy}
                onClick={() => void handleContinue()}
              >
                {props.busy ? t("Saving…") : t("Continue")}
              </button>
            </Show>
          </div>
          <Show when={!props.isNew}>
            <button class="btn danger" disabled={props.busy} onClick={props.onDelete}>
              <IconTrash size={14} />
              {t("Delete provider")}
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function ModelPicker(props: {
  providerId: string;
  isWizard: boolean;
  /** Model ids already stored on the provider; pre-checked when editing. */
  existing: string[];
  busy: boolean;
  error: string;
  onBack: () => void;
  onCancel: () => void;
  onSave: (selected: ModelSpec[]) => Promise<boolean>;
}) {
  const [rows, setRows] = createSignal<{ id: string; spec: ModelSpec }[]>([]);
  const [checked, setChecked] = createSignal<Record<string, boolean>>({});
  const [loading, setLoading] = createSignal(true);
  const [fetchError, setFetchError] = createSignal("");
  const [query, setQuery] = createSignal("");
  const [manual, setManual] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  let loadToken = 0;

  async function load() {
    const token = ++loadToken;
    setLoading(true);
    setFetchError("");
    try {
      const ids = await invoke<string[]>("preview_models", {
        providerId: props.providerId,
      });
      if (token !== loadToken) return;
      const seen = new Set<string>();
      const unique: string[] = [];
      for (const raw of ids) {
        const id = raw.trim();
        const key = normalizeModelId(id);
        if (!id || seen.has(key)) continue;
        seen.add(key);
        unique.push(id);
      }
      let described: ModelSpec[] = [];
      try {
        described = await invoke<ModelSpec[]>("describe_models", { modelIds: unique });
      } catch {
        // Mapping is advisory; every model still works with default settings.
      }
      if (token !== loadToken) return;
      const byKey = new Map<string, ModelSpec>();
      for (const spec of described) byKey.set(normalizeModelId(spec.id), spec);
      const nextRows = unique.map((id) => ({
        id,
        spec: byKey.get(normalizeModelId(id)) ?? modelFromId(id),
      }));
      setRows(nextRows);
      const kept = new Set(props.existing.map(normalizeModelId));
      const nextChecked: Record<string, boolean> = {};
      for (const row of nextRows) {
        const key = normalizeModelId(row.id);
        // A new provider starts with every fetched model selected. When
        // editing, the list mirrors what is already configured so saving
        // only adds the models the user newly checks.
        nextChecked[key] = props.isWizard || kept.has(key);
      }
      setChecked(nextChecked);
    } catch (error) {
      if (token !== loadToken) return;
      setRows([]);
      setChecked({});
      setFetchError(errorText(error));
    } finally {
      if (token === loadToken) setLoading(false);
    }
  }

  onMount(() => {
    void load();
  });

  const visibleRows = () => {
    const text = query().trim().toLowerCase();
    if (!text) return rows();
    return rows().filter(
      (row) =>
        row.id.toLowerCase().includes(text) ||
        row.spec.name.toLowerCase().includes(text),
    );
  };

  const selectedCount = () =>
    rows().filter((row) => checked()[normalizeModelId(row.id)]).length;

  const selectedSpecs = (): ModelSpec[] =>
    rows()
      .filter((row) => checked()[normalizeModelId(row.id)])
      .map((row) => ({ ...cloneModel(row.spec), id: row.id, enabled: true }));

  function setAllVisible(value: boolean) {
    const next = { ...checked() };
    for (const row of visibleRows()) next[normalizeModelId(row.id)] = value;
    setChecked(next);
  }

  function toggle(id: string, value: boolean) {
    const key = normalizeModelId(id);
    setChecked({ ...checked(), [key]: value });
  }

  async function addManual() {
    const ids = parseModelIds(manual());
    if (ids.length === 0) return;
    const known = new Set(rows().map((row) => normalizeModelId(row.id)));
    const fresh = ids.filter((id) => !known.has(normalizeModelId(id)));
    if (fresh.length === 0) {
      setManual("");
      return;
    }
    let described: ModelSpec[] = [];
    try {
      described = await invoke<ModelSpec[]>("describe_models", { modelIds: fresh });
    } catch {
      // Id-only specs are valid; the backend enriches unknown models.
    }
    const byKey = new Map<string, ModelSpec>();
    for (const spec of described) byKey.set(normalizeModelId(spec.id), spec);
    const nextRows = [
      ...rows(),
      ...fresh.map((id) => ({
        id,
        spec: byKey.get(normalizeModelId(id)) ?? modelFromId(id),
      })),
    ];
    setRows(nextRows);
    const nextChecked = { ...checked() };
    for (const id of fresh) nextChecked[normalizeModelId(id)] = true;
    setChecked(nextChecked);
    setManual("");
  }

  async function handleSave() {
    const selected = selectedSpecs();
    if (selected.length === 0 || saving()) return;
    setSaving(true);
    try {
      const saved = await props.onSave(selected);
      if (saved) props.onBack();
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <div class="editor-head">
        <div class="editor-head-main">
          <h2>{t("Models")}</h2>
          <Show when={props.isWizard}>
            <WizardSteps step={2} />
          </Show>
          <p class="hint">
            {t("Select the models to add to this provider.")}
          </p>
        </div>
      </div>

      <Show when={props.error}>
        <div class="editor-status">
          <div class="inline-error">
            <IconCircleAlert size={14} />
            <span>{props.error}</span>
          </div>
        </div>
      </Show>

      <section class="editor-section">
        <Show when={!props.isWizard}>
          <p class="hint">
            {t("Existing models are kept; selected models are added.")}
          </p>
        </Show>

        <Show when={loading()}>
          <div class="picker-loading">
            <span class="ring" aria-hidden="true" />
            <span>{t("Fetching models…")}</span>
          </div>
        </Show>

        <Show when={!loading()}>
          <p class="hint">{t("Models without a mapping use default settings.")}</p>

          <div class="picker-toolbar">
            <input
              class="settings-input picker-search"
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
              placeholder={t("Search models")}
              autocomplete="off"
            />
            <div class="picker-actions">
              <button
                type="button"
                class="link-button"
                disabled={visibleRows().length === 0}
                onClick={() => setAllVisible(true)}
              >
                {t("Select all")}
              </button>
              <button
                type="button"
                class="link-button"
                disabled={rows().length === 0}
                onClick={() => setAllVisible(false)}
              >
                {t("Select none")}
              </button>
            </div>
          </div>

          <p class="picker-count hint">
            {tArgs("{selected} of {total} selected", {
              selected: selectedCount(),
              total: rows().length,
            })}
          </p>

          <Show when={fetchError()}>
            <div class="picker-failure">
              <div class="inline-error">
                <IconCircleAlert size={14} />
                <span>{fetchError()}</span>
              </div>
              <button class="btn" onClick={() => void load()}>
                <IconRefresh size={14} />
                {t("Retry")}
              </button>
            </div>
          </Show>

          <Show
            when={!fetchError() && rows().length === 0}
            fallback={
              <div class="model-pick-list">
                <For each={visibleRows()}>
                  {(row) => {
                    const key = normalizeModelId(row.id);
                    const mapped = () => row.spec.source === "catalog";
                    const name = () =>
                      row.spec.name && row.spec.name !== row.id ? row.spec.name : "";
                    return (
                      <label class="model-pick-row">
                        <input
                          type="checkbox"
                          checked={Boolean(checked()[key])}
                          onChange={(event) => toggle(row.id, event.currentTarget.checked)}
                        />
                        <span class="model-pick-main">
                          <span class="model-pick-id mono" title={row.id}>
                            {row.id}
                          </span>
                          <Show when={name()}>
                            <span class="model-pick-name" title={name()}>
                              {name()}
                            </span>
                          </Show>
                        </span>
                        <span class="model-tag" title={t("Model scale (1-5)")}>
                          S{row.spec.scale ?? 3}
                        </span>
                        <span
                          class="model-pick-ctx mono"
                          title={formatWindow(row.spec.contextWindow)}
                        >
                          {formatWindow(row.spec.contextWindow) || "—"}
                        </span>
                        <span
                          classList={{
                            "model-pick-badge": true,
                            mapped: mapped(),
                          }}
                        >
                          {mapped() ? t("Mapped") : t("Defaults")}
                        </span>
                      </label>
                    );
                  }}
                </For>
              </div>
            }
          >
            <p class="hint">{t("The provider returned no models.")}</p>
          </Show>

          <Show when={!loading() && (Boolean(fetchError()) || rows().length === 0)}>
            <div class="picker-manual">
              <p class="hint">{t("Add model IDs manually")}</p>
              <textarea
                class="mono"
                rows={2}
                value={manual()}
                onInput={(event) => setManual(event.currentTarget.value)}
                placeholder={t("Comma or newline separated")}
                spellcheck={false}
              />
              <button
                class="btn"
                disabled={!manual().trim()}
                onClick={() => void addManual()}
              >
                <IconPlus size={14} />
                {t("Add manually")}
              </button>
            </div>
          </Show>
        </Show>
      </section>

      <div class="editor-foot">
        <div class="editor-foot-group">
          <Show when={props.isWizard}>
            <button class="btn" disabled={saving() || props.busy} onClick={props.onCancel}>
              {t("Cancel")}
            </button>
          </Show>
        </div>
        <div class="editor-foot-group">
          <button class="btn" disabled={saving() || props.busy} onClick={props.onBack}>
            {t("Back")}
          </button>
          <button
            class="btn primary"
            disabled={saving() || props.busy || selectedCount() === 0}
            onClick={() => void handleSave()}
          >
            {saving() || props.busy
              ? t("Saving…")
              : tArgs("Add {count} models", { count: selectedCount() })}
          </button>
        </div>
      </div>
    </>
  );
}
