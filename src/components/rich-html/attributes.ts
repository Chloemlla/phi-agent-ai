// Attribute validation. Every value read from the inert DOM is checked
// against an allowlist here, and dynamic values stay behind getters so the
// browser never sees an unchecked URL or text.

import { platform } from "../../platform";
import { applyAssignments, evaluate, interpolate } from "../uiExpression";
import {
  BIND_NAME,
  BLANK_IMAGE,
  BLOCKED_KEYS,
  BUTTON_TYPES,
  DIRS,
  HTML_TAG_ATTRS,
  INPUT_TYPES,
  LI_STATES,
  OL_TYPES,
  SAFE_IMAGE,
  SAFE_LINK,
  SCOPES,
  UI_ATTR_VALUES,
} from "./allowlist";
import type { RenderContext } from "./context";

export function htmlProps(
  tag: string,
  element: Element,
  classes: string[],
  context: RenderContext,
): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  if (classes.length > 0) props.class = classes.join(" ");
  applyGlobalAttributes(element, props, context);
  applyUiAttributes(element, props);
  applyVisibility(element, props, context);

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

  const setExpression = element.getAttribute("data-ui-set");
  if (setExpression && tag !== "button") {
    const run = (event: Event) => {
      event.preventDefault();
      applyAssignments(setExpression, context.state, context.locals, context.assign);
    };
    props.onClick = run;
    props.onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter" || event.key === " ") run(event);
    };
    props.role = "button";
    props.tabIndex = 0;
    props.class = [props.class as string | undefined, "ui-clickable"].filter(Boolean).join(" ");
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
  const ariaLabel = element.getAttribute("aria-label");
  if (ariaLabel) textProp(props, "aria-label", ariaLabel, context, 200);
  const lang = element.getAttribute("lang");
  if (lang) props.lang = cap(lang, 20);
  const dir = element.getAttribute("dir");
  if (dir && DIRS.has(dir)) props.dir = dir;
}

/** Tokenized layout attributes. They are only forwarded when both the name and
 * the value are known, so the stylesheet never sees model-invented values. */
function applyUiAttributes(element: Element, props: Record<string, unknown>) {
  for (const [name, allowed] of Object.entries(UI_ATTR_VALUES)) {
    const value = element.getAttribute(name);
    if (value !== null && allowed.has(value)) props[name] = value;
  }
  if (element.getAttribute("data-wrap") === "nowrap") props["data-wrap"] = "nowrap";
  if (element.hasAttribute("data-border")) props["data-border"] = "";
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

function applyImage(element: Element, props: Record<string, unknown>, context: RenderContext) {
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

function applyOption(element: Element, props: Record<string, unknown>, context: RenderContext) {
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
      context.assign(name, select.multiple ? values : (values[0] ?? ""));
    };
  }
}

export function applyVisibility(
  element: Element,
  props: Record<string, unknown>,
  context: RenderContext,
) {
  const expression = element.getAttribute("data-ui-show") ?? element.getAttribute("data-ui-if");
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

// ─────────────────────────── helpers ───────────────────────────

function bindName(raw: string | null): string | null {
  if (!raw) return null;
  const name = raw.trim().replace(/^\$/, "");
  return BIND_NAME.test(name) && !BLOCKED_KEYS.has(name) ? name : null;
}

function defineGetter(props: Record<string, unknown>, name: string, read: () => unknown) {
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
    defineGetter(props, name, () => cap(interpolate(value, context.state, context.locals), limit));
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
    defineGetter(props, name, () => read(interpolate(value, context.state, context.locals)));
  } else if (value) {
    props[name] = read(value);
  }
}

export function validImageSource(src: string): boolean {
  const value = src.trim();
  return value.includes("{{") || SAFE_IMAGE.test(value);
}

function openExternal(url: string, onOpenUrl?: (url: string) => void) {
  if (onOpenUrl) {
    onOpenUrl(url);
    return;
  }
  void platform.openExternal(url).catch(() => undefined);
}

export function cleanText(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

export function cap(value: string, limit: number): string {
  return value.length > limit ? value.slice(0, limit) : value;
}
