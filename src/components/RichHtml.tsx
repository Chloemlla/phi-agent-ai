// Inline interface blocks. The model writes HTML inside an `html` fence; this
// module parses it with the browser's inert HTML parser and rebuilds only
// allowlisted elements, classes and attributes as Solid JSX. No raw HTML
// string is ever injected, no model-authored script runs, and every visual
// decision stays with the host stylesheet.
//
// A small, HTML-native layer makes the block work like an answer rather than a
// picture:
//   - <script type="application/json" data-ui-state>{...}</script> declares state
//   - {{ expression }} interpolates state into text or a few attributes
//   - data-ui-bind wires an input/select/textarea to a state name
//   - data-ui-set / data-ui-send / data-ui-copy act on a click
//   - data-ui-show hides an element while its expression is falsy
//   - <template data-ui-each="items" data-ui-as="item"> repeats markup
// Expressions are parsed by `uiExpression.ts`; no JavaScript is evaluated.

import {
  For,
  Show,
  createEffect,
  createMemo,
  untrack,
  type JSX,
} from "solid-js";
import { createStore } from "solid-js/store";
import { Dynamic } from "solid-js/web";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import { pushToast } from "../toast";
import { InlineChart, readChartTable } from "./InlineChart";
import {
  applyAssignments,
  asList,
  evaluate,
  hashString,
  interpolate,
  type UiValues,
} from "./uiExpression";

/** A block longer than this is shown as a notice instead of being rendered. */
export const INLINE_UI_MAX_CHARS = 100_000;
const MAX_DEPTH = 32;
const MAX_NODES = 2_500;
const MAX_SEND_CHARS = 4_000;
const MAX_EACH_ROWS = 200;
const MAX_CACHED_STATES = 100;

