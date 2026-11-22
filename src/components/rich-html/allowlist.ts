// Allowlists and budgets for inline interface blocks. Everything the model
// writes is discarded unless it appears here; classes, tags and attributes
// are validated against these sets by the attribute and SVG helpers.

export const MAX_DEPTH = 32;
export const MAX_NODES = 2_500;
export const MAX_SEND_CHARS = 4_000;
export const MAX_EACH_ROWS = 200;
export const MAX_CACHED_STATES = 100;

export const SVG_NS = "http://www.w3.org/2000/svg";
export const SAFE_LINK = /^(https?:\/\/|mailto:)/i;
export const SAFE_IMAGE = /^https:\/\/[^\s"'<>]+$/i;
/** Shown instead of a rejected dynamic URL so the browser makes no request. */
export const BLANK_IMAGE = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";
export const BIND_NAME = /^[A-Za-z_][A-Za-z0-9_]{0,39}$/;
export const BLOCKED_KEYS = new Set(["__proto__", "prototype", "constructor"]);

/** Elements rendered as-is. Everything else is unwrapped or dropped. */
export const HTML_TAGS = new Set([
  "a",
  "abbr",
  "address",
  "article",
  "aside",
  "b",
  "blockquote",
  "br",
  "button",
  "caption",
  "cite",
  "code",
  "dd",
  "del",
  "details",
  "div",
  "dl",
  "dt",
  "em",
  "fieldset",
  "figcaption",
  "figure",
  "footer",
  "form",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "header",
  "hr",
  "i",
  "img",
  "input",
  "ins",
  "kbd",
  "label",
  "legend",
  "li",
  "main",
  "mark",
  "meter",
  "nav",
  "ol",
  "optgroup",
  "option",
  "output",
  "p",
  "pre",
  "progress",
  "q",
  "s",
  "samp",
  "section",
  "select",
  "small",
  "span",
  "strong",
  "sub",
  "summary",
  "sup",
  "table",
  "tbody",
  "td",
  "textarea",
  "tfoot",
  "th",
  "thead",
  "time",
  "tr",
  "u",
  "ul",
  "var",
]);

export const VOID_TAGS = new Set(["br", "hr", "img", "input"]);

/** Elements whose whole subtree is removed. */
export const DROPPED_TAGS = new Set([
  "applet",
  "audio",
  "base",
  "canvas",
  "dialog",
  "embed",
  "frame",
  "frameset",
  "iframe",
  "link",
  "math",
  "meta",
  "noscript",
  "object",
  "picture",
  "portal",
  "slot",
  "source",
  "style",
  "track",
  "video",
]);

/** Host-styled component vocabulary. Any other class token is discarded. */
export const ALLOWED_CLASSES = new Set([
  "ui-col",
  "ui-row",
  "ui-box",
  "ui-grid",
  "ui-split",
  "ui-card",
  "ui-callout",
  "ui-info",
  "ui-ok",
  "ui-warn",
  "ui-danger",
  "ui-badge",
  "ui-metrics",
  "ui-metric",
  "ui-steps",
  "ui-kv",
  "ui-chart",
  "ui-chips",
  "ui-spacer",
  "ui-divider",
  "ui-figure",
  "ui-caption",
  "ui-carousel",
  "ui-icon",
  "ui-muted",
  "ui-num",
  "ui-clickable",
]);

/** Layout attributes: a token-sized subset of flex/grid control for the model.
 * They stay on the element only when their value is in the matching set. */
export const UI_STEPS = new Set(["0", "1", "2", "3", "4", "5", "6"]);
export const UI_COLUMNS = new Set(["1", "2", "3", "4", "5", "6"]);
export const UI_ALIGN = new Set(["start", "center", "end", "stretch"]);
export const UI_JUSTIFY = new Set(["start", "center", "end", "between"]);
export const UI_TONES = new Set([
  "ink",
  "muted",
  "faint",
  "accent",
  "info",
  "ok",
  "warn",
  "danger",
  "purple",
  "pink",
  "chart1",
  "chart2",
  "chart3",
  "chart4",
  "chart5",
  "chart6",
  "chart7",
  "chart8",
]);
export const UI_BACKGROUNDS = new Set([
  "soft",
  "surface",
  "inset",
  "info",
  "ok",
  "warn",
  "danger",
  "accent",
]);
export const UI_RADII = new Set(["sm", "md", "lg", "xl", "full"]);
export const UI_SIZES = new Set(["xs", "sm", "md", "lg", "xl", "2xl"]);
export const UI_WEIGHTS = new Set(["normal", "medium", "semibold"]);
export const UI_TEXT_ALIGN = new Set(["left", "center", "right"]);
export const UI_FLEX = new Set(["1", "2", "3", "4"]);
export const UI_DIRECTIONS = new Set(["row", "col"]);

export const UI_ATTR_VALUES: Record<string, Set<string>> = {
  "data-gap": UI_STEPS,
  "data-pad": UI_STEPS,
  "data-cols": UI_COLUMNS,
  "data-align": UI_ALIGN,
  "data-justify": UI_JUSTIFY,
  "data-tone": UI_TONES,
  "data-bg": UI_BACKGROUNDS,
  "data-radius": UI_RADII,
  "data-size": UI_SIZES,
  "data-weight": UI_WEIGHTS,
  "data-text-align": UI_TEXT_ALIGN,
  "data-width": new Set(["full", "auto"]),
  "data-flex": UI_FLEX,
  "data-dir": UI_DIRECTIONS,
  "data-fit": new Set(["cover", "contain"]),
  "data-item": new Set(["sm", "md", "lg"]),
};

/** SVG presentation colors that resolve to host theme tokens. */
export const SVG_COLOR_TOKENS: Record<string, string> = {
  ink: "var(--text-primary)",
  muted: "var(--text-secondary)",
  faint: "var(--text-muted)",
  surface: "var(--bg-subtle)",
  accent: "var(--accent)",
  info: "var(--accent)",
  ok: "var(--ok)",
  success: "var(--ok)",
  warn: "var(--warn)",
  warning: "var(--warn)",
  danger: "var(--danger)",
  purple: "var(--chart-7)",
  pink: "var(--chart-5)",
  chart1: "var(--chart-1)",
  chart2: "var(--chart-2)",
  chart3: "var(--chart-3)",
  chart4: "var(--chart-4)",
  chart5: "var(--chart-5)",
  chart6: "var(--chart-6)",
  chart7: "var(--chart-7)",
  chart8: "var(--chart-8)",
};

export const SVG_TAGS = new Set([
  "svg",
  "g",
  "path",
  "rect",
  "circle",
  "ellipse",
  "line",
  "polyline",
  "polygon",
  "text",
  "tspan",
  "desc",
]);

export const DROPPED_SVG_TAGS = new Set([
  "a",
  "animate",
  "animatemotion",
  "animatetransform",
  "clippath",
  "defs",
  "filter",
  "foreignobject",
  "image",
  "lineargradient",
  "marker",
  "mask",
  "pattern",
  "radialgradient",
  "script",
  "set",
  "stop",
  "style",
  "switch",
  "symbol",
  "textpath",
  "title",
  "use",
]);

export const SVG_ATTRS = new Set([
  "viewBox",
  "width",
  "height",
  "x",
  "y",
  "x1",
  "x2",
  "y1",
  "y2",
  "cx",
  "cy",
  "r",
  "rx",
  "ry",
  "d",
  "points",
  "fill",
  "fill-opacity",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-dasharray",
  "stroke-opacity",
  "opacity",
  "transform",
  "text-anchor",
  "dominant-baseline",
  "font-size",
  "font-weight",
  "aria-label",
  "role",
]);

export const HTML_TAG_ATTRS: Record<string, Set<string>> = {
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
    "type",
    "name",
    "value",
    "placeholder",
    "min",
    "max",
    "step",
    "checked",
    "disabled",
    "required",
    "readonly",
    "multiple",
  ]),
  textarea: new Set(["name", "rows", "cols", "placeholder", "disabled", "required", "readonly"]),
  select: new Set(["name", "multiple", "disabled", "required"]),
  option: new Set(["value", "selected", "disabled", "label"]),
  optgroup: new Set(["label", "disabled"]),
  fieldset: new Set(["disabled"]),
  img: new Set(["src", "alt", "width", "height"]),
};

export const INPUT_TYPES = new Set([
  "text",
  "search",
  "number",
  "range",
  "date",
  "time",
  "email",
  "url",
  "tel",
  "checkbox",
  "radio",
]);
export const DIRS = new Set(["ltr", "rtl", "auto"]);
export const OL_TYPES = new Set(["1", "a", "A", "i", "I"]);
export const SCOPES = new Set(["row", "col", "rowgroup", "colgroup"]);
export const LI_STATES = new Set(["done", "current", "todo"]);
export const BUTTON_TYPES = new Set(["submit", "button", "reset"]);

export function allowedClasses(element: Element): string[] {
  const classes: string[] = [];
  for (const token of element.classList) {
    if (ALLOWED_CLASSES.has(token)) classes.push(token);
  }
  return classes;
}
