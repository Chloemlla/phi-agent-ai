// Settings information architecture. This module is UI-free: the desktop rail
// and the mobile drill-down list both render this schema, and the scope field
// decides which sections a shell may show at all.

export type SettingsScope = "desktop" | "mobile";
export type SettingsSectionId = "general" | "providers" | "memory";
export type SettingsGroupId = "general" | "advanced";

export interface SettingsSectionSpec {
  id: SettingsSectionId;
  group: SettingsGroupId;
  label: string;
  description: string;
  /** Sections apply to both shells unless a future feature is end-specific. */
  scope: "all" | SettingsScope;
}

export interface SettingsGroupSpec {
  id: SettingsGroupId;
  label: string;
}

export const SETTINGS_GROUPS: readonly SettingsGroupSpec[] = [
  { id: "general", label: "Preferences" },
  { id: "advanced", label: "Advanced" },
];

export const SETTINGS_SECTIONS: readonly SettingsSectionSpec[] = [
  {
    id: "general",
    group: "general",
    label: "General",
    description: "Workspace boundary, language and interface preferences.",
    scope: "all",
  },
  {
    id: "providers",
    group: "advanced",
    label: "Model providers",
    description: "Endpoints, credentials and the models each provider offers.",
    scope: "all",
  },
  {
    id: "memory",
    group: "advanced",
    label: "Memory",
    description: "Inspect, search and prune what the agent remembers.",
    scope: "all",
  },
];

export function settingsSectionsFor(scope: SettingsScope): SettingsSectionSpec[] {
  return SETTINGS_SECTIONS.filter((section) => section.scope === "all" || section.scope === scope);
}

export function settingsGroupsFor(scope: SettingsScope): SettingsGroupSpec[] {
  const visible = new Set(settingsSectionsFor(scope).map((section) => section.group));
  return SETTINGS_GROUPS.filter((group) => visible.has(group.id));
}

export function settingsSection(id: SettingsSectionId): SettingsSectionSpec {
  const found = SETTINGS_SECTIONS.find((section) => section.id === id);
  if (!found) throw new Error(`Unknown settings section: ${id}`);
  return found;
}
