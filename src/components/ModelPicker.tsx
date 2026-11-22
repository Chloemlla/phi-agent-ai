// Model chip and menu. Desktop reveals thinking effort on hover; coarse
// pointers expand it inline. The selection itself lives in App.

import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { t } from "../i18n";
import {
  THINKING_LEVELS,
  type ModelSelection,
  type ModelSpec,
  type ProviderView,
  type ThinkingLevel,
} from "../types";
import { IconCheck, IconChevronDown } from "./Icons";

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

function ThinkingChoices(props: {
  model: ModelSpec;
  selected: boolean;
  current: ThinkingLevel;
  onChoose: (model: ModelSpec, level: ThinkingLevel) => void;
}) {
  return (
    <>
      <div class="model-submenu-label">{t("Thinking")}</div>
      <For each={THINKING_LEVELS}>
        {(level) => (
          <button
            type="button"
            class="model-effort"
            classList={{
              checked: props.selected && props.current === level,
            }}
            role="menuitemradio"
            aria-checked={props.selected && props.current === level}
            onClick={(event) => {
              event.stopPropagation();
              props.onChoose(props.model, level);
            }}
          >
            <span class="model-effort-check">
              <Show when={props.selected && props.current === level}>
                <IconCheck size={13} />
              </Show>
            </span>
            {thinkingLabel(level)}
          </button>
        )}
      </For>
    </>
  );
}

