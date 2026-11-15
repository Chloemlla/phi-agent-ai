// Frameless window title bar: custom drag region, window controls, the
// session identity in the middle, and node status on the right.

import { Show } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { modeTone, stateLabel } from "../status";
import { IconPanel, IconSettings } from "./Icons";

function act(action: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
  try {
    action(getCurrentWindow()).catch(() => undefined);
  } catch {
    // The window API only exists inside the desktop shell; browser previews
    // simply render the title bar without window controls.
  }
}

export function Titlebar(props: {
  title: string;
  workspace: string | null;
  mode: string;
  modeReason: string;
  sidebarOpen: boolean;
  settingsOpen: boolean;
  onToggleSidebar: () => void;
  onOpenSettings: () => void;
}) {
  function handleDoubleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    if (target?.hasAttribute("data-tauri-drag-region")) {
      act((window) => window.toggleMaximize());
    }
  }

  return (
    <header
      class="titlebar"
      data-tauri-drag-region
      onDblClick={handleDoubleClick}
    >
      <div class="titlebar-left">
        <div class="win-controls">
          <button
            class="win-dot close"
            title={t("Close window")}
            aria-label={t("Close window")}
            onClick={() => act((window) => window.close())}
          />
          <button
            class="win-dot min"
            title={t("Minimize window")}
            aria-label={t("Minimize window")}
            onClick={() => act((window) => window.minimize())}
          />
          <button
            class="win-dot max"
            title={t("Maximize window")}
            aria-label={t("Maximize window")}
            onClick={() => act((window) => window.toggleMaximize())}
          />
        </div>
        <div class="brand" data-tauri-drag-region>
          <span class="brand-mark" data-tauri-drag-region>
            φ
          </span>
          <span class="brand-name" data-tauri-drag-region>
            phi-agent
          </span>
        </div>
      </div>

      <div class="titlebar-center" data-tauri-drag-region>
        <span class="tb-title" data-tauri-drag-region>
          {props.title}
        </span>
        <span class="tb-path mono" data-tauri-drag-region title={props.workspace ?? ""}>
          {props.workspace ?? ""}
        </span>
      </div>

      <div class="titlebar-right">
        <Show when={props.mode}>
          <span
            classList={{
              "mode-pill": true,
              [`tone-${modeTone(props.mode)}`]: true,
            }}
            title={props.modeReason}
          >
            <span class="dot" />
            {stateLabel(props.mode)}
          </span>
        </Show>
        <button
          classList={{ "icon-btn": true, active: props.settingsOpen }}
          title={t("Settings")}
          aria-label={t("Settings")}
          onClick={props.onOpenSettings}
        >
          <IconSettings size={16} />
        </button>
        <button
          class="icon-btn"
          title={props.sidebarOpen ? t("Collapse sidebar") : t("Expand sidebar")}
          aria-label={props.sidebarOpen ? t("Collapse sidebar") : t("Expand sidebar")}
          onClick={props.onToggleSidebar}
        >
          <IconPanel size={16} />
        </button>
      </div>
    </header>
  );
}
