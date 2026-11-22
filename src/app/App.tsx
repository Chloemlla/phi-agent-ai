// Application assembly: providers, shell choice and the chat/settings switch.
// No chat or settings business logic lives here.

import { Show, Suspense, createEffect, createSignal, lazy, onCleanup, onMount } from "solid-js";
import { ChatPage } from "../pages/shared/ChatPage";
import { ChatProvider, useChat } from "../core/chat/ChatProvider";
import { NotificationContainer } from "../ui/notifications";
import { isDrawerMode, isMobileShell, initEnvironment, shellKind } from "../layout/environment";
import { platform } from "../platform";
import { DesktopShell } from "../shells/DesktopShell";
import { MobileShell } from "../shells/MobileShell";
import { t } from "../i18n";
import "../styles/app.css";

const SettingsRoute = lazy(() =>
  import("../pages/split/settings/SettingsRoute").then((module) => ({
    default: module.SettingsRoute,
  })),
);

function AppFrame() {
  const chat = useChat();
  const [view, setView] = createSignal<"chat" | "settings">("chat");
  const [sidebarOpen, setSidebarOpen] = createSignal(!isDrawerMode());
  let sidebarInitialized = false;
  let wasDrawer = isDrawerMode();

  function closeSidebar() {
    setSidebarOpen(false);
  }

  function openSettings() {
    if (isDrawerMode()) closeSidebar();
    setView("settings");
  }

  function requestCloseSettings() {
    // Settings may hold an unsaved provider draft and can cancel this event.
    if (window.dispatchEvent(new CustomEvent("phi:settings-leave", { cancelable: true }))) {
      setView("chat");
    }
  }

  function backToChat() {
    setView("chat");
  }

  onMount(() => {
    const stopEnvironment = initEnvironment();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && sidebarOpen() && isDrawerMode()) {
        closeSidebar();
      }
    };
    document.addEventListener("keydown", onKeyDown);

    let unlistenBack: (() => void) | undefined;

    void platform
      .onBackButton((canGoBack) => {
        if (view() === "settings") {
          // Let a settings pane consume one level first (provider detail,
          // settings index), then let the settings shell guard drafts.
          if (window.dispatchEvent(new CustomEvent("phi:settings-back", { cancelable: true }))) {
            requestCloseSettings();
          }
          return;
        }
        if (sidebarOpen() && isDrawerMode()) {
          closeSidebar();
          return;
        }
        if (canGoBack) {
          window.history.back();
          return;
        }
        void platform.exitApp().catch(() => undefined);
      })
      .then((listener) => {
        unlistenBack = listener;
      });

    onCleanup(() => {
      document.removeEventListener("keydown", onKeyDown);
      stopEnvironment();
      unlistenBack?.();
    });
  });

  // The first bootstrap decides whether the rail starts open.
  createEffect(() => {
    const bootstrap = chat.boot();
    if (!bootstrap || sidebarInitialized) return;
    sidebarInitialized = true;
    setSidebarOpen(!isDrawerMode());
  });

  // Crossing into drawer mode must not leave a backdrop-less open drawer.
  createEffect(() => {
    const drawer = isDrawerMode();
    if (drawer && !wasDrawer) closeSidebar();
    wasDrawer = drawer;
  });

  // Mobile notifications need the taller app-bar offset.
  createEffect(() => {
    document.documentElement.dataset.shell = shellKind();
  });

  const content = () => (
    <Show when={view() === "settings"} fallback={<ChatPage />}>
      <Suspense
        fallback={
          <div class="settings-route-loading" role="status">
            <span class="ring" aria-hidden="true" />
            <span>{t("Loading…")}</span>
          </div>
        }
      >
        <SettingsRoute onBack={backToChat} />
      </Suspense>
    </Show>
  );

  return (
    <Show
      when={isMobileShell()}
      fallback={
        <DesktopShell
          settingsOpen={view() === "settings"}
          sidebarOpen={sidebarOpen()}
          onToggleSidebar={() => setSidebarOpen((open) => !open)}
          onOpenSettings={openSettings}
          onCloseSidebar={closeSidebar}
        >
          {content()}
        </DesktopShell>
      }
    >
      <MobileShell
        settingsOpen={view() === "settings"}
        sidebarOpen={sidebarOpen()}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onOpenSettings={openSettings}
        onCloseSidebar={closeSidebar}
      >
        {content()}
      </MobileShell>
    </Show>
  );
}

function App() {
  return (
    <ChatProvider>
      <AppFrame />
      <NotificationContainer />
    </ChatProvider>
  );
}

export default App;
