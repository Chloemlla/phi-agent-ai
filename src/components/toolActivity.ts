// Presentation-only classification of tool calls into read/search/list
// activity. It decides whether consecutive low-noise calls can be summarized
// as "Read 3 files and searched for 2 patterns"; it is not a safety check and
// must never gate execution. The policy plugin owns the security whitelist.
//
// Shell classification mirrors the reference project: commands are split on
// shell separators, each segment is classified, and any unknown or write-like
// segment makes the whole call non-compressible so it stays a normal row.

export type ToolActivityKind = "read" | "search" | "list";

export type ToolActivityCount = {
  kind: ToolActivityKind;
  count: number;
};

const READ_COMMANDS = new Set(["cat", "file", "head", "sed", "sort", "stat", "tail", "type", "wc"]);

const SEARCH_COMMANDS = new Set(["ack", "ag", "findstr", "grep", "rg"]);

const LIST_COMMANDS = new Set(["dir", "find", "ls", "pwd", "tree", "where", "which"]);

const SHELL_PLUMBING_COMMANDS = new Set(["echo", "printf", "true", "tr", "uniq"]);

const READ_GIT_COMMANDS = new Set([
  "branch",
  "describe",
  "diff",
  "log",
  "rev-parse",
  "show",
  "status",
]);

const SEARCH_GIT_COMMANDS = new Set(["grep"]);

const LIST_GIT_COMMANDS = new Set(["ls-files"]);

/** Analyze a tool call, returning null when it has no compressible activity. */
export function analyzeToolActivity(toolId: string, input: unknown): ToolActivityCount[] | null {
  if (toolId === "fs_read") return [{ kind: "read", count: 1 }];
  if (toolId === "fs_list") return [{ kind: "list", count: 1 }];
  if (toolId !== "shell_exec") return null;
  const command = stringField(input, "command");
  return command ? analyzeShellCommand(command) : null;
}

/** Classify a shell command; null means "keep this call as a normal row". */
export function analyzeShellCommand(command: string): ToolActivityCount[] | null {
  const normalized = removeSafeShellRedirections(command);
  // Command substitution, redirection, backticks and cmd escapes can hide
  // writes or nested commands; leave those calls uncompressed.
  if (!normalized.trim() || /[<>`]|\$\(|\$\{|%[A-Za-z_]+%|\^/.test(normalized)) {
    return null;
  }
  if (normalized.includes("&") && !normalized.includes("&&")) return null;

  const segments = splitShellSegments(normalized);
  if (segments.length === 0) return null;

  const counts: Record<ToolActivityKind, number> = { read: 0, search: 0, list: 0 };
  let meaningful = false;
  for (const segment of segments) {
    const words = shellWords(segment);
    if (words === null || words.length === 0) return null;
    const activity = classifyShellSegment(words[0] ?? "", words.slice(1));
    if (activity === null) return null;
    for (const entry of activity) counts[entry.kind] += entry.count;
    meaningful ||= activity.length > 0;
  }

  if (!meaningful) return null;
  return (Object.entries(counts) as [ToolActivityKind, number][])
    .filter(([, count]) => count > 0)
    .map(([kind, count]) => ({ kind, count }));
}

function stringField(input: unknown, key: string): string {
  if (!input || typeof input !== "object") return "";
  const value = (input as Record<string, unknown>)[key];
  return typeof value === "string" ? value : "";
}

function splitShellSegments(command: string): string[] {
  const segments: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaped = false;
  for (let index = 0; index < command.length; index += 1) {
    const character = command[index];
    const next = command[index + 1];
    if (escaped) {
      current += character;
      escaped = false;
      continue;
    }
    if (character === "\\" && quote !== "'") {
      current += character;
      escaped = true;
      continue;
    }
    if (quote) {
      current += character;
      if (character === quote) quote = null;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      current += character;
      continue;
    }
    if (
      character === ";" ||
      character === "\n" ||
      character === "|" ||
      (character === "&" && next === "&")
    ) {
      if (current.trim()) segments.push(current.trim());
      current = "";
      if (character === "&") index += 1;
      continue;
    }
    current += character;
  }
  if (current.trim()) segments.push(current.trim());
  if (quote !== null) return [];
  return segments;
}

function shellWords(segment: string): string[] | null {
  const words: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaped = false;
  for (let index = 0; index < segment.length; index += 1) {
    const character = segment[index];
    if (character === undefined) continue;
    if (escaped) {
      current += character;
      escaped = false;
      continue;
    }
    if (character === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote !== null) {
      if (character === quote) quote = null;
      else current += character;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      if (current) words.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  if (escaped || quote !== null) return null;
  if (current) words.push(current);
  return words;
}

function classifyShellSegment(
  rawCommand: string,
  args: readonly string[],
): readonly ToolActivityCount[] | null {
  const base = rawCommand.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  const command = base.endsWith(".exe") ? base.slice(0, -4) : base;
  if (!command) return null;
  if (command === "echo" || SHELL_PLUMBING_COMMANDS.has(command)) return [];
  if (READ_COMMANDS.has(command)) {
    if (command === "sed" && args.some((arg) => arg === "--in-place" || /-[^-]*i/.test(arg))) {
      return null;
    }
    if (command === "sort" && args.some((arg) => arg === "-o" || arg === "--output")) {
      return null;
    }
    return [{ kind: "read", count: 1 }];
  }
  if (SEARCH_COMMANDS.has(command)) return [{ kind: "search", count: 1 }];
  if (LIST_COMMANDS.has(command)) {
    if (
      command === "find" &&
      args.some((arg) =>
        ["-exec", "-execdir", "-delete", "-ok", "-fprint", "-fprintf", "-fls"].includes(arg),
      )
    ) {
      return null;
    }
    return [{ kind: "list", count: 1 }];
  }
  if (command === "git") return classifyGitSegment(args);
  if (command === "bash" || command === "sh" || command === "zsh" || command === "cmd") {
    const commandIndex = args.findIndex((arg) =>
      ["-c", "-lc", "--command", "/c", "/C"].includes(arg),
    );
    const script = commandIndex >= 0 ? args[commandIndex + 1] : undefined;
    return script ? analyzeShellCommand(script) : null;
  }
  return null;
}

function classifyGitSegment(args: readonly string[]): readonly ToolActivityCount[] | null {
  const subcommand = args[0]?.toLowerCase();
  if (!subcommand) return null;
  if (subcommand === "branch") {
    if (
      args.some((arg) =>
        ["-d", "-D", "-m", "-M", "-f", "--delete", "--move", "--force"].includes(arg),
      )
    ) {
      return null;
    }
    return [{ kind: "read", count: 1 }];
  }
  if (subcommand === "diff") {
    if (args.some((arg) => arg === "--output" || arg.startsWith("--output="))) return null;
    return [{ kind: "read", count: 1 }];
  }
  if (READ_GIT_COMMANDS.has(subcommand)) return [{ kind: "read", count: 1 }];
  if (SEARCH_GIT_COMMANDS.has(subcommand)) return [{ kind: "search", count: 1 }];
  if (LIST_GIT_COMMANDS.has(subcommand)) return [{ kind: "list", count: 1 }];
  return null;
}

/** Ignore output-only stderr/stdout redirection when deciding compressibility. */
function removeSafeShellRedirections(command: string): string {
  return command.replace(/(?:^|\s)(?:[12]>\s*\/dev\/null|2>&1)(?=\s|[;&|]|$)/g, " ").trim();
}
