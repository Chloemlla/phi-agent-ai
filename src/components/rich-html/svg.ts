// SVG sanitization. Only allowlisted attributes with validated value shapes
// reach the renderer; the tag allowlists live in `allowlist.ts`. Attribute
// values may carry `{{ expressions }}` so a drawing can follow block state,
// and color names resolve to host theme tokens instead of raw hex values.

import { interpolate } from "../uiExpression";
import { SVG_COLOR_TOKENS, SVG_ATTRS } from "./allowlist";
import { applyVisibility, cap } from "./attributes";
import type { RenderContext } from "./context";

const SVG_COLOR =
  /^(none|currentColor|transparent|#[0-9a-fA-F]{3,8}|(?:rgba?|hsla?)\([0-9.,%\s/]+\)|[A-Za-z]{3,20})$/;
const SVG_NUMBER = /^[0-9eE+\-.,%\s]+$/;
const SVG_PATH = /^[0-9A-Za-z+\-.,\s]*$/;
const SVG_TRANSFORM = /^[0-9A-Za-z+\-.,()\s]*$/;
const SVG_VIEW_BOX = /^-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+$/;

export function svgProps(element: Element, context: RenderContext): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  for (const attribute of Array.from(element.attributes)) {
    const name = attribute.name;
    if (!SVG_ATTRS.has(name)) continue;
    const raw = attribute.value;
    if (raw.includes("{{")) {
      defineGetter(props, name, () =>
        normalizeSvgAttribute(name, interpolate(raw, context.state, context.locals).trim()),
      );
      continue;
    }
    const value = normalizeSvgAttribute(name, raw);
    if (value !== undefined) props[name] = value;
  }
  // Conditional visibility works on SVG groups and shapes too, so a diagram
  // can highlight a part without re-rendering the whole block.
  applyVisibility(element, props, context);
  return props;
}

/** A validated SVG attribute value, or undefined to drop the attribute. */
function normalizeSvgAttribute(name: string, value: string): string | undefined {
  const text = value.trim();
  if (text.length > 20_000) return undefined;
  switch (name) {
    case "fill":
    case "stroke": {
      if (text.length > 60) return undefined;
      const token = SVG_COLOR_TOKENS[text];
      if (token) return token;
      return SVG_COLOR.test(text) ? text : undefined;
    }
    case "d":
    case "points":
      return SVG_PATH.test(text) ? cap(text, 20_000) : undefined;
    case "transform":
      return text.length <= 500 && SVG_TRANSFORM.test(text) ? text : undefined;
    case "viewBox":
      return SVG_VIEW_BOX.test(text) ? text : undefined;
    case "text-anchor":
      return text === "start" || text === "middle" || text === "end" ? text : undefined;
    case "dominant-baseline":
      return ["auto", "middle", "hanging", "central", "text-top", "text-bottom"].includes(text)
        ? text
        : undefined;
    case "stroke-linecap":
      return ["butt", "round", "square"].includes(text) ? text : undefined;
    case "stroke-linejoin":
      return ["miter", "round", "bevel"].includes(text) ? text : undefined;
    case "font-weight":
      return text === "normal" || text === "bold" || /^[1-9]00$/.test(text) ? text : undefined;
    case "role":
      return text === "img" || text === "presentation" ? text : undefined;
    case "aria-label":
      return cap(text, 200);
    default:
      return text.length <= 200 && SVG_NUMBER.test(text) ? text : undefined;
  }
}

function defineGetter(props: Record<string, unknown>, name: string, read: () => unknown) {
  Object.defineProperty(props, name, {
    enumerable: true,
    configurable: true,
    get: read,
  });
}
