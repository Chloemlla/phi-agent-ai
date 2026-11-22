// Section panes shared by the desktop rail and the mobile drill-down. Panes
// mount on first visit and then stay mounted (hidden when inactive), so an
// in-progress provider draft survives section switches in both views.

import { Show, Suspense, lazy } from "solid-js";
import type { SettingsSectionId } from "../../../core/settings/schema";
import { SettingsPaneLoading } from "./SettingsShared";

const GeneralPage = lazy(() =>
  import("../../../settings/GeneralPage").then((module) => ({ default: module.GeneralPage })),
);
const ProvidersPage = lazy(() =>
  import("../../../settings/ProvidersPage").then((module) => ({ default: module.ProvidersPage })),
);
const MemoryPage = lazy(() =>
  import("../../../settings/MemoryPage").then((module) => ({ default: module.MemoryPage })),
);

interface SettingsSectionsProps {
  section: SettingsSectionId;
  visited: ReadonlySet<SettingsSectionId>;
  /** Hide every pane without unmounting them (mobile settings index). */
  hidden?: boolean;
  workspace: string;
  language: string;
  onRefresh: () => Promise<void>;
  onProvidersDirty: (dirty: boolean) => void;
}

export function SettingsSections(props: SettingsSectionsProps) {
  return (
    <div class="settings-panes" hidden={props.hidden}>
      <Show when={props.visited.has("general")}>
        <div class="settings-pane" hidden={props.section !== "general"}>
          <Suspense fallback={<SettingsPaneLoading />}>
            <GeneralPage
              workspace={props.workspace}
              language={props.language}
              onRefresh={props.onRefresh}
            />
          </Suspense>
        </div>
      </Show>
      <Show when={props.visited.has("providers")}>
        <div class="settings-pane" hidden={props.section !== "providers"}>
          <Suspense fallback={<SettingsPaneLoading />}>
            <ProvidersPage
              active={props.section === "providers" && !props.hidden}
              onDirtyChange={props.onProvidersDirty}
              onRefresh={props.onRefresh}
            />
          </Suspense>
        </div>
      </Show>
      <Show when={props.visited.has("memory")}>
        <div class="settings-pane" hidden={props.section !== "memory"}>
          <Suspense fallback={<SettingsPaneLoading />}>
            <MemoryPage onRefresh={props.onRefresh} />
          </Suspense>
        </div>
      </Show>
    </div>
  );
}