const SVG_NS = "http://www.w3.org/2000/svg";
const SAFE_LINK = /^(https?:\/\/|mailto:)/i;
const SAFE_IMAGE = /^https:\/\/[^\s"'<>]+$/i;
/** Shown instead of a rejected dynamic URL so the browser makes no request. */
const BLANK_IMAGE =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";
const BIND_NAME = /^[A-Za-z_][A-Za-z0-9_]{0,39}$/;
const BLOCKED_KEYS = new Set(["__proto__", "prototype", "constructor"]);

/** Elements rendered as-is. Everything else is unwrapped or dropped. */
const HTML_TAGS = new Set([
  "a", "abbr", "address", "article", "aside", "b", "blockquote", "br",
  "button", "caption", "cite", "code", "dd", "del", "details", "div", "dl",
  "dt", "em", "fieldset", "figcaption", "figure", "footer", "form", "h1",
  "h2", "h3", "h4", "h5", "h6", "header", "hr", "i", "img", "input", "ins",
  "kbd", "label", "legend", "li", "main", "mark", "meter", "nav", "ol",
  "optgroup", "option", "output", "p", "pre", "progress", "q", "s", "samp",
  "section", "select", "small", "span", "strong", "sub", "summary", "sup",
  "table", "tbody", "td", "textarea", "tfoot", "th", "thead", "time", "tr",
  "u", "ul", "var",
]);

const VOID_TAGS = new Set(["br", "hr", "img", "input"]);

/** Elements whose whole subtree is removed. */
const DROPPED_TAGS = new Set([
  "applet", "audio", "base", "canvas", "dialog", "embed", "frame", "frameset",
  "iframe", "link", "math", "meta", "noscript", "object", "picture", "portal",
  "slot", "source", "style", "track", "video",
]);

/** Host-styled component vocabulary. Any other class token is discarded. */
const ALLOWED_CLASSES = new Set([
  "ui-grid", "ui-split", "ui-card", "ui-callout", "ui-info", "ui-ok",
  "ui-warn", "ui-danger", "ui-badge", "ui-metrics", "ui-metric", "ui-steps",
  "ui-kv", "ui-chart", "ui-chips", "ui-muted", "ui-num", "ui-clickable",
]);

const SVG_TAGS = new Set([
  "svg", "g", "path", "rect", "circle", "ellipse", "line", "polyline",
  "polygon", "text", "tspan", "desc",
]);

const DROPPED_SVG_TAGS = new Set([
  "a", "animate", "animatemotion", "animatetransform", "clippath", "defs",
  "filter", "foreignobject", "image", "lineargradient", "marker", "mask",
  "pattern", "radialgradient", "script", "set", "stop", "style", "switch",
  "symbol", "textpath", "title", "use",
]);

const SVG_ATTRS = new Set([
  "viewBox", "width", "height", "x", "y", "x1", "x2", "y1", "y2", "cx",
  "cy", "r", "rx", "ry", "d", "points", "fill", "fill-opacity", "stroke",
  "stroke-width", "stroke-linecap", "stroke-linejoin", "stroke-dasharray",
  "stroke-opacity", "opacity", "transform", "text-anchor",
  "dominant-baseline", "font-size", "font-weight", "aria-label", "role",
]);

const HTML_TAG_ATTRS: Record<string, Set<string>> = {
  a: new Set(["href"]),
  ol: new Set(["start", "reversed", "type"]),
  details: new Set(["open"]),
  th: new Set(["colspan", "rowspan", "scope"]),
  td: new Set(["colspan", "rowspan"]),
  time: new Set(["datetime"]),
  progress: new Set(["value", "max"]),
  meter: new Set(["value", "min", "max", "low", "high", "optimum"]),
  table: new Set(["data-type", "data-height"]),
  li: new Set(["data-state"]),
  button: new Set(["type", "disabled"]),
  input: new Set([
    "type", "name", "value", "placeholder", "min", "max", "step", "checked",
    "disabled", "required", "readonly", "multiple",
  ]),
  textarea: new Set(["name", "rows", "cols", "placeholder", "disabled", "required", "readonly"]),
  select: new Set(["name", "multiple", "disabled", "required"]),
  option: new Set(["value", "selected", "disabled", "label"]),
  optgroup: new Set(["label", "disabled"]),
  fieldset: new Set(["disabled"]),
  img: new Set(["src", "alt", "width", "height"]),
};

const INPUT_TYPES = new Set([
  "text", "search", "number", "range", "date", "time", "email", "url",
  "tel", "checkbox", "radio",
]);
const DIRS = new Set(["ltr", "rtl", "auto"]);
const OL_TYPES = new Set(["1", "a", "A", "i", "I"]);
const SCOPES = new Set(["row", "col", "rowgroup", "colgroup"]);
const LI_STATES = new Set(["done", "current", "todo"]);
const BUTTON_TYPES = new Set(["submit", "button", "reset"]);

type RenderContext = {
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
  state: UiValues;
  locals: UiValues;
  assign: (name: string, value: unknown) => void;
  depth: number;
  budget: { nodes: number };
};

// ─────────────────────────── state ───────────────────────────

/** States live for the session, keyed by the block source. The source changes
 * only when the model writes a different answer, so a reply keeps its choices
 * while the transcript refreshes. */
const stateCache = new Map<string, UiValues>();

function cacheState(key: string, values: UiValues) {
  if (!stateCache.has(key) && stateCache.size >= MAX_CACHED_STATES) {
    const oldest = stateCache.keys().next().value;
    if (oldest) stateCache.delete(oldest);
  }
  stateCache.set(key, values);
}

function plain(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [name, child] of Object.entries(value as Record<string, unknown>)) {
      if (BLOCKED_KEYS.has(name)) continue;
      out[name] = plain(child);
    }
    return out;
  }
  if (typeof value === "function") return undefined;
  return value;
}

function compatible(defaultValue: unknown, savedValue: unknown): boolean {
  if (defaultValue === null) return savedValue === null;
  if (Array.isArray(defaultValue)) return Array.isArray(savedValue);
  return typeof defaultValue === typeof savedValue;
}

function mergeState(defaults: UiValues, saved: UiValues): UiValues {
  const merged: UiValues = {};
  for (const [name, value] of Object.entries(defaults)) {
    if (BLOCKED_KEYS.has(name)) continue;
    merged[name] =
      Object.prototype.hasOwnProperty.call(saved, name) && compatible(value, saved[name])
        ? saved[name]
        : value;
  }
  for (const [name, value] of Object.entries(saved)) {
    if (BLOCKED_KEYS.has(name) || name in merged) continue;
    merged[name] = value;
  }
  return merged;
}

