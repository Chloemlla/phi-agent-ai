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
// Allowlists, state and attribute validation live under `rich-html/`; this
// file owns the component and the JSX rendering walk.

import { For, Show, createEffect, createMemo, untrack, type JSX } from "solid-js";
import { createStore } from "solid-js/store";
import { Dynamic } from "solid-js/web";
import { t } from "../i18n";
import { pushToast } from "../toast";
import { createCoalesced } from "./inlineUi";
import { InlineChart, readChartTable } from "./InlineChart";
import { RichSandbox } from "./RichSandbox";
import {
  applyAssignments,
  asList,
  evaluate,
  hashString,
  interpolate,
  type UiValues,
} from "./uiExpression";
import {
  BIND_NAME,
  BLOCKED_KEYS,
  BUTTON_TYPES,
  DROPPED_SVG_TAGS,
  DROPPED_TAGS,
  HTML_TAGS,
  MAX_DEPTH,
  MAX_EACH_ROWS,
  MAX_NODES,
  MAX_SEND_CHARS,
  SVG_NS,
  SVG_TAGS,
  UI_TONES,
  VOID_TAGS,
  allowedClasses,
} from "./rich-html/allowlist";
import { applyVisibility, cap, htmlProps, validImageSource } from "./rich-html/attributes";
import type { RenderContext } from "./rich-html/context";
import { formMessage } from "./rich-html/forms";
import { HOST_ICONS, hostIconSize } from "./rich-html/icons";
import {
  cacheState,
  compatible,
  mergeState,
  parseBlock,
  plain,
  stateCache,
} from "./rich-html/state";
import { svgProps } from "./rich-html/svg";

/** A block longer than this is shown as a notice instead of being rendered. */
export const INLINE_UI_MAX_CHARS = 100_000;

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
  // Streaming appends to the same block. Parsing the growing document at most
  // once per coalescing window keeps token-rate work out of the parser; a
  // durable message that never changes parses once.
  const liveHtml = createCoalesced(() => props.html, 64);
  const parsed = createMemo(() => parseBlock(liveHtml()), firstBlock);

  // New content may add state names; already-known names keep their value.
  createEffect(() => {
    const block = parsed();
    const key = `ui:${hashString(liveHtml())}`;
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
    cacheState(`ui:${hashString(liveHtml())}`, plain(untrack(() => state)) as UiValues);
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

  return <Show when={content()}>{(rendered) => <div class="inline-ui">{rendered()}</div>}</Show>;
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
    asList(evaluate(expression, props.context.state, props.context.locals)).slice(0, MAX_EACH_ROWS),
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
    if (element.hasAttribute("data-ui-rich")) {
      return <RichSandbox html={element.innerHTML} onOpenUrl={context.onOpenUrl} />;
    }
    return element.hasAttribute("data-ui-each") ? (
      <EachView element={element as HTMLTemplateElement} context={context} />
    ) : null;
  }
  if (DROPPED_TAGS.has(tag)) return null;

  const classes = allowedClasses(element);
  if (classes.includes("ui-icon")) {
    const Icon = HOST_ICONS[(element.getAttribute("data-icon") ?? "").trim()];
    if (!Icon) {
      // An unknown name falls back to the element's own content.
      return <>{renderChildren(Array.from(element.childNodes), nested)}</>;
    }
    const props: Record<string, unknown> = { class: "ui-icon" };
    const tone = element.getAttribute("data-tone");
    if (tone && UI_TONES.has(tone)) props["data-tone"] = tone;
    const title = element.getAttribute("title");
    if (title) props.title = cap(title, 200);
    const label = element.getAttribute("aria-label");
    if (label) {
      props["aria-label"] = cap(label, 200);
      props.role = "img";
    } else {
      props["aria-hidden"] = "true";
    }
    applyVisibility(element, props, context);
    const size = hostIconSize(element.getAttribute("data-size"));
    return (
      <span {...props}>
        <Icon size={size} aria-hidden="true" />
      </span>
    );
  }
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
  const action = setExpression !== null || sendExpression !== null || copyExpression !== null;
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
        applyAssignments(setExpression, context.state, context.locals, context.assign);
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
  return ["ui-btn", primary ? "ui-btn-primary" : "", inert ? "ui-btn-inert" : "", ...classes]
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

// ─────────────────────────── svg ───────────────────────────

function renderSvgNode(element: Element, context: RenderContext): JSX.Element | null {
  const tag = element.tagName.toLowerCase();
  if (DROPPED_SVG_TAGS.has(tag)) return null;
  const nested = { ...context, depth: context.depth + 1 };
  if (!SVG_TAGS.has(tag)) {
    return <>{renderChildren(Array.from(element.childNodes), nested)}</>;
  }

  const children = renderChildren(Array.from(element.childNodes), nested);
  return (
    <Dynamic component={tag} isSVG {...svgProps(element, context)}>
      {children}
    </Dynamic>
  );
}
