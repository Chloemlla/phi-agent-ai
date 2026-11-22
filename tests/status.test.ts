import { describe, expect, test } from "bun:test";
import { riskLevel } from "../src/status";

describe("riskLevel", () => {
  test("uses the structured backend level first", () => {
    expect(riskLevel("high", "fs_write", "Reversible: data modification")).toBe("high");
    expect(riskLevel("low", "shell_exec", "Irreversible")).toBe("low");
  });

  test("falls back to stable tool ids for old journals", () => {
    expect(riskLevel("", "shell_exec", "whatever")).toBe("high");
    expect(riskLevel(undefined, "fs_write", "whatever")).toBe("medium");
  });

  test("falls back to English prose only when the tool is unknown", () => {
    expect(riskLevel("", "other", "This is irreversible")).toBe("high");
    expect(riskLevel("", "other", "Reversible change")).toBe("medium");
    expect(riskLevel("", "other", "Read only")).toBe("low");
  });
});
