// Rich visual blocks. The model wraps arbitrary HTML/CSS in
// `<template data-ui-rich>` inside an `html` fence. The template is inert in
// the host document; this component sanitizes its contents and renders them
// in a sandboxed, script-disabled iframe. The host document never receives
// model markup, and model-written JavaScript never runs.
//
// The iframe keeps `allow-same-origin` (so the host can measure its height and
// intercept links) but not `allow-scripts`, `allow-forms` or
// `allow-popups`. A second, stricter CSP inside the frame removes the network
// except for images. Host design tokens are injected as CSS variables so a
// rich block still matches the app's light theme.

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { t } from "../i18n";
import { createCoalesced } from "./inlineUi";

const MAX_RICH_CHARS = 100_000;
const MAX_HEIGHT = 2400;
const MIN_HEIGHT = 72;

/** Design tokens copied into the frame. Keep this in sync with tokens.css. */
const TOKENS = [
  "--bg-app",
  "--bg-surface",
  "--bg-subtle",
  "--bg-inset",
  "--border-softer",
  "--border-soft",
  "--border-strong",
  "--text-primary",
  "--text-secondary",
  "--text-muted",
  "--text-faint",
  "--accent",
  "--accent-hover",
  "--accent-active",
  "--accent-soft",
  "--accent-softer",
  "--accent-border",
  "--accent-ink",
  "--accent-bright",
  "--on-accent",
  "--ok",
  "--ok-soft",
  "--ok-border",
  "--ok-ink",
  "--warn",
  "--warn-soft",
  "--warn-border",
  "--warn-ink",
  "--danger",
  "--danger-soft",
  "--danger-border",
  "--danger-ink",
  "--chart-1",
  "--chart-2",
  "--chart-3",
  "--chart-4",
  "--chart-5",
  "--chart-6",
  "--chart-7",
  "--chart-8",
  "--chart-other",
  "--font-ui",
  "--font-mono",
  "--sp-1",
  "--sp-2",
  "--sp-3",
  "--sp-4",
  "--sp-5",
  "--sp-6",
  "--sp-7",
  "--r-sm",
  "--r-md",
  "--r-lg",
  "--r-xl",
];

const BASE_CSS = `
  *, *::before, *::after { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: transparent; }
  body {
    font-family: var(--font-ui);
    font-size: 13.5px;
    line-height: 1.65;
    color: var(--text-primary);
    overflow-wrap: anywhere;
  }
  h1, h2, h3, h4, h5, h6 { margin: 0.8em 0 0.4em; font-weight: 600; line-height: 1.3; }
  h1 { font-size: 22px; }
  h2 { font-size: 18px; }
  h3 { font-size: 15px; }
  p { margin: 0 0 0.65em; }
  ul, ol { margin: 0 0 0.65em; padding-left: 20px; }
  a { color: var(--accent-ink); text-decoration: underline; text-underline-offset: 2px; }
  img { max-width: 100%; height: auto; }
  pre {
    margin: 0 0 0.65em;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border-softer);
    border-radius: var(--r-sm);
    background: var(--bg-subtle);
    font-family: var(--font-mono);
    font-size: 12px;
  }
  code { font-family: var(--font-mono); font-size: 0.92em; }
  table { border-collapse: collapse; width: 100%; }
  th, td { padding: 7px 9px; border-bottom: 1px solid var(--border-softer); text-align: left; }
  th { color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; }
`;

function tokenBlock(): string {
  if (typeof document === "undefined") return ":root{}";
  const styles = getComputedStyle(document.documentElement);
  const values = TOKENS.map((name) => `${name}: ${styles.getPropertyValue(name).trim()};`).join(
    " ",
  );
  return `:root { ${values} }`;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => {
    switch (char) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case '"':
        return "&quot;";
      default:
        return "&#39;";
    }
  });
}

