// Model and thinking selectors. Two adjacent dropdowns keep the choice of
// model and effort explicit; both menus open from their own trigger and carry
// no provider header, matching the compact composer language.

import { Show } from "solid-js";
import { t } from "../i18n";
import {
  THINKING_LEVELS,
  type ModelSelection,
  type ModelSpec,
  type ProviderView,
  type ThinkingLevel,
} from "../types";
import { Select, type SelectOption } from "../ui/Select";

function thinkingLabel(level: ThinkingLevel): string {
  switch (level) {
    case "max":
      return t("Maximum");
    case "xhigh":
      return t("Extra high");
    case "high":
      return t("High");
    case "medium":
      return t("Medium");
    case "low":
      return t("Low");
    default:
      return t("Default");
  }
}

export function ModelPicker(props: {
  provider: ProviderView | null;
  fallbackModel: string;
  value: ModelSelection | null;
  disabled: boolean;
  disabledHint?: string;
  onChange: (selection: ModelSelection) => void;
}) {
  const models = (): ModelSpec[] => {
    const provider = props.provider;
    if (!provider) return [];
    const enabled = provider.models.filter((model) => model.enabled);
    if (enabled.length > 0) return enabled;
    const fallback = props.fallbackModel || provider.model;
    return fallback ? [{ id: fallback, name: fallback, enabled: true }] : [];
  };

  const modelOptions = (): SelectOption[] =>
    models().map((model) => ({
      value: model.id,
      label: model.name && model.name !== model.id ? model.name : model.id,
    }));

  const thinkingOptions = (): SelectOption[] =>
    THINKING_LEVELS.map((level) => ({ value: level, label: thinkingLabel(level) }));

  const currentModelId = () => props.value?.modelId ?? models()[0]?.id ?? props.fallbackModel ?? "";
  const currentThinking = (): ThinkingLevel => props.value?.thinking ?? "off";
  const hasModel = () => Boolean(currentModelId());

  function emit(modelId: string, thinking: ThinkingLevel) {
    const provider = props.provider;
    if (!provider || !modelId) return;
    props.onChange({ providerId: provider.id, modelId, thinking });
  }

  const hint = () => {
    if (!props.provider) {
      return t("No model provider is configured. Add one in Settings.");
    }
    if (props.disabled) {
      return props.disabledHint ?? t("The model cannot be changed while a run is active.");
    }
    return t("Choose the model and thinking effort");
  };

  return (
    <div class="model-picker" classList={{ disabled: props.disabled || !props.provider }}>
      <Show
        when={hasModel()}
        fallback={
          <span class="model-picker-empty" title={hint()}>
            {t("No model")}
          </span>
        }
      >
        <Select
          class="model-select ui-select--ghost"
          aria-label={t("Model")}
          title={hint()}
          value={currentModelId()}
          options={modelOptions()}
          disabled={props.disabled || !props.provider}
          menuWidth={224}
          onChange={(modelId) => emit(modelId, currentThinking())}
        />
        <Select
          class="thinking-select ui-select--ghost"
          aria-label={t("Thinking effort")}
          title={hint()}
          value={currentThinking()}
          options={thinkingOptions()}
          disabled={props.disabled || !props.provider || !hasModel()}
          menuWidth={144}
          onChange={(value) => emit(currentModelId(), value as ThinkingLevel)}
        />
      </Show>
    </div>
  );
}
