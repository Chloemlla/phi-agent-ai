// Mobile shell: a compact app bar for the chat surface, a modal session
// drawer, and a full-bleed page area. Settings takes over the whole viewport
// with its own drill-down chrome.

import { Show, type JSX } from "solid-js";
import { Sidebar } from "../components/Sidebar";
import { IconMenu } from "../components/Icons";
import { t } from "../i18n";

interface MobileShellProps {
  settingsOpen: boolean;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  onOpenSettings: () => void;
  onCloseSidebar: () => void;
  children: JSX.Element;
}

export function MobileShell(props: MobileShellProps) {
  return (
    <div classList={{ app: true, mobile: true, "drawer-mode": true }}>
      <Show when={!props.settingsOpen}>
        <header class="mobile-bar">
          <button
            type="button"
            class="icon-btn mobile-menu"
            title={props.sidebarOpen ? t("Close sidebar") : t("Expand sidebar")}
            aria-label={props.sidebarOpen ? t("Close sidebar") : t("Expand sidebar")}
            aria-expanded={props.sidebarOpen}
            onClick={() => props.onToggleSidebar()}
          >
            <IconMenu size={19} />
          </button>
          <div class="brand">
            <span class="brand-mark">φ</span>
            <span class="brand-name">Phi Agent</span>
          </div>
          <div class="mobile-bar-spacer" />
        </header>
      </Show>

      <Show
        when={!props.settingsOpen}
        fallback={
          <div class="shell shell--settings">
            <main class="main">{props.children}</main>
          </div>
        }
      >
        <div class="shell">
          <Show when={props.sidebarOpen}>
            <div
              class="sidebar-backdrop"
              aria-hidden="true"
              onClick={() => props.onCloseSidebar()}
            />
          </Show>
          <Sidebar
            open={props.sidebarOpen}
            modal
            settingsOpen={props.settingsOpen}
            onToggle={props.onToggleSidebar}
            onOpenSettings={props.onOpenSettings}
            onNavigate={props.onCloseSidebar}
          />
          <main class="main" inert={props.sidebarOpen ? true : undefined}>
            {props.children}
          </main>
        </div>
      </Show>
    </div>
  );
}
