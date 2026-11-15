// Composer: the primary surface. The run bar above it shows live usage; the
// composer itself stays focused on the task input and the model choice.

import { createEffect, onMount } from "solid-js";
import { t } from "../i18n";
import type { ModelSelection, ProviderView } from "../types";
import { IconArrowUp } from "./Icons";
import { ModelPicker } from "./ModelPicker";

const MAX_COMPOSER_HEIGHT = 180;

export function Composer(props: {
  value: string;
  disabled: boolean;
  disabledHint?: string;
  running: boolean;
  provider: ProviderView | null;
  fallbackModel: string;
  selection: ModelSelection | null;
  onModelChange: (selection: ModelSelection) => void;
  onInput: (value: string) => void;
  onSend: () => void;
}) {
  let textareaRef: HTMLTextAreaElement | undefined;

  function resize() {
    const element = textareaRef;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, MAX_COMPOSER_HEIGHT)}px`;
  }

  function handleKeyDown(event: KeyboardEvent) {
    // Enter confirms an IME candidate; never send while composing.
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === "Enter" && !event.shiftKey) {
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
            : t("Describe a task… (Enter to send, Shift+Enter for a newline)")
        }
        disabled={props.disabled}
        rows={2}
        aria-label={t("Message")}
      />
      <div class="composer-actions">
        <ModelPicker
          provider={props.provider}
          fallbackModel={props.fallbackModel}
          value={props.selection}
          disabled={props.disabled}
          disabledHint={props.disabledHint}
          onChange={props.onModelChange}
        />
        <button
          class="btn primary send"
          onClick={props.onSend}
          disabled={props.disabled || !props.value.trim()}
        >
          {t("Send")}
          <IconArrowUp size={14} />
        </button>
      </div>
    </div>
  );
}
