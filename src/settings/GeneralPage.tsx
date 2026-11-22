// General settings: workspace boundary and interface language.

import { Show, createSignal } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { setLanguage as setNodeLanguage, setWorkspace as setNodeWorkspace } from "../bridge";
import { setLang, t } from "../i18n";
import { errorText } from "../errors";
import { IconCheck, IconFolderOpen } from "../components/Icons";

export function GeneralPage(props: {
  workspace: string;
  language: string;
  onRefresh: () => Promise<void>;
}) {
  const [workspace, setWorkspace] = createSignal(props.workspace);
  const [language, setLanguage] = createSignal(props.language === "zh-CN" ? "zh-CN" : "en");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal<"workspace" | "language" | null>(null);

  async function chooseWorkspace() {
    setError("");
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: workspace() || undefined,
      });
      if (typeof picked === "string" && picked) setWorkspace(picked);
    } catch (e) {
      setError(errorText(e));
    }
  }

  async function saveWorkspace() {
    setError("");
    setBusy(true);
    try {
      const savedPath = await setNodeWorkspace(workspace());
      setWorkspace(savedPath);
      await props.onRefresh().catch(() => undefined);
      setSaved("workspace");
      window.setTimeout(() => setSaved(null), 1800);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  async function saveLanguage() {
    setError("");
    setBusy(true);
    try {
      const savedLanguage = await setNodeLanguage(language());
      setLang(savedLanguage);
      await props.onRefresh().catch(() => undefined);
      setSaved("language");
      window.setTimeout(() => setSaved(null), 1800);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="settings-section general-page">
      <div class="settings-page-head">
        <div>
          <h1>{t("General")}</h1>
          <p class="settings-subtitle">
            {t("Workspace boundary, language and the node this window is connected to.")}
          </p>
        </div>
      </div>

      <Show when={error()}>
        <div class="banner danger" role="alert">
          {error()}
        </div>
      </Show>

      <section class="settings-card">
        <h3>{t("Workspace")}</h3>
        <p class="hint">{t("The folder the agent may read and edit.")}</p>
        <div class="settings-actions">
          <input
            class="mono settings-input"
            value={workspace()}
            onInput={(event) => setWorkspace(event.currentTarget.value)}
            placeholder={t("Absolute workspace path")}
            aria-label={t("Workspace")}
          />
          <button
            class="btn"
            type="button"
            disabled={busy()}
            onClick={() => void chooseWorkspace()}
          >
            <IconFolderOpen size={14} />
            {t("Choose folder")}
          </button>
          <button class="btn primary" disabled={busy()} onClick={() => void saveWorkspace()}>
            {t("Save")}
          </button>
          <Show when={saved() === "workspace"}>
            <span class="saved-note" role="status">
              <IconCheck size={14} />
              {t("Saved")}
            </span>
          </Show>
        </div>
      </section>

      <section class="settings-card">
        <h3>{t("Interface language")}</h3>
        <p class="hint">{t("Changes apply to this node and to every shell connected to it.")}</p>
        <div class="settings-actions">
          <select
            class="settings-input"
            value={language()}
            aria-label={t("Interface language")}
            onChange={(event) => setLanguage(event.currentTarget.value)}
          >
            <option value="en">{t("English")}</option>
            <option value="zh-CN">{t("Chinese (Simplified)")}</option>
          </select>
          <button class="btn primary" disabled={busy()} onClick={() => void saveLanguage()}>
            {t("Save")}
          </button>
          <Show when={saved() === "language"}>
            <span class="saved-note" role="status">
              <IconCheck size={14} />
              {t("Saved")}
            </span>
          </Show>
        </div>
      </section>
    </div>
  );
}
