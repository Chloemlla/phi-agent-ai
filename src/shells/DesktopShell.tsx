// Desktop shell: frameless title bar with only the brand and window controls,
// a persistent session rail, and the page surface. Navigation geometry lives
// here; the page inside never knows whether the rail is open or a drawer.

import { Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { Sidebar } from "../components/Sidebar";
import {
  IconMenu,
  IconWinClose,
  IconWinMaximize,
  IconWinMinimize,
  IconWinRestore,
} from "../components/Icons";
import { t } from "../i18n";
import { isDrawerMode, isMacPlatform } from "../layout/environment";
import { platform } from "../platform";

interface DesktopShellProps {
  settingsOpen: boolean;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  onOpenSettings: () => void;
  onCloseSidebar: () => void;
  children: JSX.Element;
}

export function DesktopShell(props: DesktopShellProps) {
  const [maximized, setMaximized] = createSignal(false);
  const mac = isMacPlatform();
  const drawer = () => isDrawerMode();

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void platform.isMaximized().then((value) => {
      if (!disposed) setMaximized(value);
    });
    void platform
      .onWindowResized(() => {
        void platform.isMaximized().then((value) => {
          if (!disposed) setMaximized(value);
        });
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    onCleanup(() => {
      disposed = true;
      unlisten?.();
    });
  });

  function toggleMaximize() {
    platform.toggleMaximize();
  }

  function handleDoubleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, select, textarea")) return;
    toggleMaximize();
  }

  const drawerOpen = () => drawer() && props.sidebarOpen;
  const showReveal = () => !props.settingsOpen && !props.sidebarOpen;

  return (
    <div classList={{ app: true, "drawer-mode": drawer() }}>
      <header
        class="titlebar"
        classList={{ mac }}
        onDblClick={handleDoubleClick}
        data-tauri-drag-region
      >
        <Show when={mac}>
          <div class="win-controls mac">
            <button
              type="button"
              class="mac-dot close"
              title={t("Close window")}
              aria-label={t("Close window")}
              onClick={() => platform.closeWindow()}
            />
            <button
              type="button"
              class="mac-dot min"
              title={t("Minimize window")}
              aria-label={t("Minimize window")}
              onClick={() => platform.minimizeWindow()}
            />
            <button
              type="button"
              class="mac-dot max"
              title={maximized() ? t("Restore window") : t("Maximize window")}
              aria-label={maximized() ? t("Restore window") : t("Maximize window")}
              onClick={toggleMaximize}
            />
          </div>
        </Show>

        <div class="brand" data-tauri-drag-region>
          <span class="brand-mark" data-tauri-drag-region>
            φ
          </span>
          <span class="brand-name" data-tauri-drag-region>
            Phi Agent
          </span>
        </div>

        <div class="titlebar-drag" data-tauri-drag-region />

        <Show when={!mac}>
          <div class="caption-controls">
            <button
              type="button"
              class="caption-btn"
              title={t("Minimize window")}
              aria-label={t("Minimize window")}
              onClick={() => platform.minimizeWindow()}
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
              onClick={() => platform.closeWindow()}
            >
              <IconWinClose />
            </button>
          </div>
        </Show>
      </header>

      <Show
        when={!props.settingsOpen}
        fallback={
          <div class="shell shell--settings">
            <main class="main">{props.children}</main>
          </div>
        }
      >
        <div class="shell" classList={{ "rail-collapsed": !props.sidebarOpen }}>
          <Show when={drawerOpen()}>
            <div
              class="sidebar-backdrop"
              aria-hidden="true"
              onClick={() => props.onCloseSidebar()}
            />
          </Show>
          <Sidebar
            open={props.sidebarOpen}
            modal={drawer()}
            settingsOpen={props.settingsOpen}
            onToggle={props.onToggleSidebar}
            onOpenSettings={props.onOpenSettings}
            onNavigate={() => {
              if (drawer()) props.onCloseSidebar();
            }}
          />
          <main class="main" inert={drawerOpen() ? true : undefined}>
            <Show when={showReveal()}>
              <button
                type="button"
                class="shell-reveal"
                title={t("Expand sidebar")}
                aria-label={t("Expand sidebar")}
                onClick={() => props.onToggleSidebar()}
              >
                <IconMenu size={17} />
              </button>
            </Show>
            {props.children}
          </main>
        </div>
      </Show>
    </div>
  );
}
