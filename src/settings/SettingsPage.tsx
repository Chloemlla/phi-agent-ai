// Full-page settings shell: a local navigation rail and the two panes. Both
// panes stay mounted so switching sections never discards a provider draft.

import { createSignal } from "solid-js";
import { t } from "../i18n";
import { IconArrowLeft, IconServer, IconSettings } from "../components/Icons";
import { GeneralPage } from "./GeneralPage";
import { ProvidersPage } from "./ProvidersPage";
import "./Settings.css";

type Section = "providers" | "general";

export function SettingsPage(props: {
  workspace: string;
  language: string;
  onBack: () => void;
  onRefresh: () => Promise<void>;
}) {
  const [section, setSection] = createSignal<Section>("providers");

  return (
    <div class="settings-page">
      <aside class="settings-nav">
        <button class="settings-back" onClick={props.onBack}>
          <IconArrowLeft size={15} />
          {t("Back to chat")}
        </button>
        <div class="side-label">{t("Settings")}</div>
        <button
          classList={{ "settings-nav-item": true, active: section() === "providers" }}
          onClick={() => setSection("providers")}
        >
          <IconServer size={15} />
          {t("Model providers")}
        </button>
        <button
          classList={{ "settings-nav-item": true, active: section() === "general" }}
          onClick={() => setSection("general")}
        >
          <IconSettings size={15} />
          {t("General")}
        </button>
        <div class="settings-nav-foot">{t("Settings are stored on this node.")}</div>
      </aside>

      <div class="settings-content">
        <div class="settings-content-inner">
          <div classList={{ "settings-pane": true, hidden: section() !== "providers" }}>
            <ProvidersPage onRefresh={props.onRefresh} />
          </div>
          <div classList={{ "settings-pane": true, hidden: section() !== "general" }}>
            <GeneralPage
              workspace={props.workspace}
              language={props.language}
              onRefresh={props.onRefresh}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
