// Second wizard step: preview the provider's live model list and pick the
// models to store. Fetching and selection state live here; persistence is
// delegated to the page through `onSave`.

import { For, Show, createSignal, onMount } from "solid-js";
import { describeModels, previewModels } from "../bridge";
import { t, tArgs, tPlural } from "../i18n";
import { errorText } from "../errors";
import { IconCircleAlert, IconPlus, IconRefresh } from "../components/Icons";
import { cloneModel, modelFromId, normalizeModelId, type ModelSpec } from "./types";
import { formatWindow } from "./providerForm";

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

export function WizardSteps(props: { step: 1 | 2 }) {
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

export function ModelSelectionStep(props: {
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
      const ids = await previewModels(props.providerId);
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
        described = await describeModels(unique);
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
      (row) => row.id.toLowerCase().includes(text) || row.spec.name.toLowerCase().includes(text),
    );
  };

  const selectedCount = () => rows().filter((row) => checked()[normalizeModelId(row.id)]).length;

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
      described = await describeModels(fresh);
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
          <p class="hint">{t("Select the models to add to this provider.")}</p>
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
          <p class="hint">{t("Existing models are kept; selected models are added.")}</p>
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
              aria-label={t("Search models")}
              autocomplete="off"
              autocapitalize="off"
              autocorrect="off"
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
                aria-label={t("Add model IDs manually")}
                autocapitalize="off"
                autocorrect="off"
                spellcheck={false}
              />
              <button class="btn" disabled={!manual().trim()} onClick={() => void addManual()}>
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
              : tPlural("Add {n} model", "Add {n} models", selectedCount())}
          </button>
        </div>
      </div>
    </>
  );
}
