// Composer: the primary surface. The run bar above it shows live usage; the
// composer itself stays focused on the task input and the model choice.

import { Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { t } from "../i18n";
import type { ContextUsageView, ModelSelection, ProviderView } from "../types";
import { ContextMeter } from "./ContextMeter";
import { IconArrowUp, IconStop } from "./Icons";
import { ModelPicker } from "./ModelPicker";

const MAX_COMPOSER_HEIGHT = 180;

/** Touch keyboards have no Shift+Enter, so Enter must insert a newline there. */
function coarsePointerQuery(): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(pointer: coarse)")
    : null;
}

export function Composer(props: {
  value: string;
  disabled: boolean;
  disabledHint?: string;
  running: boolean;
  provider: ProviderView | null;
  fallbackModel: string;
  selection: ModelSelection | null;
  usage: ContextUsageView | null;
  onModelChange: (selection: ModelSelection) => void;
  onInput: (value: string) => void;
  onSend: () => void;
  onStop: () => void;
}) {
  let textareaRef: HTMLTextAreaElement | undefined;
  // 2-in-1 devices can switch between touch and a trackpad, so keep this live
  // instead of sampling matchMedia once during render.
  const [coarsePointer, setCoarsePointer] = createSignal(coarsePointerQuery()?.matches ?? false);

  onMount(() => {
    const query = coarsePointerQuery();
    if (!query) return;
    setCoarsePointer(query.matches);
    const onChange = (event: MediaQueryListEvent) => {
      setCoarsePointer(event.matches);
    };
    query.addEventListener("change", onChange);
    onCleanup(() => query.removeEventListener("change", onChange));
  });

  function resize() {
    const element = textareaRef;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, MAX_COMPOSER_HEIGHT)}px`;
  }

  function handleKeyDown(event: KeyboardEvent) {
    // Enter confirms an IME candidate; never send while composing.
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === "Enter" && !event.shiftKey && !coarsePointer()) {
      event.preventDefault();
      props.onSend();
    }
  }

  createEffect(() => {
    void props.value;
    resize();
  });

  onMount(resize);

  return (
    <div class="composer">
      <textarea
        ref={(element) => (textareaRef = element)}
        value={props.value}
        onInput={(event) => {
          props.onInput(event.currentTarget.value);
          resize();
        }}
        onKeyDown={handleKeyDown}
        placeholder={
          props.running
            ? t("The agent is working…")
            : coarsePointer()
              ? t("Describe a task…")
              : t("Describe a task… (Enter to send, Shift+Enter for a newline)")
        }
        enterkeyhint={coarsePointer() ? "enter" : "send"}
        disabled={props.disabled}
        rows={2}
        aria-label={t("Message")}
      />
      <Show when={props.disabled && !props.running && props.disabledHint}>
        <p class="composer-hint">{props.disabledHint}</p>
      </Show>
      <div class="composer-actions">
        <ModelPicker
          provider={props.provider}
          fallbackModel={props.fallbackModel}
          value={props.selection}
          disabled={props.disabled}
          disabledHint={props.disabledHint}
          onChange={props.onModelChange}
        />
        <div class="composer-actions-right">
          <ContextMeter usage={props.usage} />
          <Show
            when={props.running}
            fallback={
              <button
                class="btn primary send"
                onClick={props.onSend}
                disabled={props.disabled || !props.value.trim()}
              >
                {t("Send")}
                <IconArrowUp size={14} />
              </button>
            }
          >
            <button
              type="button"
              class="btn stop send"
              title={t("Stop")}
              aria-label={t("Stop")}
              onClick={props.onStop}
            >
              <IconStop size={13} />
              {t("Stop")}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