function parseState(source: string): UiValues {
  try {
    const value = JSON.parse(source) as unknown;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const state: UiValues = {};
    for (const [name, child] of Object.entries(value as Record<string, unknown>)) {
      if (BLOCKED_KEYS.has(name)) continue;
      state[name] = child;
    }
    return state;
  } catch {
    return {};
  }
}

function parseBlock(html: string): { body: Element; state: UiValues } {
  const document = new DOMParser().parseFromString(html, "text/html");
  const script = document.querySelector(
    'script[type="application/json"][data-ui-state]',
  );
  // A template with the same attribute is an equally inert alternative for
  // models that prefer not to write a script tag.
  const template = document.querySelector("template[data-ui-state]");
  const text = script?.textContent ?? template?.getAttribute("data-ui-state") ?? "";
  return {
    body: document.body,
    state: text.trim() ? parseState(text.slice(0, 20_000)) : {},
  };
}

// ─────────────────────────── component ───────────────────────────

export function RichHtml(props: {
  html: string;
  onOpenUrl?: (url: string) => void;
  onSend?: (text: string) => void;
}) {
  const firstSource = props.html;
  const firstBlock = parseBlock(firstSource);
  const firstKey = `ui:${hashString(firstSource)}`;
  const [state, setState] = createStore<UiValues>(
    mergeState(firstBlock.state, stateCache.get(firstKey) ?? {}),
  );
  const parsed = createMemo(() => parseBlock(props.html), firstBlock);

  // New content may add state names; already-known names keep their value.
  createEffect(() => {
    const block = parsed();
    const key = `ui:${hashString(props.html)}`;
    const saved = stateCache.get(key) ?? {};
    for (const [name, value] of Object.entries(block.state)) {
      if (BLOCKED_KEYS.has(name)) continue;
      if (untrack(() => state[name]) !== undefined) continue;
      setState(
        name,
        Object.prototype.hasOwnProperty.call(saved, name) && compatible(value, saved[name])
          ? saved[name]
          : value,
      );
    }
  });

  const assign = (name: string, value: unknown) => {
    if (!BIND_NAME.test(name) || BLOCKED_KEYS.has(name)) return;
    setState(name, plain(value));
    cacheState(`ui:${hashString(props.html)}`, plain(untrack(() => state)) as UiValues);
  };

  const content = createMemo(() => {
    const block = parsed();
    const context: RenderContext = {
      onOpenUrl: props.onOpenUrl,
      onSend: props.onSend,
      state,
      locals: {},
      assign,
      depth: 0,
      budget: { nodes: 0 },
    };
    return <>{renderChildren(Array.from(block.body.childNodes), context)}</>;
  });

  return (
    <Show when={content()}>
      {(rendered) => <div class="inline-ui">{rendered()}</div>}
    </Show>
  );
}

function BoundText(props: { template: string; context: RenderContext }) {
  const value = createMemo(() =>
    interpolate(props.template, props.context.state, props.context.locals),
  );
  return <>{value()}</>;
}

function EachView(props: { element: HTMLTemplateElement; context: RenderContext }) {
  const expression = props.element.getAttribute("data-ui-each") ?? "";
  const requested = (props.element.getAttribute("data-ui-as") ?? "item").replace(/^\$/, "");
  const name = BIND_NAME.test(requested) ? requested : "item";
  const rows = createMemo(() =>
    asList(
      evaluate(expression, props.context.state, props.context.locals),
    ).slice(0, MAX_EACH_ROWS),
  );
  return (
    <For each={rows()}>
      {(row, index) =>
        renderChildren(Array.from(props.element.content.childNodes), {
          ...props.context,
          locals: { ...props.context.locals, [name]: row, index: index() },
        })
      }
    </For>
  );
}

// ─────────────────────────── rendering ───────────────────────────

function renderChildren(nodes: ChildNode[], context: RenderContext): JSX.Element[] {
  const elements: JSX.Element[] = [];
  for (const node of nodes) {
    const rendered = renderNode(node, context);
    if (rendered !== null && rendered !== undefined) elements.push(rendered);
  }
  return elements;
}