/** Parse inertly, drop active content, and return body markup. */
export function sanitizeRichHtml(html: string): string {
  if (html.length > MAX_RICH_CHARS) return "";
  const document = new DOMParser().parseFromString(html, "text/html");
  for (const element of Array.from(
    document.querySelectorAll("script, iframe, object, embed, link, meta, base"),
  )) {
    element.remove();
  }
  for (const element of Array.from(document.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on")) {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (
        (name === "href" || name === "src" || name === "action" || name === "formaction") &&
        /^\s*(javascript|vbscript|data:text\/html)/i.test(attribute.value)
      ) {
        element.removeAttribute(attribute.name);
      }
    }
  }
  return document.body.innerHTML;
}

function sandboxDocument(html: string, title: string): string {
  const policy = [
    "default-src 'none'",
    "style-src 'unsafe-inline'",
    "img-src https: data:",
    "font-src 'none'",
    "media-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
    "frame-src 'none'",
  ].join("; ");
  return [
    '<!doctype html><html><head><meta charset="utf-8">',
    `<meta http-equiv="Content-Security-Policy" content="${policy}">`,
    `<title>${escapeHtml(title)}</title>`,
    `<style>${tokenBlock()}</style>`,
    `<style>${BASE_CSS}</style>`,
    "</head><body>",
    html,
    "</body></html>",
  ].join("");
}

export function RichSandbox(props: {
  html: string;
  title?: string;
  onOpenUrl?: (url: string) => void;
}) {
  const source = createCoalesced(() => props.html, 250);
  const safe = createMemo(() => sanitizeRichHtml(source()));
  const documentSource = createMemo(() => sandboxDocument(safe(), t("Generated interface")));
  const [height, setHeight] = createSignal(160);
  let frame: HTMLIFrameElement | undefined;
  let frameWindow: Window | null = null;
  let frameDocument: Document | null = null;
  let timer: number | undefined;
  let lateTimer: number | undefined;

  function measure() {
    const body = frameDocument?.body;
    if (!body) return;
    const next = Math.ceil(body.getBoundingClientRect().height);
    if (next > 0) setHeight(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, next)));
  }

  function onClick(event: MouseEvent) {
    const target = event.target as Element | null;
    const anchor = target?.closest?.("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    event.preventDefault();
    if (/^(https?:\/\/|mailto:)/i.test(href)) props.onOpenUrl?.(href);
  }

  function detach() {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timer = undefined;
    }
    if (lateTimer !== undefined) {
      window.clearTimeout(lateTimer);
      lateTimer = undefined;
    }
    frameWindow?.removeEventListener("load", measure, true);
    frameDocument?.removeEventListener("click", onClick, true);
    frameWindow = null;
    frameDocument = null;
  }

  function attach() {
    detach();
    frameWindow = frame?.contentWindow ?? null;
    frameDocument = frame?.contentDocument ?? null;
    if (!frameWindow || !frameDocument) return;
    frameWindow.addEventListener("load", measure, true);
    frameDocument.addEventListener("click", onClick, true);
    measure();
    timer = window.setTimeout(measure, 300);
    // Remote images may finish after the document load; measure once more.
    lateTimer = window.setTimeout(measure, 1200);
  }

  onMount(() => {
    const onResize = () => measure();
    window.addEventListener("resize", onResize);
    onCleanup(() => window.removeEventListener("resize", onResize));
  });

  onCleanup(detach);

  return (
    <div class="inline-ui-rich-wrap">
      <div class="inline-ui-rich-bar">
        <span class="inline-ui-rich-dot" aria-hidden="true" />
        {t("Generated interface")}
      </div>
      <iframe
        ref={(element) => {
          frame = element;
        }}
        class="inline-ui-rich"
        sandbox="allow-same-origin"
        referrerpolicy="no-referrer"
        title={props.title ?? t("Generated interface")}
        srcdoc={documentSource()}
        style={{ height: `${height()}px` }}
        onLoad={attach}
      />
    </div>
  );
}
