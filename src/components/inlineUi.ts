// Splits streamed assistant text into plain prose and `html` fenced blocks so
// the live transcript can render an interface block as soon as it arrives.
// The durable message goes through the full Markdown renderer instead.

export type InlineUiSegment =
  | { kind: "text"; text: string }
  | { kind: "html"; html: string };

const OPEN_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*html\s*$/i;

function isClosingFence(line: string, marker: string, length: number): boolean {
  const fence = line.trim();
  if (fence.length < length) return false;
  for (const char of fence) {
    if (char !== marker) return false;
  }
  return true;
}

export function splitInlineUi(text: string): InlineUiSegment[] {
  if (!text.includes("```") && !text.includes("~~~")) {
    return [{ kind: "text", text }];
  }

  const lines = text.split("\n").map((line) => line.replace(/\r$/, ""));
  const segments: InlineUiSegment[] = [];
  let prose: string[] = [];

  function flushProse() {
    if (prose.length === 0) return;
    segments.push({ kind: "text", text: prose.join("\n") });
    prose = [];
  }

  let index = 0;
  while (index < lines.length) {
    const open = lines[index].match(OPEN_FENCE);
    if (!open) {
      prose.push(lines[index]);
      index += 1;
      continue;
    }

    flushProse();
    const marker = open[1][0];
    const length = open[1].length;
    index += 1;
    const html: string[] = [];
    while (index < lines.length && !isClosingFence(lines[index], marker, length)) {
      html.push(lines[index]);
      index += 1;
    }
    if (index < lines.length) index += 1; // consume the closing fence
    segments.push({ kind: "html", html: html.join("\n") });
  }

  flushProse();
  return segments;
}
