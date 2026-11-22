// SVG sanitization. Only allowlisted attributes with validated value shapes
// reach the renderer; the tag allowlists live in `allowlist.ts`.

import { SVG_ATTRS } from "./allowlist";
import { cap } from "./attributes";

const SVG_COLOR = /^(none|currentColor|transparent|#[0-9a-fA-F]{3,8}|(?:rgba?|hsla?)\([0-9.,%\s/]+\)|[A-Za-z]{3,20})$/;
const SVG_NUMBER = /^[0-9eE+\-.,%\s]+$/;
const SVG_PATH = /^[0-9A-Za-z+\-.,\s]*$/;
const SVG_TRANSFORM = /^[0-9A-Za-z+\-.,()\s]*$/;
const SVG_VIEW_BOX = /^-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+\s+-?[0-9.]+$/;

export function svgProps(element: Element): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  for (const attribute of Array.from(element.attributes)) {
    const name = attribute.name;
    if (!SVG_ATTRS.has(name)) continue;
    if (!validSvgAttribute(name, attribute.value)) continue;
    props[name] = cap(attribute.value, name === "d" || name === "points" ? 20_000 : 200);
  }
  return props;
}

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