function renderNode(node: ChildNode, context: RenderContext): JSX.Element | null {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.nodeValue ?? "";
    return text.includes("{{") ? <BoundText template={text} context={context} /> : text;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    // Comments and other non-rendered nodes carry nothing the user should see.
    return null;
  }

  // A hostile or runaway tree must not exhaust the renderer.
  if (context.depth > MAX_DEPTH) return null;
  context.budget.nodes += 1;
  if (context.budget.nodes > MAX_NODES) return null;

  const element = node as Element;
  if (element.namespaceURI === SVG_NS) {
    return renderSvgNode(element, context);
  }

  const tag = element.tagName.toLowerCase();
  const nested = { ...context, depth: context.depth + 1 };

  if (tag === "script") return null;
  if (tag === "template") {
    return element.hasAttribute("data-ui-each") ? (
      <EachView element={element as HTMLTemplateElement} context={context} />
    ) : null;
  }
  if (DROPPED_TAGS.has(tag)) return null;

  const classes = allowedClasses(element);
  if (tag === "table" && classes.includes("ui-chart")) {
    const chart = readChartTable(element);
    if (chart) {
      const props: Record<string, unknown> = {};
      applyVisibility(element, props, context);
      return (
        <div {...props}>
          <InlineChart data={chart} />
        </div>
      );
    }
  }
  if (tag === "button") {
    return renderButton(element, classes, nested, context);
  }
  if (tag === "form") {
    return renderForm(element, classes, nested, context);
  }
  if (!HTML_TAGS.has(tag)) {
    // Unknown wrappers (for example a custom element) keep their content.
    return <>{renderChildren(Array.from(element.childNodes), nested)}</>;
  }
  if (tag === "br") return <br />;
  if (tag === "hr") return <hr />;

  if (tag === "img" && !validImageSource(element.getAttribute("src") ?? "")) {
    return null;
  }

  const props = htmlProps(tag, element, classes, context);
  const children = VOID_TAGS.has(tag)
    ? undefined
    : renderChildren(Array.from(element.childNodes), nested);
  return (
    <Dynamic component={tag} {...props}>
      {children}
    </Dynamic>
  );
}

function allowedClasses(element: Element): string[] {
  const classes: string[] = [];
  for (const token of element.classList) {
    if (ALLOWED_CLASSES.has(token)) classes.push(token);
  }
  return classes;
}

function htmlProps(
  tag: string,
  element: Element,
  classes: string[],
  context: RenderContext,
): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  if (classes.length > 0) props.class = classes.join(" ");
  applyGlobalAttributes(element, props, context);

  if (tag === "img") {
    applyImage(element, props, context);
    return props;
  }
  if (tag === "option") {
    applyOption(element, props, context);
    return props;
  }

  const allowed = HTML_TAG_ATTRS[tag];
  if (allowed) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (!allowed.has(name)) continue;
      applyTagAttribute(tag, name, attribute.value, props, context);
    }
  }
  applyBinding(tag, element, props, context);
  applyVisibility(element, props, context);

  const setExpression = element.getAttribute("data-ui-set");
  if (setExpression && tag !== "button") {
    const run = (event: Event) => {
      event.preventDefault();
      applyAssignments(
        setExpression,
        context.state,
        context.locals,
        context.assign,
      );
    };
    props.onClick = run;
    props.onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter" || event.key === " ") run(event);
    };
    props.role = "button";
    props.tabIndex = 0;
    props.class = [props.class as string | undefined, "ui-clickable"]
      .filter(Boolean)
      .join(" ");
  }

  return props;
}

function applyGlobalAttributes(
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const title = element.getAttribute("title");
  if (title) textProp(props, "title", title, context, 200);
  const lang = element.getAttribute("lang");
  if (lang) props.lang = cap(lang, 20);
  const dir = element.getAttribute("dir");
  if (dir && DIRS.has(dir)) props.dir = dir;
}

