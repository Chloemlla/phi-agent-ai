import { describe, expect, test } from "bun:test";
import {
  SETTINGS_SECTIONS,
  settingsGroupsFor,
  settingsSection,
  settingsSectionsFor,
} from "../src/core/settings/schema";

describe("settings schema", () => {
  test("general is the first section and both shells see the same order", () => {
    for (const scope of ["desktop", "mobile"] as const) {
      const sections = settingsSectionsFor(scope);
      expect(sections[0]?.id).toBe("general");
      expect(sections.map((section) => section.id)).toEqual(["general", "providers", "memory"]);
    }
  });

  test("groups respect the section order and hide empty groups", () => {
    expect(settingsGroupsFor("desktop").map((group) => group.id)).toEqual(["general", "advanced"]);
    expect(SETTINGS_SECTIONS.every((section) => section.scope === "all")).toBe(true);
  });

  test("section lookup returns the full specification", () => {
    expect(settingsSection("providers").label).toBe("Model providers");
    expect(settingsSection("memory").group).toBe("advanced");
  });
});