export function ModelPicker(props: {
  provider: ProviderView | null;
  fallbackModel: string;
  value: ModelSelection | null;
  disabled: boolean;
  disabledHint?: string;
  onChange: (selection: ModelSelection) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [hovered, setHovered] = createSignal<string | null>(null);
  const [expanded, setExpanded] = createSignal<string | null>(null);
  const [coarse, setCoarse] = createSignal(false);
  const [narrow, setNarrow] = createSignal(false);
  const [submenuTop, setSubmenuTop] = createSignal(0);
  const [submenuFlip, setSubmenuFlip] = createSignal(false);
  let rootRef: HTMLDivElement | undefined;
  let chipRef: HTMLButtonElement | undefined;
  let hideTimer: number | undefined;

  // A flyout submenu needs room to the side; on narrow windows and touch
  // devices the choices expand inline under the row instead.
  const inlineEffort = () => coarse() || narrow();

  const models = (): ModelSpec[] => {
    const provider = props.provider;
    if (!provider) return [];
    const enabled = provider.models.filter((model) => model.enabled);
    if (enabled.length > 0) return enabled;
    const fallback = props.fallbackModel || provider.model;
    return fallback ? [{ id: fallback, name: fallback, enabled: true }] : [];
  };

  const currentModel = () => models().find((model) => model.id === props.value?.modelId) ?? null;

  const currentName = () =>
    currentModel()?.name || props.value?.modelId || props.fallbackModel || t("No model");

  const currentThinking = (): ThinkingLevel => props.value?.thinking ?? "off";

  const hoveredModel = () => models().find((model) => model.id === hovered()) ?? null;

  const floatingModel = () => {
    if (inlineEffort()) return null;
    return hoveredModel();
  };

  function close(returnFocus = false) {
    setOpen(false);
    setHovered(null);
    setExpanded(null);
    if (hideTimer) {
      window.clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    if (returnFocus) queueMicrotask(() => chipRef?.focus());
  }

  function showHovered(model: ModelSpec, row: HTMLElement) {
    if (inlineEffort()) return;
    if (hideTimer) {
      window.clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    setHovered(model.id);
    const popover = rootRef?.querySelector<HTMLElement>(".model-popover");
    if (!popover) return;
    const popoverRect = popover.getBoundingClientRect();
    const rowRect = row.getBoundingClientRect();
    const top = rowRect.top - popoverRect.top - 4;
    setSubmenuTop(Math.max(4, Math.min(top, popoverRect.height - 168)));
    setSubmenuFlip(rowRect.right + 6 + 168 > window.innerWidth - 8);
  }

  function scheduleHide() {
    if (inlineEffort()) return;
    if (hideTimer) window.clearTimeout(hideTimer);
    hideTimer = window.setTimeout(() => {
      setHovered(null);
      hideTimer = undefined;
    }, 140);
  }

  function keepHovered(id: string) {
    if (hideTimer) {
      window.clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    setHovered(id);
  }

  function toggle() {
    if (props.disabled || !props.provider) return;
    if (open()) {
      close();
      return;
    }
    setOpen(true);
    queueMicrotask(() => {
      const rows = rootRef?.querySelectorAll<HTMLButtonElement>(".model-row");
      const target = rootRef?.querySelector<HTMLButtonElement>(".model-row.current") ?? rows?.[0];
      target?.focus();
    });
  }

  function emit(model: ModelSpec, thinking: ThinkingLevel) {
    const provider = props.provider;
    if (!provider) return;
    props.onChange({
      providerId: provider.id,
      modelId: model.id,
      thinking,
    });
  }

  function thinkingFor(model: ModelSpec): ThinkingLevel {
    if (props.value?.modelId === model.id) return currentThinking();
    return "off";
  }

  function chooseModel(model: ModelSpec) {
    emit(model, thinkingFor(model));
    close(true);
  }

  function handleModelClick(model: ModelSpec) {
    if (!inlineEffort()) {
      chooseModel(model);
      return;
    }
    if (expanded() === model.id) {
      setExpanded(null);
      return;
    }
    // Select the model immediately, then let the same popover choose effort.
    emit(model, thinkingFor(model));
    setExpanded(model.id);
  }

  function chooseEffort(model: ModelSpec, level: ThinkingLevel) {
    emit(model, level);
    close(true);
  }

  function handleMenuKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      close(true);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const rows = Array.from(rootRef?.querySelectorAll<HTMLButtonElement>(".model-row") ?? []);
      if (rows.length === 0) return;
      const active = document.activeElement as HTMLElement | null;
      const index = active ? rows.indexOf(active as HTMLButtonElement) : -1;
      const next =
        event.key === "ArrowDown"
          ? (index + 1 + rows.length) % rows.length
          : (index - 1 + rows.length) % rows.length;
      rows[index === -1 ? 0 : next]?.focus();
      event.preventDefault();
      return;
    }
    if (event.key === "ArrowRight" && !inlineEffort()) {
      const row = (event.target as HTMLElement | null)?.closest(".model-row");
      const effort = rootRef?.querySelector<HTMLButtonElement>(".model-submenu .model-effort");
      if (row && effort) {
        effort.focus();
        event.preventDefault();
      }
    }
  }

  createEffect(() => {
    if (props.disabled) close();
  });

  createEffect(() => {
    const provider = props.provider;
    if (!provider) close();
    void provider;
  });

  onMount(() => {
    if (typeof window !== "undefined" && window.matchMedia) {
      const query = window.matchMedia("(hover: none), (pointer: coarse)");
      const update = () => setCoarse(query.matches);
      update();
      query.addEventListener("change", update);
      onCleanup(() => query.removeEventListener("change", update));

      const narrowQuery = window.matchMedia("(max-width: 760px)");
      const updateNarrow = () => setNarrow(narrowQuery.matches);
      updateNarrow();
      narrowQuery.addEventListener("change", updateNarrow);
      onCleanup(() => narrowQuery.removeEventListener("change", updateNarrow));
    }

    const onPointerDown = (event: PointerEvent) => {
      if (!open()) return;
      const target = event.target;
      if (rootRef && target instanceof Node && !rootRef.contains(target)) {
        close();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (open() && event.key === "Escape") close();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
      if (hideTimer) window.clearTimeout(hideTimer);
    });
  });

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
    <div class="model-picker" classList={{ coarse: coarse(), open: open() }} ref={rootRef}>
      <button
        type="button"
        class="model-chip"
        ref={(element) => (chipRef = element)}
        disabled={props.disabled || !props.provider}
        title={hint()}
        aria-haspopup="menu"
        aria-expanded={open()}
        onClick={toggle}
      >
        <span class="model-chip-name">{currentName()}</span>
        <Show when={props.value && currentThinking() !== "off"}>
          <span class="model-chip-effort">· {thinkingLabel(currentThinking())}</span>
        </Show>
        <IconChevronDown size={12} />
      </button>

      <Show when={open()}>
        <div
          class="model-popover"
          role="menu"
          aria-label={t("Model")}
          onKeyDown={handleMenuKeyDown}
        >
          <div class="model-menu-head">
            <span>{t("Model")}</span>
            <span class="model-menu-provider">{props.provider?.name}</span>
          </div>

          <Show
            when={models().length > 0}
            fallback={<p class="model-note">{t("No models are enabled for this provider.")}</p>}
          >
            <div class="model-list" onScroll={() => setHovered(null)}>
              <For each={models()}>
                {(model) => {
                  const selected = () => props.value?.modelId === model.id;
                  return (
                    <div
                      class="model-item"
                      classList={{ expanded: expanded() === model.id }}
                      onMouseEnter={(event) => showHovered(model, event.currentTarget)}
                      onMouseLeave={scheduleHide}
                    >
                      <button
                        type="button"
                        class="model-row"
                        classList={{ current: selected() }}
                        role="menuitem"
                        aria-haspopup="menu"
                        onFocus={(event) => showHovered(model, event.currentTarget)}
                        onClick={() => handleModelClick(model)}
                      >
                        <span class="model-row-name">{model.name || model.id}</span>
                        <Show when={selected()}>
                          <span class="model-row-check">
                            <IconCheck size={13} />
                          </span>
                        </Show>
                        <span class="model-row-effort">
                          {selected() && currentThinking() !== "off"
                            ? thinkingLabel(currentThinking())
                            : t("Thinking")}
                        </span>
                      </button>

                      <Show when={inlineEffort() && expanded() === model.id}>
                        <div class="model-submenu" role="menu" aria-label={t("Thinking")}>
                          <ThinkingChoices
                            model={model}
                            selected={selected()}
                            current={selected() ? currentThinking() : "off"}
                            onChoose={chooseEffort}
                          />
                        </div>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>

            <Show when={floatingModel()}>
              {(model) => (
                <div
                  class="model-submenu floating"
                  classList={{ flip: submenuFlip() }}
                  style={{ top: `${submenuTop()}px` }}
                  role="menu"
                  aria-label={t("Thinking")}
                  onMouseEnter={() => keepHovered(model().id)}
                  onMouseLeave={scheduleHide}
                >
                  <ThinkingChoices
                    model={model()}
                    selected={props.value?.modelId === model().id}
                    current={props.value?.modelId === model().id ? currentThinking() : "off"}
                    onChoose={chooseEffort}
                  />
                </div>
              )}
            </Show>
          </Show>
        </div>
      </Show>
    </div>
  );
}