function applyTagAttribute(
  tag: string,
  name: string,
  value: string,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  switch (name) {
    case "href": {
      if (tag !== "a") return;
      const read = () => interpolate(value, context.state, context.locals).trim();
      if (value.includes("{{")) {
        defineGetter(props, "href", () => {
          const url = read();
          return SAFE_LINK.test(url) ? url : undefined;
        });
      } else {
        const url = value.trim();
        if (!SAFE_LINK.test(url)) return;
        props.href = url;
      }
      props.onClick = (event: MouseEvent) => {
        event.preventDefault();
        const url = read();
        if (SAFE_LINK.test(url)) openExternal(url, context.onOpenUrl);
      };
      return;
    }
    case "start": {
      numberProp(props, "start", value, context, 10_000);
      return;
    }
    case "reversed":
    case "open":
    case "checked":
    case "disabled":
    case "required":
    case "readonly":
    case "multiple":
    case "selected":
      props[name] = true;
      return;
    case "type": {
      if (tag === "ol") {
        if (OL_TYPES.has(value)) props.type = value;
      } else if (tag === "input") {
        const type = value.toLowerCase();
        if (INPUT_TYPES.has(type)) props.type = type;
      } else if (tag === "button") {
        const type = value.toLowerCase();
        if (BUTTON_TYPES.has(type)) props.type = type;
      }
      return;
    }
    case "scope": {
      if (tag === "th" && SCOPES.has(value)) props.scope = value;
      return;
    }
    case "colspan":
    case "rowspan":
      numberProp(props, name, value, context, 1000);
      return;
    case "rows":
    case "cols":
      numberProp(props, name, value, context, 40);
      return;
    case "width":
    case "height":
      numberProp(props, name, value, context, 2000);
      return;
    case "value":
    case "max":
    case "min":
    case "low":
    case "high":
    case "optimum":
    case "step":
      textProp(props, name, value, context, 80);
      return;
    case "name": {
      if (BIND_NAME.test(value)) props.name = value;
      return;
    }
    case "placeholder":
    case "label":
      textProp(props, name, value, context, 200);
      return;
    case "datetime":
      textProp(props, "datetime", value, context, 80);
      return;
    case "data-state": {
      if (LI_STATES.has(value)) props["data-state"] = value;
      return;
    }
    case "data-type": {
      if (tag === "table" && (value === "bar" || value === "line" || value === "area")) {
        props["data-type"] = value;
      }
      return;
    }
    case "data-height": {
      const height = Number.parseInt(value, 10);
      if (Number.isFinite(height)) {
        props["data-height"] = Math.min(420, Math.max(140, height));
      }
      return;
    }
    default:
      return;
  }
}

function applyImage(
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const src = element.getAttribute("src") ?? "";
  if (src.includes("{{")) {
    defineGetter(props, "src", () => {
      const url = interpolate(src, context.state, context.locals).trim();
      return SAFE_IMAGE.test(url) ? url : BLANK_IMAGE;
    });
  } else if (SAFE_IMAGE.test(src.trim())) {
    props.src = src.trim();
  }
  const alt = element.getAttribute("alt");
  props.alt = cap(alt ?? "", 300);
  numberProp(props, "width", element.getAttribute("width") ?? "", context, 2000);
  numberProp(props, "height", element.getAttribute("height") ?? "", context, 2000);
  props.loading = "lazy";
  props.decoding = "async";
  props.referrerPolicy = "no-referrer";
}

function applyOption(
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const select = element.closest("select");
  const bind = bindName(select?.getAttribute("data-ui-bind") ?? null);
  const value = element.getAttribute("value") ?? cleanText(element.textContent);
  if (bind) {
    const multiple = select?.hasAttribute("multiple") === true;
    defineGetter(props, "selected", () => {
      if (multiple) {
        const current = context.state[bind];
        return Array.isArray(current) && current.includes(value);
      }
      return String(context.state[bind] ?? "") === value;
    });
  } else if (element.hasAttribute("selected")) {
    props.selected = true;
  }
  if (element.hasAttribute("disabled")) props.disabled = true;
  const label = element.getAttribute("label");
  if (label) props.label = cap(label, 200);
  if (element.hasAttribute("value")) props.value = cap(value, 200);
}

