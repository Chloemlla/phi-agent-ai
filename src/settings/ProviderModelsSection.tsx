// Existing models and advanced request options for a saved provider. The
// editor owns the draft; this section only renders it and forwards edits.

import { For, Index, Show } from "solid-js";
import { t } from "../i18n";
import { IconClose, IconPlus, IconRefresh, IconTrash } from "../components/Icons";
import type { ModelSpec } from "./types";
import { formatWindow } from "./providerForm";

export function ProviderModelsSection(props: {
  models: ModelSpec[];
  busy: boolean;
  canFetch: boolean;
  isPrimary: (model: ModelSpec) => boolean;
  keyHint?: string;
  apiKey?: string;
  headers: { key: string; value: string }[];
  manualModel: string;
  addingModel: boolean;
  onFetch: () => void;
  onRemove: (index: number) => void;
  onManualModelChange: (value: string) => void;
  onAddManual: () => void;
  onCommitHeaders: (next: { key: string; value: string }[]) => void;
  onAddHeader: () => void;
  onClearKey: () => void;
}) {
  return (
    <>
      <section class="editor-section">
        <div class="section-head">
          <h3>{t("Models")}</h3>
          <button
            class="btn"
            disabled={props.busy || !props.canFetch}
            onClick={props.onFetch}
          >
            <IconRefresh size={14} />
            {t("Fetch models")}
          </button>
        </div>

        <Show
          when={props.models.length > 0}
          fallback={
            <p class="hint">
              {t("No models yet. Add one manually or fetch the provider's model list.")}
            </p>
          }
        >
          <ul class="provider-model-list">
            <For each={props.models}>
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
                  <Show when={props.isPrimary(model)}>
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
                    onClick={() => props.onRemove(index())}
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
            props.onAddManual();
          }}
        >
          <input
            class="mono"
            value={props.manualModel}
            onInput={(event) => props.onManualModelChange(event.currentTarget.value)}
            placeholder={t("Add model by id")}
            autocomplete="off"
            autocapitalize="off"
            autocorrect="off"
            spellcheck={false}
          />
          <button
            class="btn"
            type="submit"
            disabled={props.busy || props.addingModel || !props.manualModel.trim()}
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
            <Index each={props.headers}>
              {(header, index) => (
                <div class="map-row">
                  <input
                    class="mono"
                    value={header().key}
                    onInput={(event) => {
                      const next = props.headers.map((row, i) =>
                        i === index ? { ...row, key: event.currentTarget.value } : row,
                      );
                      props.onCommitHeaders(next);
                    }}
                    placeholder={t("Header")}
                    autocomplete="off"
                    spellcheck={false}
                  />
                  <input
                    class="mono"
                    value={header().value}
                    onInput={(event) => {
                      const next = props.headers.map((row, i) =>
                        i === index ? { ...row, value: event.currentTarget.value } : row,
                      );
                      props.onCommitHeaders(next);
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
                      props.onCommitHeaders(props.headers.filter((_, i) => i !== index))
                    }
                  >
                    <IconClose size={14} />
                  </button>
                </div>
              )}
            </Index>
            <button type="button" class="btn" onClick={props.onAddHeader}>
              <IconPlus size={14} />
              {t("Add header")}
            </button>
          </div>
          <Show when={props.keyHint || (props.apiKey ?? "") !== ""}>
            <div class="advanced-actions">
              <button type="button" class="link-button" onClick={props.onClearKey}>
                {t("Clear key")}
              </button>
            </div>
          </Show>
        </div>
      </details>
    </>
  );
}
