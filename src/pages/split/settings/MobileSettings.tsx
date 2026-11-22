// Mobile settings view: an index of grouped sections that drills into one
// section at a time. Panes stay mounted under the detail layer so drafts and
// scroll state survive the back-and-forth; the index is its own screen, not a
// squeezed desktop rail.

import { For, Show } from "solid-js";
import { IconChevron, IconChevronLeft } from "../../../components/Icons";
import {
  settingsGroupsFor,
  settingsSectionsFor,
  settingsSection,
  type SettingsSectionId,
} from "../../../core/settings/schema";
import { t } from "../../../i18n";
import { SettingsLeaveGuard, SettingsSectionIcon } from "./SettingsShared";
import { SettingsSections } from "./SettingsSections";

interface MobileSettingsProps {
  section: SettingsSectionId;
  visited: ReadonlySet<SettingsSectionId>;
  mobileSection: SettingsSectionId | null;
  leavePrompt: boolean;
  onOpen: (section: SettingsSectionId) => void;
  onCloseSection: () => void;
  onBack: () => void;
  onDiscardLeave: () => void;
  onCancelLeave: () => void;
  workspace: string;
  language: string;
  onRefresh: () => Promise<void>;
  onProvidersDirty: (dirty: boolean) => void;
}

export function MobileSettings(props: MobileSettingsProps) {
  const groups = settingsGroupsFor("mobile");
  const sections = settingsSectionsFor("mobile");
  const current = () => (props.mobileSection ? settingsSection(props.mobileSection) : null);

  return (
    <div class="mobile-settings">
      <Show when={props.mobileSection === null}>
        <div class="mobile-settings-index">
          <header class="mobile-settings-head">
            <button
              type="button"
              class="icon-btn"
              aria-label={t("Back to chat")}
              title={t("Back to chat")}
              onClick={() => props.onBack()}
            >
              <IconChevronLeft size={20} />
            </button>
            <h1>{t("Settings")}</h1>
          </header>

          <div class="mobile-settings-scroll">
            <Show when={props.leavePrompt}>
              <SettingsLeaveGuard onDiscard={props.onDiscardLeave} onCancel={props.onCancelLeave} />
            </Show>
            <For each={groups}>
              {(group) => (
                <section class="mobile-settings-group">
                  <p class="mobile-settings-group-label">{t(group.label)}</p>
                  <div class="mobile-settings-card">
                    <For each={sections.filter((section) => section.group === group.id)}>
                      {(section) => (
                        <button
                          type="button"
                          class="mobile-settings-row"
                          onClick={() => props.onOpen(section.id)}
                        >
                          <span class="mobile-settings-icon">
                            <SettingsSectionIcon id={section.id} size={18} />
                          </span>
                          <span class="mobile-settings-copy">
                            <span class="mobile-settings-label">{t(section.label)}</span>
                            <span class="mobile-settings-description">
                              {t(section.description)}
                            </span>
                          </span>
                          <IconChevron size={17} class="mobile-settings-chevron" />
                        </button>
                      )}
                    </For>
                  </div>
                </section>
              )}
            </For>
          </div>
        </div>
      </Show>

      <div class="mobile-settings-detail" hidden={props.mobileSection === null}>
        <header class="mobile-settings-head">
          <button
            type="button"
            class="icon-btn"
            aria-label={t("Back to settings")}
            title={t("Back to settings")}
            onClick={() => props.onCloseSection()}
          >
            <IconChevronLeft size={20} />
          </button>
          <span class="mobile-settings-crumb">{t("Settings")}</span>
          <span class="mobile-settings-title">{current() ? t(current()!.label) : ""}</span>
        </header>
        <div class="mobile-settings-scroll">
          <Show when={props.leavePrompt}>
            <SettingsLeaveGuard onDiscard={props.onDiscardLeave} onCancel={props.onCancelLeave} />
          </Show>
          <SettingsSections
            section={props.section}
            visited={props.visited}
            hidden={props.mobileSection === null}
            workspace={props.workspace}
            language={props.language}
            onRefresh={props.onRefresh}
            onProvidersDirty={props.onProvidersDirty}
          />
        </div>
      </div>
    </div>
  );
}