function applyBinding(
  tag: string,
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const name = bindName(element.getAttribute("data-ui-bind"));
  if (!name) return;

  if (tag === "input") {
    const type = (element.getAttribute("type") ?? "text").toLowerCase();
    if (type === "checkbox") {
      defineGetter(props, "checked", () => Boolean(context.state[name]));
      props.onChange = (event: Event) => {
        const input = event.currentTarget as HTMLInputElement;
        context.assign(name, input.checked);
      };
      return;
    }
    if (type === "radio") {
      const value = element.getAttribute("value") ?? "";
      defineGetter(props, "checked", () => String(context.state[name] ?? "") === value);
      props.onChange = (event: Event) => {
        const input = event.currentTarget as HTMLInputElement;
        if (input.checked) context.assign(name, value);
      };
      return;
    }
    if (type === "number" || type === "range") {
      defineGetter(props, "value", () => String(context.state[name] ?? ""));
      props.onInput = (event: Event) => {
        const input = event.currentTarget as HTMLInputElement;
        context.assign(name, input.value === "" ? 0 : Number(input.value));
      };
      return;
    }
    defineGetter(props, "value", () => String(context.state[name] ?? ""));
    props.onInput = (event: Event) => {
      const input = event.currentTarget as HTMLInputElement;
      context.assign(name, input.value);
    };
    return;
  }

  if (tag === "textarea") {
    defineGetter(props, "value", () => String(context.state[name] ?? ""));
    props.onInput = (event: Event) => {
      const textarea = event.currentTarget as HTMLTextAreaElement;
      context.assign(name, textarea.value);
    };
    return;
  }

  if (tag === "select") {
    props.onChange = (event: Event) => {
      const select = event.currentTarget as HTMLSelectElement;
      const values = Array.from(select.selectedOptions).map(
        (option) => option.value || cleanText(option.textContent),
      );
      context.assign(name, select.multiple ? values : values[0] ?? "");
    };
  }
}

function applyVisibility(
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const expression =
    element.getAttribute("data-ui-show") ?? element.getAttribute("data-ui-if");
  if (!expression) return;
  defineGetter(props, "style", () => ({
    display: visible(expression, context) ? undefined : "none",
  }));
}

function visible(expression: string, context: RenderContext): boolean {
  try {
    return Boolean(evaluate(expression, context.state, context.locals));
  } catch {
    return true;
  }
}

// ─────────────────────────── actions ───────────────────────────

function renderButton(
  element: Element,
  classes: string[],
  nested: RenderContext,
  context: RenderContext,
): JSX.Element {
  const children = renderChildren(Array.from(element.childNodes), nested);
  const setExpression = element.getAttribute("data-ui-set");
  const sendExpression = element.getAttribute("data-ui-send");
  const copyExpression = element.getAttribute("data-ui-copy");
  const disabledAttr = element.hasAttribute("disabled");
  const inForm = element.closest("form") !== null;
  const requested = (element.getAttribute("type") ?? "").toLowerCase();
  const type = BUTTON_TYPES.has(requested) ? requested : inForm ? "submit" : "button";
  const title = element.getAttribute("title");
  const action =
    setExpression !== null || sendExpression !== null || copyExpression !== null;
  const props: Record<string, unknown> = {};
  applyVisibility(element, props, context);
  if (title) props.title = cap(title, 200);

  if (action) {
    const requiresSend = sendExpression !== null && !context.onSend;
    const primary = sendExpression !== null;
    props.type = "button";
    props.class = buttonClass(classes, primary, false);
    props.disabled = disabledAttr || requiresSend;
    props.onClick = () => {
      if (setExpression) {
        applyAssignments(
          setExpression,
          context.state,
          context.locals,
          context.assign,
        );
      }
      if (copyExpression !== null) {
        void copyText(interpolate(copyExpression, context.state, context.locals));
      }
      if (sendExpression !== null) {
        const text =
          interpolate(sendExpression, context.state, context.locals).trim() ||
          interpolate(element.textContent ?? "", context.state, context.locals).trim();
        if (text) context.onSend?.(cap(text, MAX_SEND_CHARS));
      }
    };
    return (
      <Dynamic component="button" {...props}>
        {children}
      </Dynamic>
    );
  }

  if (inForm && type === "submit") {
    props.type = "submit";
    props.class = buttonClass(classes, true, false);
    props.disabled = disabledAttr || !context.onSend;
    return (
      <Dynamic component="button" {...props}>
        {children}
      </Dynamic>
    );
  }

  if (inForm && type === "reset") {
    props.type = "reset";
    props.class = buttonClass(classes, false, false);
    return (
      <Dynamic component="button" {...props}>
        {children}
      </Dynamic>
    );
  }

  // A bare button has no host action; keep its label without a false affordance.
  props.type = "button";
  props.class = buttonClass(classes, false, true);
  props.disabled = true;
  return (
    <Dynamic component="button" {...props}>
      {children}
    </Dynamic>
  );
}

