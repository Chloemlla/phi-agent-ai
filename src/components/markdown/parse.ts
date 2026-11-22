// Pure Markdown parsing. The parser returns plain data; `Markdown.tsx` turns
// the nodes into Solid JSX. It is intentionally small and streaming-friendly:
// an unterminated fence keeps rendering as code, and raw HTML is only passed
// along as a block for the allowlist-based `RichHtml` renderer.

const SAFE_LINK = /^(https?:\/\/|mailto:)/i;

function safeUrl(raw: string): string | null {
  const url = raw.trim();
  return SAFE_LINK.test(url) ? url : null;
}

// ─────────────────────────── inline syntax ───────────────────────────

export type InlineNode =
  | { kind: "text"; value: string }
  | { kind: "code"; value: string }
  | { kind: "link"; url: string; label: InlineNode[] }
  | { kind: "strong"; children: InlineNode[] }
  | { kind: "del"; children: InlineNode[] }
  | { kind: "em"; children: InlineNode[] };

function isWordChar(char: string | undefined): boolean {
  return !!char && /[A-Za-z0-9]/.test(char);
}

function matchLink(
  text: string,
  start: number,
): { label: string; url: string; end: number } | null {
  const labelEnd = text.indexOf("]", start + 1);
  if (labelEnd === -1 || text[labelEnd + 1] !== "(") return null;
  const targetEnd = text.indexOf(")", labelEnd + 2);
  if (targetEnd === -1) return null;
  const target = text.slice(labelEnd + 2, targetEnd).trim();
  const url = safeUrl(target.split(/\s+/)[0] ?? "");
  if (!url) return null;
  return {
    label: text.slice(start + 1, labelEnd),
    url,
    end: targetEnd + 1,
  };
}

export function parseInline(text: string): InlineNode[] {
  const nodes: InlineNode[] = [];
  let buffer = "";
  let index = 0;

  const flush = () => {
    if (buffer) {
      nodes.push({ kind: "text", value: buffer });
      buffer = "";
    }
  };

  while (index < text.length) {
    const char = text[index];

    if (char === "\\" && index + 1 < text.length) {
      buffer += text[index + 1];
      index += 2;
      continue;
    }

    if (char === "`") {
      let run = 1;
      while (text[index + run] === "`") run += 1;
      const fence = "`".repeat(run);
      const end = text.indexOf(fence, index + run);
      if (end !== -1) {
        flush();
        nodes.push({ kind: "code", value: text.slice(index + run, end) });
        index = end + run;
        continue;
      }
      buffer += char;
      index += 1;
      continue;
    }

    if (char === "[") {
      const link = matchLink(text, index);
      if (link) {
        flush();
        nodes.push({
          kind: "link",
          url: link.url,
          label: parseInline(link.label),
        });
        index = link.end;
        continue;
      }
    }

    const strongMarker =
      text.startsWith("**", index) || text.startsWith("__", index)
        ? text.slice(index, index + 2)
        : "";
    if (strongMarker) {
      const boundaryOk = strongMarker === "**" || !isWordChar(text[index - 1]);
      const end = boundaryOk ? text.indexOf(strongMarker, index + 2) : -1;
      if (end > index + 2) {
        flush();
        nodes.push({
          kind: "strong",
          children: parseInline(text.slice(index + 2, end)),
        });
        index = end + 2;
        continue;
      }
    }

    if (text.startsWith("~~", index)) {
      const end = text.indexOf("~~", index + 2);
      if (end > index + 2) {
        flush();
        nodes.push({
          kind: "del",
          children: parseInline(text.slice(index + 2, end)),
        });
        index = end + 2;
        continue;
      }
    }

    if (char === "*" || char === "_") {
      const next = text[index + 1];
      const boundaryOk =
        char === "*" || (!isWordChar(text[index - 1]) && !!next && !/\s/.test(next));
      const end = boundaryOk ? text.indexOf(char, index + 1) : -1;
      if (end > index + 1) {
        flush();
        nodes.push({
          kind: "em",
          children: parseInline(text.slice(index + 1, end)),
        });
        index = end + 1;
        continue;
      }
    }

    buffer += char;
    index += 1;
  }

  flush();
  return nodes;
}

// ─────────────────────────── block parsing ───────────────────────────

export type BlockNode =
  | { kind: "paragraph"; lines: string[] }
  | { kind: "heading"; level: number; text: string }
  | { kind: "code"; lang: string; code: string }
  | { kind: "html"; html: string }
  | { kind: "quote"; lines: string[] }
  | { kind: "list"; ordered: boolean; start: number; items: string[][] }
  | { kind: "rule" }
  | { kind: "table"; header: string[]; rows: string[][] };

type ListMarker = {
  indent: number;
  ordered: boolean;
  start: number;
  text: string;
};

