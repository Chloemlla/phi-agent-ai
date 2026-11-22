// Desktop settings view: a rail of grouped sections beside a scrolling pane,
// inside a rounded card. Hover is a flat translucent fill, never a border.

import { For, Show } from "solid-js";
import { IconArrowLeft } from "../../../components/Icons";
import {
  settingsGroupsFor,
  settingsSectionsFor,
  type SettingsSectionId,
} from "../../../core/settings/schema";
import { t } from "../../../i18n";
import { SettingsLeaveGuard, SettingsSectionIcon } from "./SettingsShared";
import { SettingsSections } from "./SettingsSections";

interface DesktopSettingsProps {
  section: SettingsSectionId;
  visited: ReadonlySet<SettingsSectionId>;
  leavePrompt: boolean;
  onSelect: (section: SettingsSectionId) => void;
  onBack: () => void;
  onDiscardLeave: () => void;
  onCancelLeave: () => void;
  workspace: string;
  language: string;
  onRefresh: () => Promise<void>;
  onProvidersDirty: (dirty: boolean) => void;
}

export function DesktopSettings(props: DesktopSettingsProps) {
  const groups = settingsGroupsFor("desktop");
  const sections = settingsSectionsFor("desktop");

  return (
    <div class="settings-canvas">
      <div class="settings-shell">
        <nav class="settings-rail" aria-label={t("Settings")}>
          <button type="button" class="settings-back" onClick={() => props.onBack()}>
            <IconArrowLeft size={16} />
            <span>{t("Back to chat")}</span>
          </button>
          <p class="settings-rail-title">{t("Settings")}</p>
          <For each={groups}>
            {(group) => (
              <>
                <p class="settings-group-label">{t(group.label)}</p>
                <For each={sections.filter((section) => section.group === group.id)}>
                  {(section) => (
                    <button
                      type="button"
                      class="settings-nav-item"
                      classList={{ active: props.section === section.id }}
                      aria-current={props.section === section.id ? "page" : undefined}
                      onClick={() => props.onSelect(section.id)}
                    >
                      <SettingsSectionIcon id={section.id} />
                      <span>{t(section.label)}</span>
                    </button>
                  )}
                </For>
              </>
            )}
          </For>
        </nav>

        <div class="settings-content">
          <Show when={props.leavePrompt}>
            <SettingsLeaveGuard onDiscard={props.onDiscardLeave} onCancel={props.onCancelLeave} />
          </Show>
          <div class="settings-content-inner">
            <SettingsSections
              section={props.section}
              visited={props.visited}
              workspace={props.workspace}
              language={props.language}
              onRefresh={props.onRefresh}
              onProvidersDirty={props.onProvidersDirty}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
