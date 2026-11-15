// Provider editor: identity, credentials, models (auto-configured by model
// name), role mappings, custom headers and the provider-wide defaults.

import { For, Index, Show, createEffect, createSignal, on } from "solid-js";
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
import {
  mergeSuggestion,
  modelFromId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";
import { PRESETS, type ProviderPreset } from "./presets";

function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

function numberOrNull(value: string): number | null {
  if (value.trim() === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
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

export function authLabel(auth: string): string {
  switch (auth) {
    case "x_api_key":
      return t("API key header (x-api-key)");
    case "bearer":
      return t("Bearer token");
    case "none":
      return t("No authentication");
    default:
      return auth;
  }
}

function sourceLabel(source: string): string {
  switch (source) {
    case "catalog":
      return t("Auto");
    case "fetched":
      return t("Fetched");
    default:
      return t("Manual");
  }
}

function entries(map: Record<string, string> | undefined): { key: string; value: string }[] {
  return Object.entries(map ?? {}).map(([key, value]) => ({ key, value }));
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
  onSave: () => void;
  onTest: () => void;
  onFetch: () => void;
  onDelete: () => void;
  onActivate: () => void;
  onCancelNew: () => void;
  onPreset: (preset: ProviderPreset) => void;
}) {
  const [keyInput, setKeyInput] = createSignal("");
  const [showKey, setShowKey] = createSignal(false);
  const [roles, setRoles] = createSignal<{ key: string; value: string }[]>(
    entries(props.draft.modelMap),
  );
  const [headers, setHeaders] = createSignal<{ key: string; value: string }[]>(
    entries(props.draft.headers),
  );

  createEffect(
    on(
      () => props.selectionKey,
      () => {
        setKeyInput("");
        setShowKey(false);
        setRoles(entries(props.draft.modelMap));
        setHeaders(entries(props.draft.headers));
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
  const modelIds = () => models().map((model) => model.id).filter(Boolean);

  function update(patch: Partial<ProviderInput>) {
    props.onChange({ ...props.draft, ...patch });
  }

  function updateModel(index: number, patch: Partial<ModelSpec>) {
    const next = models().map((model, i) => (i === index ? { ...model, ...patch } : model));
    update({ models: next });
  }

  function removeModel(index: number) {
    const removed = models()[index];
    const next = models().filter((_, i) => i !== index);
    const patch: Partial<ProviderInput> = { models: next };
    if (removed && props.draft.model === removed.id) {
      patch.model = next[0]?.id ?? "";
    }
    update(patch);
  }

  function addModel() {
    update({ models: [...models(), modelFromId("")] });
  }

  async function suggestModel(index: number) {
    const model = models()[index];
    if (!model || !model.id.trim()) return;
    try {
      const [suggested] = await invoke<ModelSpec[]>("describe_models", {
        modelIds: [model.id.trim()],
      });
      if (suggested) updateModel(index, mergeSuggestion(models()[index], suggested));
    } catch {
      // Suggestion is best-effort; save-time enrichment still runs.
    }
  }

  async function suggestPrimary() {
    const id = (props.draft.model ?? "").trim();
    if (!id || models().some((model) => model.id === id)) return;
    try {
      const [suggested] = await invoke<ModelSpec[]>("describe_models", { modelIds: [id] });
      update({ models: [...models(), suggested ?? modelFromId(id)] });
    } catch {
      update({ models: [...models(), modelFromId(id)] });
    }
  }

  function updatePricing(side: "inputPerMtok" | "outputPerMtok", raw: string) {
    const current = props.draft.pricing ?? { inputPerMtok: 0, outputPerMtok: 0 };
    const value = numberOrNull(raw);
    const next = {
      inputPerMtok: side === "inputPerMtok" ? value ?? 0 : current.inputPerMtok,
      outputPerMtok: side === "outputPerMtok" ? value ?? 0 : current.outputPerMtok,
    };
    update({ pricing: next });
  }

  function updateModelPricing(
    index: number,
    side: "inputPerMtok" | "outputPerMtok",
    raw: string,
  ) {
    const model = models()[index];
    if (!model) return;
    const current = model.pricing ?? { inputPerMtok: 0, outputPerMtok: 0 };
    const value = numberOrNull(raw);
    const next = {
      inputPerMtok: side === "inputPerMtok" ? value ?? 0 : current.inputPerMtok,
      outputPerMtok: side === "outputPerMtok" ? value ?? 0 : current.outputPerMtok,
    };
    updateModel(index, { pricing: next });
  }

  function clearModelPricing(index: number) {
    // Keep the cleared price on the next save: a manual spec is not re-filled
    // from the catalogue for numeric fields.
    updateModel(index, { pricing: null, source: "manual" });
  }

  function commitRoles(next: { key: string; value: string }[]) {
    setRoles(next);
    update({
      modelMap: Object.fromEntries(
        next
          .filter((row) => row.key.trim() !== "")
          .map((row) => [row.key.trim(), row.value.trim()]),
      ),
    });
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
    const next = { ...props.draft };
    if (value.trim() === "") {
      delete next.apiKey;
    } else {
      next.apiKey = value;
      if (next.auth === "none") {
        next.auth = next.protocol === "anthropic_messages" ? "x_api_key" : "bearer";
      }
    }
    props.onChange(next);
  }

  function clearKey() {
    setKeyInput("");
    update({ apiKey: "", auth: "none" });
  }

  return (
    <div class="provider-editor">
      <div class="provider-editor-head">
        <div>
          <h2>{props.isNew ? t("New provider") : props.view?.name || t("Provider")}</h2>
          <div class="provider-editor-meta">
            <Show when={props.view?.active}>
              <span class="provider-active-badge">{t("Active")}</span>
            </Show>
            <Show when={props.view}>
              <span class="mono">{protocolLabel(props.view?.protocol ?? "")}</span>
              <span class={props.draft.enabled === false ? "dot tone-idle" : "dot tone-ok"} />
              <span>{props.draft.enabled === false ? t("Disabled") : t("Enabled")}</span>
            </Show>
          </div>
        </div>
        <Show when={props.isNew}>
          <button class="btn default" onClick={props.onCancelNew}>
            {t("Cancel")}
          </button>
        </Show>
      </div>

      <Show when={props.error}>
        <div class="banner danger">{props.error}</div>
      </Show>
      <Show when={props.notice}>
        <div class="settings-note">
          <IconCheck size={14} />
          {props.notice}
        </div>
      </Show>

      <Show when={props.isNew}>
        <section class="settings-section">
          <h3>{t("What do you want to connect?")}</h3>
          <p class="hint">{t("Start from a preset, then adjust any field.")}</p>
          <div class="preset-grid">
            <For each={PRESETS}>
              {(preset) => (
                <button class="preset" onClick={() => props.onPreset(preset)}>
                  <span class="preset-name">{preset.name}</span>
                  <span class="preset-desc">{t(preset.description)}</span>
                </button>
              )}
            </For>
          </div>
        </section>
      </Show>

      <section class="settings-section">
        <h3>{t("Connection")}</h3>
        <div class="field-grid">
          <label class="field">
            <span>{t("Name")}</span>
            <input
              value={props.draft.name ?? ""}
              onInput={(event) => update({ name: event.currentTarget.value })}
              placeholder={t("My provider")}
            />
          </label>
          <label class="field">
            <span>{t("Protocol")}</span>
            <select
              value={props.draft.protocol ?? "anthropic_messages"}
              onChange={(event) => {
                const protocol = event.currentTarget.value;
                update({
                  protocol,
                  auth:
                    protocol === "anthropic_messages"
                      ? "x_api_key"
                      : "bearer",
                });
              }}
            >
              <For each={["anthropic_messages", "openai_chat", "openai_responses"]}>
                {(protocol) => <option value={protocol}>{protocolLabel(protocol)}</option>}
              </For>
            </select>
          </label>
          <label class="field full">
            <span>{t("Base URL")}</span>
            <input
              class="mono"
              value={props.draft.baseUrl ?? ""}
              onInput={(event) => update({ baseUrl: event.currentTarget.value })}
              placeholder="https://api.example.com/v1"
            />
          </label>
          <label class="field">
            <span>{t("Authentication")}</span>
            <select
              value={props.draft.auth ?? "x_api_key"}
              onChange={(event) => update({ auth: event.currentTarget.value })}
            >
              <option value="x_api_key">{authLabel("x_api_key")}</option>
              <option value="bearer">{authLabel("bearer")}</option>
              <option value="none">{authLabel("none")}</option>
            </select>
          </label>
          <Show when={props.draft.auth !== "none"}>
            <label class="field">
              <span>{t("API key")}</span>
              <div class="field-row">
                <input
                  type={showKey() ? "text" : "password"}
                  value={keyInput()}
                  onInput={(event) => handleKeyInput(event.currentTarget.value)}
                  placeholder={
                    props.view?.hasKey
                      ? t("Leave empty to keep the current key.")
                      : t("API key or env:NAME reference")
                  }
                />
                <button
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
              <Show when={props.view?.hasKey && props.draft.apiKey !== ""}>
                <span class="field-hint mono">
                  {tArgs("Current key: {hint}", { hint: props.view?.keyHint ?? "" })}
                </span>
              </Show>
              <Show when={props.draft.apiKey === ""}>
                <span class="field-hint danger-text">{t("The key will be cleared when you save.")}</span>
              </Show>
              <Show when={props.view?.hasKey && props.draft.apiKey !== ""}>
                <button class="link-button" onClick={clearKey}>
                  {t("Clear key")}
                </button>
              </Show>
            </label>
          </Show>
          <label class="field full">
            <span>{t("Website")}</span>
            <input
              value={props.draft.websiteUrl ?? ""}
              onInput={(event) => update({ websiteUrl: event.currentTarget.value })}
              placeholder="https://"
            />
          </label>
          <label class="field full">
            <span>{t("Notes")}</span>
            <textarea
              rows={2}
              value={props.draft.notes ?? ""}
              onInput={(event) => update({ notes: event.currentTarget.value })}
            />
          </label>
        </div>
        <label class="switch-row provider-enabled">
          <input
            type="checkbox"
            checked={props.draft.enabled !== false}
            onChange={(event) => update({ enabled: event.currentTarget.checked })}
          />
          <span class="switch" aria-hidden="true" />
          <span>{t("Enabled")}</span>
        </label>
      </section>

      <section class="settings-section">
        <div class="settings-section-head">
          <div>
            <h3>{t("Models")}</h3>
            <p class="hint">
              {t(
                "Add model ids manually or fetch them from the provider. Metadata is auto-configured from the model name and can be edited.",
              )}
            </p>
          </div>
          <div class="settings-actions">
            <button class="btn default small" disabled={props.busy} onClick={props.onFetch}>
              <IconRefresh size={14} />
              {t("Fetch from provider")}
            </button>
            <button class="btn default small" onClick={addModel}>
              <IconPlus size={14} />
              {t("Add model")}
            </button>
          </div>
        </div>

        <label class="field">
          <span>{t("Primary model")}</span>
          <input
            class="mono"
            list="provider-model-ids"
            value={props.draft.model ?? ""}
            onInput={(event) => update({ model: event.currentTarget.value })}
            onBlur={() => void suggestPrimary()}
            placeholder={t("Model id")}
          />
          <datalist id="provider-model-ids">
            <For each={modelIds()}>{(id) => <option value={id} />}</For>
          </datalist>
        </label>

        <Show
          when={models().length > 0}
          fallback={<p class="hint">{t("No models yet. Add one manually or fetch the provider's model list.")}</p>}
        >
          <div class="model-list">
            <Index each={models()}>
              {(model, index) => (
                <details
                  class="model-row"
                  ref={(element) => {
                    if (model().id === "") element.open = true;
                  }}
                >
                  <summary class="model-summary">
                    <input
                      type="checkbox"
                      checked={model().enabled}
                      onClick={(event) => event.stopPropagation()}
                      onChange={(event) =>
                        updateModel(index, { enabled: event.currentTarget.checked })
                      }
                    />
                    <span class="model-id mono">{model().id || t("New model")}</span>
                    <span class="model-name">{model().name !== model().id ? model().name : ""}</span>
                    <span class="model-tags">
                      <For each={model().tags.slice(0, 3)}>
                        {(tag) => <span class="model-tag">{tag}</span>}
                      </For>
                    </span>
                    <span class={`model-source source-${model().source}`}>
                      {sourceLabel(model().source)}
                    </span>
                    <button
                      class="icon-btn"
                      title={t("Remove model")}
                      aria-label={t("Remove model")}
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        removeModel(index);
                      }}
                    >
                      <IconTrash size={14} />
                    </button>
                  </summary>
                  <div class="model-body">
                    <label class="field">
                      <span>{t("Model id")}</span>
                      <input
                        class="mono"
                        value={model().id}
                        onInput={(event) => updateModel(index, { id: event.currentTarget.value })}
                        onBlur={() => void suggestModel(index)}
                      />
                    </label>
                    <label class="field">
                      <span>{t("Display name")}</span>
                      <input
                        value={model().name}
                        onInput={(event) => updateModel(index, { name: event.currentTarget.value })}
                      />
                    </label>
                    <label class="field full">
                      <span>{t("Description")}</span>
                      <textarea
                        rows={2}
                        value={model().description}
                        onInput={(event) =>
                          updateModel(index, { description: event.currentTarget.value })
                        }
                        placeholder={t("What is this model good at?")}
                      />
                    </label>
                    <label class="field full">
                      <span>{t("Tags")}</span>
                      <input
                        value={model().tags.join(", ")}
                        onInput={(event) =>
                          updateModel(index, {
                            tags: event.currentTarget.value
                              .split(",")
                              .map((tag) => tag.trim())
                              .filter(Boolean),
                          })
                        }
                        placeholder={t("fast, reasoning, vision")}
                      />
                    </label>
                    <label class="field">
                      <span>{t("Context window")}</span>
                      <input
                        class="mono"
                        inputmode="numeric"
                        value={model().contextWindow ?? ""}
                        onInput={(event) =>
                          updateModel(index, {
                            contextWindow: numberOrNull(event.currentTarget.value),
                          })
                        }
                        placeholder={t("Inherit")}
                      />
                    </label>
                    <label class="field">
                      <span>{t("Max output tokens")}</span>
                      <input
                        class="mono"
                        inputmode="numeric"
                        value={model().maxOutputTokens ?? ""}
                        onInput={(event) =>
                          updateModel(index, {
                            maxOutputTokens: numberOrNull(event.currentTarget.value),
                          })
                        }
                        placeholder={t("Inherit")}
                      />
                    </label>
                    <label class="field">
                      <span>{t("Input $/Mtok")}</span>
                      <input
                        class="mono"
                        inputmode="decimal"
                        value={model().pricing?.inputPerMtok ?? ""}
                        onInput={(event) =>
                          updateModelPricing(index, "inputPerMtok", event.currentTarget.value)
                        }
                        placeholder={t("Inherit")}
                      />
                    </label>
                    <label class="field">
                      <span>{t("Output $/Mtok")}</span>
                      <div class="field-row">
                        <input
                          class="mono"
                          inputmode="decimal"
                          value={model().pricing?.outputPerMtok ?? ""}
                          onInput={(event) =>
                            updateModelPricing(index, "outputPerMtok", event.currentTarget.value)
                          }
                          placeholder={t("Inherit")}
                        />
                        <Show when={model().pricing}>
                          <button
                            class="link-button"
                            onClick={() => clearModelPricing(index)}
                          >
                            {t("Inherit")}
                          </button>
                        </Show>
                      </div>
                    </label>
                    <label class="field">
                      <span>{t("Temperature")}</span>
                      <input
                        class="mono"
                        inputmode="decimal"
                        value={model().temperature ?? ""}
                        onInput={(event) =>
                          updateModel(index, {
                            temperature: numberOrNull(event.currentTarget.value),
                          })
                        }
                        placeholder={t("Inherit")}
                      />
                    </label>
                    <span class="model-footnote full">
                      {t(
                        "Auto-configured from the model name. Edit any field to override; explicit values are kept.",
                      )}
                    </span>
                  </div>
                </details>
              )}
            </Index>
          </div>
        </Show>
      </section>

      <details class="settings-section collapsible">
        <summary>{t("Model roles")}</summary>
        <p class="hint">
          {t(
            "Roles map a logical name (for example fast or reasoner) to a model id. Agents can request a role instead of a concrete model.",
          )}
        </p>
        <div class="map-editor">
          <Index each={roles()}>
            {(role, index) => (
              <div class="map-row">
                <input
                  value={role().key}
                  onInput={(event) => {
                    const next = roles().map((row, i) =>
                      i === index ? { ...row, key: event.currentTarget.value } : row,
                    );
                    commitRoles(next);
                  }}
                  placeholder={t("Role name")}
                />
                <input
                  class="mono"
                  list="provider-model-ids"
                  value={role().value}
                  onInput={(event) => {
                    const next = roles().map((row, i) =>
                      i === index ? { ...row, value: event.currentTarget.value } : row,
                    );
                    commitRoles(next);
                  }}
                  placeholder={t("Model id")}
                />
                <button
                  class="icon-btn"
                  title={t("Remove role")}
                  aria-label={t("Remove role")}
                  onClick={() => commitRoles(roles().filter((_, i) => i !== index))}
                >
                  <IconClose size={14} />
                </button>
              </div>
            )}
          </Index>
          <button
            class="btn default small"
            onClick={() => commitRoles([...roles(), { key: "", value: "" }])}
          >
            <IconPlus size={14} />
            {t("Add role")}
          </button>
        </div>
      </details>

      <details class="settings-section collapsible">
        <summary>{t("Custom headers")}</summary>
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
                />
                <button
                  class="icon-btn"
                  title={t("Remove header")}
                  aria-label={t("Remove header")}
                  onClick={() => commitHeaders(headers().filter((_, i) => i !== index))}
                >
                  <IconClose size={14} />
                </button>
              </div>
            )}
          </Index>
          <button
            class="btn default small"
            onClick={() => commitHeaders([...headers(), { key: "", value: "" }])}
          >
            <IconPlus size={14} />
            {t("Add header")}
          </button>
        </div>
      </details>

      <details class="settings-section collapsible">
        <summary>{t("Provider defaults")}</summary>
        <p class="hint">
          {t("Used when a model does not declare its own pricing, output limit or temperature.")}
        </p>
        <div class="field-grid">
          <label class="field">
            <span>{t("Input $/Mtok")}</span>
            <input
              class="mono"
              inputmode="decimal"
              value={props.draft.pricing?.inputPerMtok ?? ""}
              onInput={(event) => updatePricing("inputPerMtok", event.currentTarget.value)}
            />
          </label>
          <label class="field">
            <span>{t("Output $/Mtok")}</span>
            <input
              class="mono"
              inputmode="decimal"
              value={props.draft.pricing?.outputPerMtok ?? ""}
              onInput={(event) => updatePricing("outputPerMtok", event.currentTarget.value)}
            />
          </label>
          <span class="full field-hint">
            <button
              class="link-button"
              onClick={() => update({ pricing: undefined, clearPricing: true })}
            >
              {t("Use protocol defaults")}
            </button>
          </span>
          <label class="field">
            <span>{t("Max output tokens")}</span>
            <input
              class="mono"
              inputmode="numeric"
              value={props.draft.maxOutputTokens ?? ""}
              onInput={(event) =>
                update({ maxOutputTokens: numberOrNull(event.currentTarget.value) ?? undefined })
              }
              placeholder={t("Default")}
            />
          </label>
          <label class="field">
            <span>{t("Temperature")}</span>
            <input
              class="mono"
              inputmode="decimal"
              value={props.draft.temperature ?? ""}
              onInput={(event) =>
                update({ temperature: numberOrNull(event.currentTarget.value) ?? undefined })
              }
              placeholder={t("Default")}
            />
          </label>
        </div>
      </details>

      <Show when={props.testResult}>
        {(result) => (
          <div
            classList={{
              "test-result": true,
              ok: result().ok,
              bad: !result().ok,
            }}
          >
            <Show when={result().ok} fallback={<IconCircleAlert size={15} />}>
              <IconCheck size={15} />
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

      <div class="provider-editor-foot">
        <div class="settings-actions">
          <button class="btn primary" disabled={props.busy} onClick={props.onSave}>
            {props.busy ? t("Saving…") : t("Save")}
          </button>
          <button class="btn default" disabled={props.busy} onClick={props.onTest}>
            {t("Test connection")}
          </button>
          <Show
            when={
              props.view &&
              !props.view.active &&
              (props.draft.model ?? "").trim() !== ""
            }
          >
            <button class="btn default" disabled={props.busy} onClick={props.onActivate}>
              {t("Set as active")}
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
    </div>
  );
}