const FENCE = /^\s{0,3}(`{3,}|~{3,})\s*(.*)$/;
const CLOSING_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*$/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*)$/;
const RULE = /^\s{0,3}((\*\s*){3,}|(-\s*){3,}|(_\s*){3,})$/;
const LIST_MARKER = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;
// A top-level line that opens a block-level HTML element. Lines that do not
// use one of these tags stay ordinary text.
const HTML_BLOCK_START =
  /^\s{0,3}<(?:html|body|section|div|article|header|footer|main|aside|nav|table|ul|ol|dl|details|figure|blockquote|pre|h[1-6]|p|hr)(?=[\s>/])/i;

function leadingSpaces(line: string): number {
  const match = line.match(/^ */);
  return match ? match[0].length : 0;
}

function matchList(line: string): ListMarker | null {
  const match = line.match(LIST_MARKER);
  if (!match) return null;
  const ordered = /\d/.test(match[2]);
  const start = ordered ? Number.parseInt(match[2], 10) : 1;
  return {
    indent: match[1].length,
    ordered,
    start: Number.isFinite(start) ? start : 1,
    text: match[3],
  };
}

function splitRow(line: string): string[] {
  let value = line.trim();
  if (value.startsWith("|")) value = value.slice(1);
  if (value.endsWith("|")) value = value.slice(0, -1);
  return value.split("|").map((cell) => cell.trim());
}

function isTableSeparator(line: string): boolean {
  if (!line.includes("-")) return false;
  const cells = splitRow(line);
  return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
}

function isBlockStart(lines: string[], index: number): boolean {
  const line = lines[index];
  if (
    FENCE.test(line) ||
    HEADING.test(line) ||
    RULE.test(line) ||
    /^\s{0,3}>/.test(line) ||
    HTML_BLOCK_START.test(line) ||
    matchList(line)
  ) {
    return true;
  }
  if (line.includes("|") && index + 1 < lines.length) {
    return isTableSeparator(lines[index + 1]);
  }
  return false;
}

function parseList(lines: string[], start: number): { node: BlockNode; next: number } {
  const first = matchList(lines[start]);
  const base = first ? first.indent : 0;
  const ordered = first?.ordered ?? false;
  const startNumber = first?.start ?? 1;
  const items: string[][] = [];
  let index = start;

  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      let nextNonBlank = index + 1;
      while (nextNonBlank < lines.length && !lines[nextNonBlank].trim()) {
        nextNonBlank += 1;
      }
      if (nextNonBlank >= lines.length) break;
      const nextIndent = leadingSpaces(lines[nextNonBlank]);
      const nextMarker = matchList(lines[nextNonBlank]);
      const continues = nextIndent > base || (nextMarker !== null && nextMarker.indent === base);
      if (!continues || items.length === 0) break;
      items[items.length - 1].push("");
      index = nextNonBlank;
      continue;
    }

    const marker = matchList(line);
    const indent = leadingSpaces(line);

    if (marker && marker.indent === base) {
      items.push([marker.text]);
      index += 1;
      continue;
    }

    if (items.length > 0 && indent > base) {
      const strip = Math.min(line.length, base + 2);
      items[items.length - 1].push(line.slice(strip));
      index += 1;
      continue;
    }

    break;
  }

  return {
    node: { kind: "list", ordered, start: startNumber, items },
    next: index,
  };
}

export function parseBlocks(lines: string[]): BlockNode[] {
  const blocks: BlockNode[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];

    if (!line.trim()) {
      index += 1;
      continue;
    }

    const fence = line.match(FENCE);
    if (fence) {
      const marker = fence[1][0];
      const length = fence[1].length;
      const lang = fence[2].trim().split(/\s+/)[0] ?? "";
      const code: string[] = [];
      index += 1;
      while (index < lines.length) {
        const closing = lines[index].match(CLOSING_FENCE);
        if (closing && closing[1][0] === marker && closing[1].length >= length) {
          index += 1;
          break;
        }
        code.push(lines[index]);
        index += 1;
      }
      if (lang.toLowerCase() === "html") {
        blocks.push({ kind: "html", html: code.join("\n") });
      } else {
        blocks.push({ kind: "code", lang, code: code.join("\n") });
      }
      continue;
    }

    const heading = line.match(HEADING);
    if (heading) {
      blocks.push({
        kind: "heading",
        level: heading[1].length,
        text: heading[2].replace(/\s+#+\s*$/, "").trim(),
      });
      index += 1;
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      index += 1;
      continue;
    }

    if (/^\s{0,3}>/.test(line)) {
      const quoted: string[] = [];
      while (index < lines.length) {
        const quoteLine = lines[index];
        if (!/^\s{0,3}>/.test(quoteLine)) {
          if (!quoteLine.trim()) {
            index += 1;
            continue;
          }
          break;
        }
        quoted.push(quoteLine.replace(/^\s{0,3}>\s?/, ""));
        index += 1;
      }
      blocks.push({ kind: "quote", lines: quoted });
      continue;
    }

    if (line.includes("|") && index + 1 < lines.length && isTableSeparator(lines[index + 1])) {
      const header = splitRow(line);
      const rows: string[][] = [];
      index += 2;
      while (index < lines.length && lines[index].includes("|")) {
        if (!lines[index].trim()) break;
        const row = splitRow(lines[index]);
        rows.push(header.map((_, column) => row[column] ?? ""));
        index += 1;
      }
      blocks.push({ kind: "table", header, rows });
      continue;
    }

    if (HTML_BLOCK_START.test(line)) {
      // CommonMark-style HTML block: the block ends at the first blank line.
      const html: string[] = [];
      while (index < lines.length && lines[index].trim()) {
        html.push(lines[index]);
        index += 1;
      }
      blocks.push({ kind: "html", html: html.join("\n") });
      continue;
    }

    if (matchList(line)) {
      const parsed = parseList(lines, index);
      blocks.push(parsed.node);
      index = parsed.next;
      continue;
    }

    const paragraph: string[] = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index].trim() && !isBlockStart(lines, index)) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    blocks.push({ kind: "paragraph", lines: paragraph });
  }

  return blocks;
}
