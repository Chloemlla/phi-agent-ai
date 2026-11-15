// Full-page settings shell: a local navigation rail and the section panes. All
// panes stay mounted so switching sections never discards a provider draft.

import { createSignal } from "solid-js";
import { t } from "../i18n";
import { IconArrowLeft, IconBrain, IconServer, IconSettings } from "../components/Icons";
import { GeneralPage } from "./GeneralPage";
import { MemoryPage } from "./MemoryPage";
import { ProvidersPage } from "./ProvidersPage";
import "./Settings.css";

type Section = "providers" | "memory" | "general";

export function SettingsPage(props: {
  workspace: string;
  language: string;
  onBack: () => void;
  onRefresh: () => Promise<void>;
}) {
  const [section, setSection] = createSignal<Section>("providers");
  let contentRef: HTMLDivElement | undefined;

  function select(next: Section) {
    if (section() === next) return;
    setSection(next);
    // Each pane keeps its state, but a new section always starts at the top.
    queueMicrotask(() => contentRef?.scrollTo({ top: 0 }));
  }

  return (
    <div class="settings-page">
      <aside class="settings-nav" aria-label={t("Settings")}>
        <button class="settings-back" onClick={props.onBack}>
          <IconArrowLeft size={15} />
          {t("Back to chat")}
        </button>
        <div class="side-label">{t("Settings")}</div>
        <button
          classList={{ "settings-nav-item": true, active: section() === "providers" }}
          onClick={() => select("providers")}
        >
          <IconServer size={15} />
          {t("Model providers")}
        </button>
        <button
          classList={{ "settings-nav-item": true, active: section() === "memory" }}
          onClick={() => select("memory")}
        >
          <IconBrain size={15} />
          {t("Memory")}
        </button>
        <button
          classList={{ "settings-nav-item": true, active: section() === "general" }}
          onClick={() => select("general")}
        >
          <IconSettings size={15} />
          {t("General")}
        </button>
        <div class="settings-nav-foot">{t("Settings are stored on this node.")}</div>
      </aside>

      <div class="settings-content" ref={contentRef}>
        <div class="settings-content-inner">
          <div classList={{ "settings-pane": true, hidden: section() !== "providers" }}>
            <ProvidersPage onRefresh={props.onRefresh} />
          </div>
          <div classList={{ "settings-pane": true, hidden: section() !== "memory" }}>
            <MemoryPage onRefresh={props.onRefresh} />
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
