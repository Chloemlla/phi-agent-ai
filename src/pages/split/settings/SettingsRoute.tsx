// Settings route: shared section state, draft guarding and hardware-back
// handling. The desktop and mobile views are lazy-loaded per shell, so a
// desktop build never downloads the mobile drill-down code and vice versa.

import { Show, Suspense, createEffect, createSignal, lazy, onCleanup, onMount } from "solid-js";
import { useChat } from "../../../core/chat/ChatProvider";
import type { SettingsSectionId } from "../../../core/settings/schema";
import { isMobileShell } from "../../../layout/environment";
import { SettingsPaneLoading } from "./SettingsShared";
import "../../../settings/Settings.css";

const DesktopSettings = lazy(() =>
  import("./DesktopSettings").then((module) => ({ default: module.DesktopSettings })),
);
const MobileSettings = lazy(() =>
  import("./MobileSettings").then((module) => ({ default: module.MobileSettings })),
);

let lastDesktopSection: SettingsSectionId = "general";

export function SettingsRoute(props: { onBack: () => void }) {
  const chat = useChat();
  const [section, setSection] = createSignal<SettingsSectionId>(lastDesktopSection);
  const [mobileSection, setMobileSection] = createSignal<SettingsSectionId | null>(null);
  const [visited, setVisited] = createSignal<ReadonlySet<SettingsSectionId>>(
    new Set([lastDesktopSection]),
  );
  const [providersDirty, setProvidersDirty] = createSignal(false);
  const [leavePrompt, setLeavePrompt] = createSignal(false);

  function select(next: SettingsSectionId) {
    setLeavePrompt(false);
    lastDesktopSection = next;
    setSection(next);
    setVisited((current) => (current.has(next) ? current : new Set(current).add(next)));
  }

  function openMobile(next: SettingsSectionId) {
    select(next);
    setMobileSection(next);
  }

  function showLeavePrompt() {
    setLeavePrompt(true);
  }

  function requestLeave() {
    if (providersDirty()) {
      showLeavePrompt();
      return;
    }
    props.onBack();
  }

  createEffect(() => {
    if (!providersDirty()) setLeavePrompt(false);
  });

  onMount(() => {
    // Closing Settings from the app shell must not discard a provider draft.
    const onLeave = (event: Event) => {
      if (!providersDirty()) return;
      event.preventDefault();
      showLeavePrompt();
    };
    // Android back: one level inside settings first (provider editor ->
    // provider list -> settings index -> chat). Views and pages emit this.
    const onBack = (event: Event) => {
      if (!isMobileShell()) return;
      if (mobileSection() !== null) {
        event.preventDefault();
        setMobileSection(null);
        return;
      }
      if (providersDirty()) {
        event.preventDefault();
        showLeavePrompt();
      }
    };
    window.addEventListener("phi:settings-leave", onLeave);
    window.addEventListener("phi:settings-back", onBack);
    onCleanup(() => {
      window.removeEventListener("phi:settings-leave", onLeave);
      window.removeEventListener("phi:settings-back", onBack);
    });
  });

  return (
    <Suspense fallback={<SettingsPaneLoading />}>
      <Show
        when={isMobileShell()}
        fallback={
          <DesktopSettings
            section={section()}
            visited={visited()}
            leavePrompt={leavePrompt()}
            onSelect={select}
            onBack={requestLeave}
            onDiscardLeave={props.onBack}
            onCancelLeave={() => setLeavePrompt(false)}
            workspace={chat.boot()?.workspace ?? ""}
            language={chat.boot()?.language ?? "en"}
            onRefresh={chat.refresh}
            onProvidersDirty={setProvidersDirty}
          />
        }
      >
        <MobileSettings
          section={section()}
          visited={visited()}
          mobileSection={mobileSection()}
          leavePrompt={leavePrompt()}
          onOpen={openMobile}
          onCloseSection={() => setMobileSection(null)}
          onBack={requestLeave}
          onDiscardLeave={props.onBack}
          onCancelLeave={() => setLeavePrompt(false)}
          workspace={chat.boot()?.workspace ?? ""}
          language={chat.boot()?.language ?? "en"}
          onRefresh={chat.refresh}
          onProvidersDirty={setProvidersDirty}
        />
      </Show>
    </Suspense>
  );
}
