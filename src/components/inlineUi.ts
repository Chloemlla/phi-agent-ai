// Splits streamed assistant text into plain prose and `html` segments so the
// live transcript can render an interface block as soon as it arrives.
// Fenced `html` blocks and top-level raw HTML blocks are both recognized, so
// the live stream follows the same rules as the durable Markdown renderer.
//
// The splitter is incremental: a live stream only appends text, so only the
// newly arrived lines are classified. The pure `splitInlineUi` entry point
// keeps the whole-string semantics for callers and tests.

import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js";
import { startsHtmlBlock } from "./markdown/parse";

export type InlineUiSegment = { kind: "text"; text: string } | { kind: "html"; html: string };

const OPEN_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([^\s`]*)/;

function isClosingFence(line: string, marker: string, length: number): boolean {
  const fence = line.trim();
  if (fence.length < length) return false;
  for (const char of fence) {
    if (char !== marker) return false;
  }
  return true;
}

/** True when appended text may open a fence or a raw HTML block. Once a block
 * exists the splitter keeps parsing; this is only the plain-text fast path. */
function hasMarkup(value: string, from: number): boolean {
  const tail = value.slice(Math.max(0, from - 4));
  return tail.includes("```") || tail.includes("~~~") || /(?:^|\n)[ \t]{0,3}<[A-Za-z!/]/.test(tail);
}

/**
 * Stateful line scanner. `update` is O(new characters) for appended text;
 * it reparses from the start only when the stream is replaced or markup first
 * appears (the no-markup fast path is plain text).
 */
export function createInlineUiSplitter() {
  let source = "";
  let consumed = 0;
  let parsing = false;
  let line = "";
  let mode: "prose" | "fence" | "raw" | "code" = "prose";
  let marker = "";
  let markerLength = 0;
  let prose: string[] = [];
  let html: string[] = [];
  let segments: InlineUiSegment[] = [];

  function reset() {
    consumed = 0;
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

  function flushHtml() {
    segments.push({ kind: "html", html: html.join("\n") });
    html = [];
  }

  function pushLine(raw: string) {
    const value = raw.replace(/\r$/, "");
    if (mode === "fence") {
      if (isClosingFence(value, marker, markerLength)) {
        flushHtml();
        mode = "prose";
      } else {
        html.push(value);
      }
      return;
    }
    if (mode === "raw") {
      // CommonMark HTML blocks end at the first blank line.
      if (!value.trim()) {
        flushHtml();
        mode = "prose";
      } else {
        html.push(value);
      }
      return;
    }
    if (mode === "code") {
      prose.push(value);
      if (isClosingFence(value, marker, markerLength)) mode = "prose";
      return;
    }
    const open = value.match(OPEN_FENCE);
    if (open) {
      const fenceMarker = open[1];
      if ((open[2] ?? "").toLowerCase() === "html") {
        flushProse();
        mode = "fence";
      } else {
        // A non-HTML fence stays prose; its contents must never be mistaken
        // for an interface block.
        prose.push(value);
        mode = "code";
      }
      marker = fenceMarker[0];
      markerLength = fenceMarker.length;
      return;
    }
    if (startsHtmlBlock(value)) {
      flushProse();
      mode = "raw";
      html.push(value);
      return;
    }
    prose.push(value);
  }

  /** Render the current state plus the still-growing last line. */
  function snapshot(): InlineUiSegment[] {
    const out = segments.slice();
    const partial = line.replace(/\r$/, "");
    if (mode === "fence") {
      if (isClosingFence(partial, marker, markerLength)) {
        out.push({ kind: "html", html: html.join("\n") });
      } else {
        out.push({ kind: "html", html: [...html, partial].join("\n") });
      }
      return out;
    }
    if (mode === "raw") {
      if (!partial.trim()) {
        out.push({ kind: "html", html: html.join("\n") });
      } else {
        out.push({ kind: "html", html: [...html, partial].join("\n") });
      }
      return out;
    }
    if (mode === "prose" && startsHtmlBlock(partial)) {
      if (prose.length > 0) {
        out.push({ kind: "text", text: prose.join("\n") });
      }
      out.push({ kind: "html", html: partial });
      return out;
    }
    const open = mode === "prose" ? partial.match(OPEN_FENCE) : null;
    if (open && (open[2] ?? "").toLowerCase() === "html") {
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
        parsing = false;
      }

      if (!parsing) {
        if (!hasMarkup(next, consumed)) {
          source = next;
          consumed = next.length;
          return [{ kind: "text", text: next }];
        }
        // Markup appeared: rebuild from the beginning so earlier plain text
        // and the new block are split at the right boundary.
        reset();
        parsing = true;
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
