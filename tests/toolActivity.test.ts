import { describe, expect, test } from "bun:test";
import { analyzeShellCommand, analyzeToolActivity } from "../src/components/toolActivity";

function kinds(command: string): Record<string, number> | null {
  const activity = analyzeShellCommand(command);
  if (!activity) return null;
  return Object.fromEntries(activity.map((entry) => [entry.kind, entry.count]));
}

describe("analyzeToolActivity", () => {
  test("maps direct file tools", () => {
    expect(analyzeToolActivity("fs_read", { path: "a.txt" })).toEqual([{ kind: "read", count: 1 }]);
    expect(analyzeToolActivity("fs_list", { path: "." })).toEqual([{ kind: "list", count: 1 }]);
    expect(analyzeToolActivity("fs_write", { path: "a.txt" })).toBeNull();
  });

  test("classifies search, read and list shell commands", () => {
    expect(kinds("rg 'fn main' src")).toEqual({ search: 1 });
    expect(kinds("grep -R TODO .")).toEqual({ search: 1 });
    expect(kinds("cat src/main.rs")).toEqual({ read: 1 });
    expect(kinds("ls -la")).toEqual({ list: 1 });
    expect(kinds("git status")).toEqual({ read: 1 });
    expect(kinds("git ls-files")).toEqual({ list: 1 });
    expect(kinds("git grep TODO")).toEqual({ search: 1 });
  });

  test("combines segments and keeps plumbing out of the counts", () => {
    expect(kinds("cat a.txt | head -20")).toEqual({ read: 2 });
    expect(kinds("rg foo src && cat a.txt")).toEqual({ search: 1, read: 1 });
    expect(kinds("ls src; rg TODO src")).toEqual({ list: 1, search: 1 });
    expect(kinds("echo hello")).toBeNull();
  });

  test("keeps write-like and unknown commands out of the summary", () => {
    expect(kinds("rm -rf dist")).toBeNull();
    expect(kinds("git branch -D topic")).toBeNull();
    expect(kinds("git diff --output patch.txt")).toBeNull();
    expect(kinds("sed -i 's/a/b/' a.txt")).toBeNull();
    expect(kinds("find . -delete")).toBeNull();
    expect(kinds("cat a.txt > b.txt")).toBeNull();
    expect(kinds("echo $(rm x)")).toBeNull();
    expect(kinds("bun test")).toBeNull();
  });
});
