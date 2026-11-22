// Composer: the primary surface. The run bar above it shows live usage; the
// composer itself stays focused on the task input and the model choice.

import { For, Show, createEffect, onMount } from "solid-js";
import { t } from "../i18n";
import { coarsePointer } from "../layout/environment";
import type { ContextUsageView, ImageAsset, ModelSelection, ProviderView } from "../types";
import { ContextMeter } from "./ContextMeter";
import { IconArrowUp, IconClose, IconPlus, IconStop } from "./Icons";
import { MessageImage } from "./MessageImage";
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
  usage: ContextUsageView | null;
  attachments: ImageAsset[];
  attaching: boolean;
  onAttach: () => void;
  onRemoveAttachment: (assetId: string) => void;
  onModelChange: (selection: ModelSelection) => void;
  onInput: (value: string) => void;
  onSend: () => void;
  onStop: () => void;
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
    if (event.key === "Enter" && !event.shiftKey && !coarsePointer()) {
      event.preventDefault();
      if (!props.attaching) props.onSend();
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
      <Show when={props.attachments.length > 0}>
        <div class="composer-attachments" role="list" aria-label={t("Attached images")}>
          <For each={props.attachments}>
            {(asset) => (
              <div class="composer-attachment" role="listitem">
                <MessageImage
                  assetId={asset.assetId}
                  alt={asset.promptPreview || t("Attached image")}
                  variant="thumbnail"
                />
                <button
                  type="button"
                  class="composer-attachment-remove"
                  aria-label={t("Remove image")}
                  title={t("Remove image")}
                  onClick={() => props.onRemoveAttachment(asset.assetId)}
                >
                  <IconClose size={11} />
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.disabled && !props.running && props.disabledHint}>
        <p class="composer-hint">{props.disabledHint}</p>
      </Show>
      <div class="composer-actions">
        <div class="composer-actions-left">
          <button
            type="button"
            class="composer-attach"
            aria-label={props.attaching ? t("Uploading image…") : t("Attach images")}
            title={props.attaching ? t("Uploading image…") : t("Attach images")}
            aria-busy={props.attaching}
            onClick={() => props.onAttach()}
            disabled={props.disabled || props.attaching}
          >
            <Show when={props.attaching} fallback={<IconPlus size={16} />}>
              <span class="ring" aria-hidden="true" />
            </Show>
          </button>
          <ModelPicker
            provider={props.provider}
            fallbackModel={props.fallbackModel}
            value={props.selection}
            disabled={props.disabled}
            disabledHint={props.disabledHint}
            onChange={props.onModelChange}
          />
        </div>
        <div class="composer-actions-right">
          <ContextMeter usage={props.usage} />
          <Show
            when={props.running}
            fallback={
              <button
                type="button"
                class="btn primary send"
                onClick={props.onSend}
                disabled={
                  props.disabled ||
                  props.attaching ||
                  (!props.value.trim() && props.attachments.length === 0)
                }
              >
                {t("Send")}
                <IconArrowUp size={15} />
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
