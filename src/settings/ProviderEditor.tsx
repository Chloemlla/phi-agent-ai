// Provider editor. A new provider is a two-step wizard (connection, then model
// selection); an existing provider uses the same connection form plus a model
// list and a collapsible advanced section. All persistence goes through the
// page's callbacks; this component never writes files directly.

import { Show, createEffect, createSignal, on, onCleanup } from "solid-js";
import { describeModels } from "../bridge";
import { t, tArgs, tPlural } from "../i18n";
import { errorText } from "../errors";
import { IconCheck, IconCircleAlert, IconTrash } from "../components/Icons";
import {
  mergeSuggestion,
  modelFromId,
  normalizeModelId,
  type ModelSpec,
  type ProviderInput,
  type ProviderTestView,
  type ProviderView,
} from "./types";
import { entries, protocolLabel, validateConnection } from "./providerForm";
import { ProviderConnectionSection } from "./ProviderConnectionSection";
import { ProviderModelsSection } from "./ProviderModelsSection";
import { ModelSelectionStep, WizardSteps } from "./ModelSelectionStep";

export { protocolLabel };

export function ProviderEditor(props: {
  selectionKey: string;
  draft: ProviderInput;
  view: ProviderView | null;
  isNew: boolean;
  busy: boolean;
  dirty: boolean;
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
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;

  function disarmDelete() {
    if (deleteTimer !== undefined) {
      clearTimeout(deleteTimer);
      deleteTimer = undefined;
    }
    setConfirmDelete(false);
  }

  function armDelete() {
    if (deleteTimer !== undefined) clearTimeout(deleteTimer);
    setConfirmDelete(true);
    deleteTimer = setTimeout(() => {
      deleteTimer = undefined;
      setConfirmDelete(false);
    }, 5000);
  }

  function handleDeleteConfirm() {
    disarmDelete();
    props.onDelete();
  }

  onCleanup(() => {
    if (deleteTimer !== undefined) clearTimeout(deleteTimer);
  });

  createEffect(
    on(
      () => props.selectionKey,
      () => {
        disarmDelete();
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
        disarmDelete();
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
      const [suggested] = await describeModels([id]);
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
        next.filter((row) => row.key.trim() !== "").map((row) => [row.key.trim(), row.value]),
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
          <ModelSelectionStep
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
                    <span class="mono">{protocolLabel(props.draft.protocol ?? "openai_chat")}</span>
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
                      ? tPlural(
                          "Connected in {ms} ms, {n} model available.",
                          "Connected in {ms} ms, {n} models available.",
                          result().modelCount,
                          { ms: result().latencyMs },
                        )
                      : tArgs("Connection failed: {error}", {
                          error: result().error ?? errorText(result()),
                        })}
                  </span>
                </div>
              )}
            </Show>
          </div>
        </Show>

        <ProviderConnectionSection
          draft={props.draft}
          view={props.view}
          isNew={props.isNew}
          attempted={attempted()}
          errors={errors()}
          keyInput={keyInput()}
          showKey={showKey()}
          onUpdate={update}
          onKeyInput={handleKeyInput}
          onToggleKey={() => setShowKey((shown) => !shown)}
          onClearKey={clearKey}
        />

        <Show when={!props.isNew}>
          <ProviderModelsSection
            models={models()}
            busy={props.busy}
            canFetch={Boolean(providerId())}
            isPrimary={isPrimary}
            keyHint={props.view?.keyHint}
            apiKey={props.draft.apiKey}
            headers={headers()}
            manualModel={manualModel()}
            addingModel={addingModel()}
            onFetch={() => void handleFetchModels()}
            onRemove={removeModel}
            onManualModelChange={setManualModel}
            onAddManual={() => void addManualModel()}
            onCommitHeaders={commitHeaders}
            onAddHeader={() => setHeaders([...headers(), { key: "", value: "" }])}
            onClearKey={clearKey}
          />
        </Show>

        <Show when={!props.isNew}>
          <section class="editor-danger">
            <Show
              when={confirmDelete()}
              fallback={
                <button type="button" class="btn danger" disabled={props.busy} onClick={armDelete}>
                  <IconTrash size={14} />
                  {t("Delete provider")}
                </button>
              }
            >
              <p class="editor-danger-question">
                {tArgs("Delete provider {name}? This cannot be undone.", {
                  name: props.view?.name ?? props.draft.name ?? "",
                })}
              </p>
              <div class="editor-foot-group">
                <button
                  type="button"
                  class="btn danger"
                  disabled={props.busy}
                  onClick={handleDeleteConfirm}
                >
                  {t("Confirm delete")}
                </button>
                <button type="button" class="btn" disabled={props.busy} onClick={disarmDelete}>
                  {t("Cancel")}
                </button>
              </div>
            </Show>
          </section>
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
                    {props.dirty ? t("Save & test") : t("Test connection")}
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
        </div>
      </Show>
    </div>
  );
}
