// Splits streamed assistant text into plain prose and `html` fenced blocks so
// the live transcript can render an interface block as soon as it arrives.
// The durable message goes through the full Markdown renderer instead.
//
// The splitter is incremental: a live stream only appends text, so only the
// newly arrived lines are classified. The pure `splitInlineUi` entry point
// keeps the old whole-string semantics for callers and tests.

import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js";

export type InlineUiSegment = { kind: "text"; text: string } | { kind: "html"; html: string };

const OPEN_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*html\s*$/i;

function isClosingFence(line: string, marker: string, length: number): boolean {
  const fence = line.trim();
  if (fence.length < length) return false;
  for (const char of fence) {
    if (char !== marker) return false;
  }
  return true;
}

function hasFence(value: string, from: number): boolean {
  return value.indexOf("```", from) !== -1 || value.indexOf("~~~", from) !== -1;
}

/**
 * Stateful line scanner. `update` is O(new characters) for appended text;
 * it reparses from the start only when the stream is replaced or the first
 * fence marker appears (the no-fence fast path is plain text).
 */
export function createInlineUiSplitter() {
  let source = "";
  let consumed = 0;
  let fenced = false;
  let line = "";
  let mode: "prose" | "html" = "prose";
  let marker = "";
  let markerLength = 0;
  let prose: string[] = [];
  let html: string[] = [];
  let segments: InlineUiSegment[] = [];

  function reset() {
    consumed = 0;
    fenced = false;
    line = "";
    mode = "prose";
    marker = "";
    markerLength = 0;
    prose = [];
    html = [];
    segments = [];
  }

  function flushProse() {
    if (prose.length === 0) return;
    segments.push({ kind: "text", text: prose.join("\n") });
    prose = [];
  }

  function pushLine(raw: string) {
    const value = raw.replace(/\r$/, "");
    if (mode === "html") {
      if (isClosingFence(value, marker, markerLength)) {
        segments.push({ kind: "html", html: html.join("\n") });
        html = [];
        mode = "prose";
      } else {
        html.push(value);
      }
      return;
    }
    const open = value.match(OPEN_FENCE);
    if (open) {
      flushProse();
      mode = "html";
      marker = open[1][0];
      markerLength = open[1].length;
      return;
    }
    prose.push(value);
  }

  /** Render the current state plus the still-growing last line. */
  function snapshot(): InlineUiSegment[] {
    const out = segments.slice();
    const partial = line.replace(/\r$/, "");
    if (mode === "html") {
      if (isClosingFence(partial, marker, markerLength)) {
        out.push({ kind: "html", html: html.join("\n") });
      } else {
        out.push({ kind: "html", html: [...html, partial].join("\n") });
      }
      return out;
    }
    const open = partial.match(OPEN_FENCE);
    if (open) {
      if (prose.length > 0) {
        out.push({ kind: "text", text: prose.join("\n") });
      }
      out.push({ kind: "html", html: "" });
      return out;
    }
    out.push({ kind: "text", text: [...prose, partial].join("\n") });
    return out;
  }

  return {
    update(next: string): InlineUiSegment[] {
      if (!next.startsWith(source)) {
        reset();
        if (!hasFence(next, 0)) {
          source = next;
          consumed = next.length;
          return [{ kind: "text", text: next }];
        }
        fenced = true;
      } else if (!fenced) {
        // Check a two-character overlap so a marker split across chunks is
        // still noticed. Without a fence the old fast path returned the raw
        // text, which also preserves CRLF.
        if (!hasFence(next, Math.max(0, consumed - 2))) {
          source = next;
          consumed = next.length;
          return [{ kind: "text", text: next }];
        }
        reset();
        fenced = true;
      }

      let index = consumed;
      while (index < next.length) {
        const newline = next.indexOf("\n", index);
        if (newline === -1) {
          line += next.slice(index);
          break;
        }
        pushLine(line + next.slice(index, newline));
        line = "";
        index = newline + 1;
      }
      consumed = next.length;
      source = next;
      return snapshot();
    },
  };
}

export function splitInlineUi(text: string): InlineUiSegment[] {
  return createInlineUiSplitter().update(text);
}

/**
 * Coalesce a rapid signal into at most one update per `interval` ms. The
 * first change after an idle period is immediate; the last value always lands
 * within one interval. Used for streamed text and inline HTML parsing.
 */
export function createCoalesced<T>(source: () => T, interval = 64): Accessor<T> {
  const [value, setValue] = createSignal<T>(source());
  let timer: number | undefined;
  let last = -Infinity;

  createEffect(() => {
    const next = source();
    const now = performance.now();
    const wait = interval - (now - last);
    if (wait <= 0) {
      last = now;
      setValue(() => next);
      return;
    }
    if (timer !== undefined) window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      timer = undefined;
      last = performance.now();
      setValue(() => next);
    }, wait);
  });

  onCleanup(() => {
    if (timer !== undefined) window.clearTimeout(timer);
  });

  return value;
}