function buttonClass(classes: string[], primary: boolean, inert: boolean): string {
  return [
    "ui-btn",
    primary ? "ui-btn-primary" : "",
    inert ? "ui-btn-inert" : "",
    ...classes,
  ]
    .filter(Boolean)
    .join(" ");
}

function renderForm(
  element: Element,
  classes: string[],
  nested: RenderContext,
  context: RenderContext,
): JSX.Element {
  const children = renderChildren(Array.from(element.childNodes), nested);
  const props: Record<string, unknown> = {
    onSubmit: (event: SubmitEvent) => {
      event.preventDefault();
      if (!context.onSend) return;
      const text = formMessage(event.currentTarget as HTMLFormElement);
      if (text) context.onSend(cap(text, MAX_SEND_CHARS));
    },
  };
  if (classes.length > 0) props.class = classes.join(" ");
  applyVisibility(element, props, context);
  return (
    <Dynamic component="form" {...props}>
      {children}
    </Dynamic>
  );
}

async function copyText(text: string) {
  const value = text.trim();
  if (!value) return;
  try {
    await navigator.clipboard.writeText(value);
    pushToast("ok", t("Copied"));
  } catch (error) {
    pushToast("danger", String(error));
  }
}

/** Turn the submitted controls into a readable message for the agent. Values
 * come from the live DOM, not from the parsed template. */
function formMessage(form: HTMLFormElement): string {
  const lines: string[] = [];
  for (const control of Array.from(form.elements)) {
    const line = controlLine(control);
    if (line) lines.push(line);
  }
  return lines.join("\n");
}

function controlLine(control: Element): string | null {
  const tag = control.tagName.toLowerCase();
  if (tag !== "input" && tag !== "select" && tag !== "textarea") return null;
  if (control.hasAttribute("disabled")) return null;
  const name = control.getAttribute("name");
  if (!name) return null;

  if (tag === "input") {
    const input = control as HTMLInputElement;
    const type = (input.type || "text").toLowerCase();
    if (type === "submit" || type === "button" || type === "reset") return null;
    if (type === "checkbox" || type === "radio") {
      if (!input.checked) return null;
      const label = groupLabel(control, name);
      const value = cleanText(input.value) || "yes";
      return `${label}: ${value === "on" ? "yes" : value}`;
    }
    const value = cleanText(input.value);
    return value ? `${fieldLabel(control, name)}: ${value}` : null;
  }

  if (tag === "select") {
    const select = control as HTMLSelectElement;
    const values = Array.from(select.selectedOptions)
      .map((option) => cleanText(option.value || option.textContent))
      .filter(Boolean);
    return values.length > 0
      ? `${fieldLabel(control, name)}: ${values.join(", ")}`
      : null;
  }

  const value = cleanText((control as HTMLTextAreaElement).value);
  return value ? `${fieldLabel(control, name)}: ${value}` : null;
}

function fieldLabel(control: Element, name: string): string {
  const label = control.closest("label");
  if (label) {
    const clone = label.cloneNode(true) as Element;
    clone
      .querySelectorAll("input, select, textarea, button, output, progress, meter")
      .forEach((node) => node.remove());
    const text = cleanText(clone.textContent);
    if (text) return cap(text, 60);
  }
  return cap(name, 60);
}

/** Radio and checkbox groups read better with the fieldset legend than with
 * the individual option's label. */
function groupLabel(control: Element, name: string): string {
  const fieldset = control.closest("fieldset");
  const legend = fieldset?.querySelector(":scope > legend");
  const text = cleanText(legend?.textContent);
  return text ? cap(text, 60) : fieldLabel(control, name);
}

// ─────────────────────────── helpers ───────────────────────────

function bindName(raw: string | null): string | null {
  if (!raw) return null;
  const name = raw.trim().replace(/^\$/, "");
  return BIND_NAME.test(name) && !BLOCKED_KEYS.has(name) ? name : null;
}

