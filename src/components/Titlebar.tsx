// Platform-aware title bar. Frameless desktop windows get native-style
// caption buttons on Windows/Linux and traffic lights on macOS. Mobile shells
// get a plain app bar with a drawer button instead of window controls.

import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { modeTone, stateLabel } from "../status";
import { isMobilePlatform } from "../types";
import {
  IconMenu,
  IconPanel,
  IconSettings,
  IconWinClose,
  IconWinMaximize,
  IconWinMinimize,
  IconWinRestore,
} from "./Icons";

type WindowHandle = ReturnType<typeof getCurrentWindow>;

function act(action: (window: WindowHandle) => Promise<unknown>) {
  try {
    void action(getCurrentWindow()).catch(() => undefined);
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
  platform: string;
  sidebarOpen: boolean;
  settingsOpen: boolean;
  onToggleSidebar: () => void;
  onOpenSettings: () => void;
}) {
  const [maximized, setMaximized] = createSignal(false);
  const mobile = () => isMobilePlatform(props.platform);
  const macos = () => props.platform === "macos";

  onMount(() => {
    if (mobile()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      const window = getCurrentWindow();
      const readMaximized = () => {
        void window
          .isMaximized()
          .then((value) => {
            if (!disposed) setMaximized(value);
          })
          .catch(() => undefined);
      };
      readMaximized();
      void window
        .onResized(readMaximized)
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch(() => undefined);
    } catch {
      // Browser preview: no window handle.
    }
    onCleanup(() => {
      disposed = true;
      unlisten?.();
    });
  });

  function toggleMaximize() {
    act((window) => window.toggleMaximize());
  }

  function handleDoubleClick(event: MouseEvent) {
    if (mobile() || macos()) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, select, textarea")) return;
    toggleMaximize();
  }

  const sidebarLabel = () =>
    props.sidebarOpen ? t("Collapse sidebar") : t("Expand sidebar");

  return (
    <header
      classList={{ titlebar: true, mobile: mobile(), mac: macos() }}
      onDblClick={handleDoubleClick}
      {...(mobile() ? {} : { "data-tauri-drag-region": true })}
    >
      <Show when={macos() && !mobile()}>
        <div class="win-controls mac">
          <button
            type="button"
            class="mac-dot close"
            title={t("Close window")}
            aria-label={t("Close window")}
            onClick={() => act((window) => window.close())}
          />
          <button
            type="button"
            class="mac-dot min"
            title={t("Minimize window")}
            aria-label={t("Minimize window")}
            onClick={() => act((window) => window.minimize())}
          />
          <button
            type="button"
            class="mac-dot max"
            title={t("Maximize window")}
            aria-label={t("Maximize window")}
            onClick={toggleMaximize}
          />
        </div>
      </Show>

      <Show when={mobile() && !props.settingsOpen}>
        <button
          type="button"
          class="icon-btn menu-toggle mobile-menu"
          title={sidebarLabel()}
          aria-label={sidebarLabel()}
          onClick={props.onToggleSidebar}
        >
          <IconMenu size={18} />
        </button>
      </Show>

      <Show when={!mobile()}>
        <div class="brand" data-tauri-drag-region>
          <span class="brand-mark" data-tauri-drag-region>
            φ
          </span>
          <span class="brand-name" data-tauri-drag-region>
            phi-agent
          </span>
        </div>
      </Show>

      <div class="titlebar-title" data-tauri-drag-region>
        <span class="tb-title" data-tauri-drag-region title={props.title}>
          {props.title}
        </span>
        <Show when={!mobile() && props.workspace}>
          <span
            class="tb-path mono"
            data-tauri-drag-region
            title={props.workspace ?? ""}
          >
            {props.workspace}
          </span>
        </Show>
      </div>

      <div class="titlebar-right">
        <Show when={!mobile()}>
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
          <Show when={!props.settingsOpen}>
            <button
              type="button"
              class="icon-btn menu-toggle"
              title={sidebarLabel()}
              aria-label={sidebarLabel()}
              onClick={props.onToggleSidebar}
            >
              <IconMenu size={16} />
            </button>
            <button
              type="button"
              class="icon-btn panel-toggle"
              title={sidebarLabel()}
              aria-label={sidebarLabel()}
              onClick={props.onToggleSidebar}
            >
              <IconPanel size={16} />
            </button>
          </Show>
          <button
            type="button"
            classList={{ "icon-btn": true, active: props.settingsOpen }}
            title={t("Settings")}
            aria-label={t("Settings")}
            onClick={props.onOpenSettings}
          >
            <IconSettings size={16} />
          </button>
        </Show>

        <Show when={mobile()}>
          <button
            type="button"
            classList={{ "icon-btn": true, active: props.settingsOpen }}
            title={t("Settings")}
            aria-label={t("Settings")}
            onClick={props.onOpenSettings}
          >
            <IconSettings size={18} />
          </button>
        </Show>

        <Show when={!mobile() && !macos()}>
          <div class="caption-controls">
            <button
              type="button"
              class="caption-btn"
              title={t("Minimize window")}
              aria-label={t("Minimize window")}
              onClick={() => act((window) => window.minimize())}
            >
              <IconWinMinimize />
            </button>
            <button
              type="button"
              class="caption-btn"
              title={maximized() ? t("Restore window") : t("Maximize window")}
              aria-label={maximized() ? t("Restore window") : t("Maximize window")}
              onClick={toggleMaximize}
            >
              <Show when={maximized()} fallback={<IconWinMaximize />}>
                <IconWinRestore />
              </Show>
            </button>
            <button
              type="button"
              class="caption-btn close"
              title={t("Close window")}
              aria-label={t("Close window")}
              onClick={() => act((window) => window.close())}
            >
              <IconWinClose />
            </button>
          </div>
        </Show>
      </div>
    </header>
  );
}
