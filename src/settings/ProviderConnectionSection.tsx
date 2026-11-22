// Connection section of the provider editor: identity, credentials and the
// request format. Validation state is owned by the editor and passed in.

import { Show } from "solid-js";
import { t, tArgs } from "../i18n";
import { IconEye, IconEyeOff } from "../components/Icons";
import { Select } from "../ui/Select";
import { PROTOCOLS, type Protocol, type ProviderInput, type ProviderView } from "./types";
import { protocolLabel, type FieldErrors } from "./providerForm";

export function ProviderConnectionSection(props: {
  draft: ProviderInput;
  view: ProviderView | null;
  isNew: boolean;
  attempted: boolean;
  errors: FieldErrors;
  keyInput: string;
  showKey: boolean;
  onUpdate: (patch: Partial<ProviderInput>) => void;
  onKeyInput: (value: string) => void;
  onToggleKey: () => void;
  onClearKey: () => void;
}) {
  return (
    <section class="editor-section">
      <h3>{t("Connection")}</h3>
      <div class="connection-fields">
        <label class="field">
          <span>{t("Name")}</span>
          <input
            value={props.draft.name ?? ""}
            onInput={(event) => props.onUpdate({ name: event.currentTarget.value })}
            placeholder={t("My provider")}
            autocomplete="off"
            required
            aria-invalid={props.attempted && Boolean(props.errors.name)}
          />
          <Show when={props.attempted && props.errors.name}>
            <span class="field-error">{props.errors.name}</span>
          </Show>
        </label>

        <label class="field">
          <span>{t("Notes")}</span>
          <input
            value={props.draft.notes ?? ""}
            onInput={(event) => props.onUpdate({ notes: event.currentTarget.value })}
            placeholder={t("Optional note, one line")}
            autocomplete="off"
          />
        </label>

        <label class="field">
          <span>{t("Website")}</span>
          <input
            value={props.draft.websiteUrl ?? ""}
            onInput={(event) => props.onUpdate({ websiteUrl: event.currentTarget.value })}
            placeholder="https://example.com"
            autocomplete="off"
            inputmode="url"
            autocapitalize="off"
            autocorrect="off"
            aria-invalid={props.attempted && Boolean(props.errors.website)}
          />
          <Show when={props.attempted && props.errors.website}>
            <span class="field-error">{props.errors.website}</span>
          </Show>
        </label>

        <label class="field">
          <span>{t("API key")}</span>
          <div class="field-row">
            <input
              type={props.showKey ? "text" : "password"}
              value={props.keyInput}
              onInput={(event) => props.onKeyInput(event.currentTarget.value)}
              placeholder={props.view?.keyHint ? "••••••••" : t("API key (optional)")}
              autocomplete="off"
              autocapitalize="off"
              autocorrect="off"
              spellcheck={false}
            />
            <button
              type="button"
              class="icon-btn"
              title={props.showKey ? t("Hide key") : t("Show key")}
              aria-label={props.showKey ? t("Hide key") : t("Show key")}
              onClick={props.onToggleKey}
            >
              <Show when={props.showKey} fallback={<IconEye size={16} />}>
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
              {t("Optional. Leave empty for endpoints that do not require authentication.")}
            </span>
          </Show>
          <Show when={props.isNew && props.view?.keyHint}>
            <button type="button" class="link-button" onClick={props.onClearKey}>
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
              onClick={() => props.onUpdate({ apiKey: undefined })}
            >
              {t("Keep current key")}
            </button>
          </Show>
        </label>

        <label class="field">
          <span>{t("Base URL")}</span>
          <input
            value={props.draft.baseUrl ?? ""}
            onInput={(event) => props.onUpdate({ baseUrl: event.currentTarget.value })}
            placeholder="https://api.example.com/v1"
            autocomplete="off"
            inputmode="url"
            autocapitalize="off"
            autocorrect="off"
            spellcheck={false}
            required
            aria-invalid={props.attempted && Boolean(props.errors.baseUrl)}
          />
          <Show when={props.attempted && props.errors.baseUrl}>
            <span class="field-error">{props.errors.baseUrl}</span>
          </Show>
        </label>

        <div class="field">
          <span>{t("Request format")}</span>
          <Select
            aria-label={t("Request format")}
            value={props.draft.protocol ?? "openai_chat"}
            options={PROTOCOLS.map((protocol) => ({
              value: protocol,
              label: protocolLabel(protocol),
            }))}
            onChange={(value) => props.onUpdate({ protocol: value as Protocol })}
          />
        </div>
      </div>
    </section>
  );
}