function defineGetter(
  props: Record<string, unknown>,
  name: string,
  read: () => unknown,
) {
  Object.defineProperty(props, name, {
    enumerable: true,
    configurable: true,
    get: read,
  });
}

function textProp(
  props: Record<string, unknown>,
  name: string,
  value: string,
  context: RenderContext,
  limit: number,
) {
  if (value.includes("{{")) {
    defineGetter(props, name, () =>
      cap(interpolate(value, context.state, context.locals), limit),
    );
  } else {
    props[name] = cap(value, limit);
  }
}

function numberProp(
  props: Record<string, unknown>,
  name: string,
  value: string,
  context: RenderContext,
  limit: number,
) {
  const read = (text: string) => {
    const number = Number.parseInt(text, 10);
    return Number.isFinite(number) ? Math.min(Math.max(number, 0), limit) : undefined;
  };
  if (value.includes("{{")) {
    defineGetter(props, name, () =>
      read(interpolate(value, context.state, context.locals)),
    );
  } else if (value) {
    props[name] = read(value);
  }
}

function validImageSource(src: string): boolean {
  const value = src.trim();
  return value.includes("{{") || SAFE_IMAGE.test(value);
}

function openExternal(url: string, onOpenUrl?: (url: string) => void) {
  if (onOpenUrl) {
    onOpenUrl(url);
    return;
  }
  void openUrl(url).catch(() => undefined);
}

function cleanText(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

function cap(value: string, limit: number): string {
  return value.length > limit ? value.slice(0, limit) : value;
}

// ─────────────────────────── svg ───────────────────────────

function renderSvgNode(element: Element, context: RenderContext): JSX.Element | null {
  const tag = element.tagName.toLowerCase();
  if (DROPPED_SVG_TAGS.has(tag)) return null;
  const nested = { ...context, depth: context.depth + 1 };
  if (!SVG_TAGS.has(tag)) {
    return <>{renderChildren(Array.from(element.childNodes), nested)}</>;
  }

  const props: Record<string, unknown> = {};
  for (const attribute of Array.from(element.attributes)) {
    const name = attribute.name;
    if (!SVG_ATTRS.has(name)) continue;
    if (!validSvgAttribute(name, attribute.value)) continue;
    props[name] = cap(attribute.value, name === "d" || name === "points" ? 20_000 : 200);
  }
  const children = renderChildren(Array.from(element.childNodes), nested);
  return (
    <Dynamic component={tag} isSVG {...props}>
      {children}
    </Dynamic>
  );
}

const SVG_COLOR = /^(none|currentColor|transparent|#[0-9a-fA-F]{3,8}|(?:rgba?|hsla?)\([0-9.,%\s/]+\)|[A-Za-z]{3,20})$/;
const SVG_NUMBER = /^[0-9eE+\-.,%\s]+$/;
const SVG_PATH = /^[0-9A-Za-z+\-.,\s]*$/;
const SVG_TRANSFORM = /^[0-9A-Za-z+\-.,()\s]*$/;
const SVG_VIEW_BOX = /^-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+$/;

function validSvgAttribute(name: string, value: string): boolean {
  const text = value.trim();
  if (text.length > 20_000) return false;
  switch (name) {
    case "fill":
    case "stroke":
      return text.length <= 60 && SVG_COLOR.test(text);
    case "d":
    case "points":
      return SVG_PATH.test(text);
    case "transform":
      return text.length <= 500 && SVG_TRANSFORM.test(text);
    case "viewBox":
      return SVG_VIEW_BOX.test(text);
    case "text-anchor":
      return text === "start" || text === "middle" || text === "end";
    case "dominant-baseline":
      return ["auto", "middle", "hanging", "central", "text-top", "text-bottom"].includes(text);
    case "stroke-linecap":
      return ["butt", "round", "square"].includes(text);
    case "stroke-linejoin":
      return ["miter", "round", "bevel"].includes(text);
    case "font-weight":
      return text === "normal" || text === "bold" || /^[1-9]00$/.test(text);
    case "role":
      return text === "img" || text === "presentation";
    case "aria-label":
      return text.length <= 200;
    default:
      return text.length <= 200 && SVG_NUMBER.test(text);
  }
}
