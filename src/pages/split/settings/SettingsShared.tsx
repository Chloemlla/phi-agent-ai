// Shared settings chrome: section icons, the unsaved-changes guard and the
// pane loading marker. Both end-specific views render these, so the wording
// and icon language never drift between desktop and mobile.

import { Show } from "solid-js";
import { IconBrain, IconServer, IconSettings } from "../../../components/Icons";
import { t } from "../../../i18n";
import type { SettingsSectionId } from "../../../core/settings/schema";

export function SettingsSectionIcon(props: { id: SettingsSectionId; size?: number }) {
  const size = () => props.size ?? 16;
  return (
    <Show
      when={props.id === "general"}
      fallback={
        <Show when={props.id === "providers"} fallback={<IconBrain size={size()} />}>
          <IconServer size={size()} />
        </Show>
      }
    >
      <IconSettings size={size()} />
    </Show>
  );
}

export function SettingsLeaveGuard(props: { onDiscard: () => void; onCancel: () => void }) {
  return (
    <div class="draft-guard settings-leave-guard" role="alert">
      <span>{t("Discard unsaved changes?")}</span>
      <div class="draft-guard-actions">
        <button type="button" class="btn danger" onClick={() => props.onDiscard()}>
          {t("Discard")}
        </button>
        <button type="button" class="btn" onClick={() => props.onCancel()}>
          {t("Cancel")}
        </button>
      </div>
    </div>
  );
}

export function SettingsPaneLoading() {
  return (
    <div class="settings-pane-loading" role="status">
      <span class="ring" aria-hidden="true" />
      <span>{t("Loading…")}</span>
    </div>
  );
}
