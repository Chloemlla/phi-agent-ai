// Full-page settings shell: a local navigation rail and the section panes.
// Panes mount on first visit and then stay mounted, so switching sections never
// discards a provider draft (and Memory is not queried until it is opened).

import {
  Show,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { t } from "../i18n";
import { IconArrowLeft, IconBrain, IconServer, IconSettings } from "../components/Icons";
import { GeneralPage } from "./GeneralPage";
import { MemoryPage } from "./MemoryPage";
import { ProvidersPage } from "./ProvidersPage";
import "./Settings.css";

type Section = "providers" | "memory" | "general";

let lastSection: Section = "providers";

export function SettingsPage(props: {
  workspace: string;
  language: string;
  onBack: () => void;
  onRefresh: () => Promise<void>;
}) {
  const [section, setSection] = createSignal<Section>(lastSection);
  const [visited, setVisited] = createSignal<ReadonlySet<Section>>(
    new Set([lastSection]),
  );
  const [providersDirty, setProvidersDirty] = createSignal(false);
  const [leavePrompt, setLeavePrompt] = createSignal(false);
  let contentRef: HTMLElement | undefined;

  function showLeavePrompt() {
    setLeavePrompt(true);
    queueMicrotask(() => contentRef?.scrollTo({ top: 0 }));
  }

  function requestLeave() {
    if (providersDirty()) {
      showLeavePrompt();
      return;
    }
    props.onBack();
  }

  function select(next: Section) {
    if (section() === next) return;
    lastSection = next;
    setLeavePrompt(false);
    setSection(next);
    setVisited((prev) => (prev.has(next) ? prev : new Set(prev).add(next)));
    // Each pane keeps its state, but a new section always starts at the top.
    queueMicrotask(() => contentRef?.scrollTo({ top: 0 }));
  }

  createEffect(() => {
    if (!providersDirty()) setLeavePrompt(false);
  });

  // App asks before closing Settings so the provider draft is not lost.
  onMount(() => {
    const onLeave = (event: Event) => {
      if (!providersDirty()) return;
      event.preventDefault();
      showLeavePrompt();
    };
    window.addEventListener("phi:settings-leave", onLeave);
    onCleanup(() => window.removeEventListener("phi:settings-leave", onLeave));
  });

  return (
    <div class="settings-page">
      <nav class="settings-nav" aria-label={t("Settings")}>
        <button class="settings-back" type="button" onClick={requestLeave}>
          <IconArrowLeft size={15} />
          {t("Back to chat")}
        </button>
        <div class="side-label">{t("Settings")}</div>
        <button
          type="button"
          aria-current={section() === "providers" ? "page" : undefined}
          classList={{ "settings-nav-item": true, active: section() === "providers" }}
          onClick={() => select("providers")}
        >
          <IconServer size={15} />
          {t("Model providers")}
        </button>
        <button
          type="button"
          aria-current={section() === "memory" ? "page" : undefined}
          classList={{ "settings-nav-item": true, active: section() === "memory" }}
          onClick={() => select("memory")}
        >
          <IconBrain size={15} />
          {t("Memory")}
        </button>
        <button
          type="button"
          aria-current={section() === "general" ? "page" : undefined}
          classList={{ "settings-nav-item": true, active: section() === "general" }}
          onClick={() => select("general")}
        >
          <IconSettings size={15} />
          {t("General")}
        </button>
        <div class="settings-nav-foot">{t("Settings are stored on this node.")}</div>
      </nav>

      <main class="settings-content" ref={contentRef}>
        <Show when={leavePrompt()}>
          <div class="draft-guard settings-leave-guard" role="alert">
            <span>{t("Discard unsaved changes?")}</span>
            <div class="draft-guard-actions">
              <button
                type="button"
                class="btn danger"
                onClick={() => props.onBack()}
              >
                {t("Discard")}
              </button>
              <button
                type="button"
                class="btn"
                onClick={() => setLeavePrompt(false)}
              >
                {t("Cancel")}
              </button>
            </div>
          </div>
        </Show>
        <div class="settings-content-inner">
          <Show when={visited().has("providers")}>
            <div classList={{ "settings-pane": true, hidden: section() !== "providers" }}>
              <ProvidersPage
                active={section() === "providers"}
                onDirtyChange={setProvidersDirty}
                onRefresh={props.onRefresh}
              />
            </div>
          </Show>
          <Show when={visited().has("memory")}>
            <div classList={{ "settings-pane": true, hidden: section() !== "memory" }}>
              <MemoryPage onRefresh={props.onRefresh} />
            </div>
          </Show>
          <Show when={visited().has("general")}>
            <div classList={{ "settings-pane": true, hidden: section() !== "general" }}>
              <GeneralPage
                workspace={props.workspace}
                language={props.language}
                onRefresh={props.onRefresh}
              />
            </div>
          </Show>
        </div>
        <p class="settings-content-foot hint">
          {t("Settings are stored on this node.")}
        </p>
      </main>
    </div>
  );
}
