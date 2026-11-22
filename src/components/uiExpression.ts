// A tiny, side-effect-free expression language for inline interface blocks.
//
// The model writes ordinary HTML and sprinkles `{{ expression }}` into text or
// a few attributes. Expressions are parsed here and evaluated against the
// block's state; no JavaScript is ever compiled or executed, so a malformed or
// hostile expression can only fail to render. The grammar is intentionally
// small and familiar: literals, identifiers, member/index access, arithmetic,
// comparisons, `&&`/`||`/`!`, ternary and a fixed function set.

export type UiValues = Record<string, unknown>;

export const MAX_EXPRESSION_CHARS = 500;

export class UiExpressionError extends Error {}

type Token = {
  kind: "number" | "string" | "identifier" | "operator";
  value: string;
};

type Node =
  | { kind: "literal"; value: unknown }
  | { kind: "identifier"; name: string }
  | { kind: "member"; object: Node; name: string }
  | { kind: "index"; object: Node; index: Node }
  | { kind: "call"; name: string; args: Node[] }
  | { kind: "unary"; operator: string; operand: Node }
  | { kind: "binary"; operator: string; left: Node; right: Node }
  | { kind: "ternary"; test: Node; yes: Node; no: Node }
  | { kind: "array"; items: Node[] };

const OPERATORS = [
  "===",
  "!==",
  "==",
  "!=",
  "<=",
  ">=",
  "&&",
  "||",
  "+",
  "-",
  "*",
  "/",
  "%",
  "<",
  ">",
  "!",
  "?",
  ":",
  ".",
  ",",
  "(",
  ")",
  "[",
  "]",
];

const BLOCKED_KEYS = new Set(["__proto__", "prototype", "constructor"]);

class Parser {
  private tokens: Token[];
  private index = 0;
  private depth = 0;

  constructor(source: string) {
    this.tokens = tokenize(source);
  }

  parse(): Node {
    const node = this.parseTernary();
    if (this.index < this.tokens.length) {
      throw new UiExpressionError(`unexpected token ${this.peek()?.value ?? "end"}`);
    }
    return node;
  }

  private peek(): Token | undefined {
    return this.tokens[this.index];
  }

  private take(): Token | undefined {
    return this.tokens[this.index++];
  }

  private match(value: string): boolean {
    if (this.peek()?.value !== value) return false;
    this.index += 1;
    return true;
  }

  private expect(value: string): void {
    if (!this.match(value)) {
      throw new UiExpressionError(`expected ${value}`);
    }
  }

  private enter(): void {
    this.depth += 1;
    if (this.depth > 64) throw new UiExpressionError("expression is nested too deeply");
  }

  private leave(): void {
    this.depth -= 1;
  }

  private parseTernary(): Node {
    this.enter();
    const test = this.parseOr();
    const node: Node = this.match("?")
      ? {
          kind: "ternary",
          test,
          yes: this.parseTernary(),
          no: (this.expect(":"), this.parseTernary()),
        }
      : test;
    this.leave();
    return node;
  }

  private parseOr(): Node {
    let left = this.parseAnd();
    while (this.match("||")) {
      left = { kind: "binary", operator: "||", left, right: this.parseAnd() };
    }
    return left;
  }

  private parseAnd(): Node {
    let left = this.parseEquality();
    while (this.match("&&")) {
      left = { kind: "binary", operator: "&&", left, right: this.parseEquality() };
    }
    return left;
  }

  private parseEquality(): Node {
    let left = this.parseComparison();
    for (;;) {
      const operator = this.peek()?.value;
      if (operator !== "==" && operator !== "!=" && operator !== "===" && operator !== "!==") {
        return left;
      }
      this.take();
      left = { kind: "binary", operator, left, right: this.parseComparison() };
    }
  }

  private parseComparison(): Node {
    let left = this.parseAdditive();
    for (;;) {
      const operator = this.peek()?.value;
      if (operator !== "<" && operator !== ">" && operator !== "<=" && operator !== ">=") {
        return left;
      }
      this.take();
      left = { kind: "binary", operator, left, right: this.parseAdditive() };
    }
  }

  private parseAdditive(): Node {
    let left = this.parseMultiplicative();
    for (;;) {
      const operator = this.peek()?.value;
      if (operator !== "+" && operator !== "-") return left;
      this.take();
      left = { kind: "binary", operator, left, right: this.parseMultiplicative() };
    }
  }

  private parseMultiplicative(): Node {
    let left = this.parseUnary();
    for (;;) {
      const operator = this.peek()?.value;
      if (operator !== "*" && operator !== "/" && operator !== "%") return left;
      this.take();
      left = { kind: "binary", operator, left, right: this.parseUnary() };
    }
  }

  private parseUnary(): Node {
    this.enter();
    let node: Node;
    if (this.match("!") || this.match("-")) {
      const operator = this.tokens[this.index - 1].value;
      node = { kind: "unary", operator, operand: this.parseUnary() };
    } else {
      node = this.parsePostfix();
    }
    this.leave();
    return node;
  }

  private parsePostfix(): Node {
    let node = this.parsePrimary();
    for (;;) {
      if (this.match(".")) {
        const name = this.take();
        if (name?.kind !== "identifier") throw new UiExpressionError("expected a property name");
        if (BLOCKED_KEYS.has(name.value)) throw new UiExpressionError("property is not readable");
        node = { kind: "member", object: node, name: name.value };
        continue;
      }
      if (this.match("[")) {
        const index = this.parseTernary();
        this.expect("]");
        node = { kind: "index", object: node, index };
        continue;
      }
      return node;
    }
  }

  private parsePrimary(): Node {
    const token = this.take();
    if (!token) throw new UiExpressionError("unexpected end of expression");

    if (token.kind === "number") {
      return { kind: "literal", value: Number(token.value) };
    }
    if (token.kind === "string") {
      return { kind: "literal", value: token.value };
    }
    if (token.value === "(") {
      this.enter();
      const node = this.parseTernary();
      this.leave();
      this.expect(")");
      return node;
    }
    if (token.value === "[") {
      const items: Node[] = [];
      if (!this.match("]")) {
        do {
          items.push(this.parseTernary());
        } while (this.match(","));
        this.expect("]");
      }
      return { kind: "array", items };
    }
    if (token.kind === "identifier") {
      if (token.value === "true") return { kind: "literal", value: true };
      if (token.value === "false") return { kind: "literal", value: false };
      if (token.value === "null") return { kind: "literal", value: null };
      if (this.match("(")) {
        if (!(token.value in FUNCTIONS)) {
          throw new UiExpressionError(`unknown function ${token.value}`);
        }
        const args: Node[] = [];
        if (!this.match(")")) {
          do {
            args.push(this.parseTernary());
          } while (this.match(","));
          this.expect(")");
        }
        return { kind: "call", name: token.value, args };
      }
      return { kind: "identifier", name: token.value };
    }
    throw new UiExpressionError(`unexpected token ${token.value}`);
  }
}

function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  while (index < source.length) {
    const char = source[index];
    if (/\s/.test(char)) {
      index += 1;
      continue;
    }
    if (/[0-9]/.test(char)) {
      const match = source.slice(index).match(/^[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/);
      if (!match) throw new UiExpressionError("invalid number");
      tokens.push({ kind: "number", value: match[0] });
      index += match[0].length;
      continue;
    }
    if (char === '"' || char === "'") {
      let value = "";
      let cursor = index + 1;
      let closed = false;
      while (cursor < source.length) {
        const next = source[cursor];
        if (next === "\\") {
          const escaped = source[cursor + 1];
          if (escaped === undefined) break;
          value +=
            escaped === "n" ? "\n" : escaped === "t" ? "\t" : escaped === "r" ? "\r" : escaped;
          cursor += 2;
          continue;
        }
        if (next === char) {
          closed = true;
          cursor += 1;
          break;
        }
        value += next;
        cursor += 1;
      }
      if (!closed) throw new UiExpressionError("unterminated string");
      tokens.push({ kind: "string", value });
      index = cursor;
      continue;
    }
    if (/[A-Za-z_$]/.test(char)) {
      const match = source.slice(index).match(/^[A-Za-z_$][A-Za-z0-9_$]*/);
      if (!match) throw new UiExpressionError("invalid identifier");
      tokens.push({ kind: "identifier", value: match[0] });
      index += match[0].length;
      continue;
    }
    const operator = OPERATORS.find((candidate) => source.startsWith(candidate, index));
    if (!operator) throw new UiExpressionError(`unexpected character ${char}`);
    tokens.push({ kind: "operator", value: operator });
    index += operator.length;
  }
  return tokens;
}

function evaluateNode(node: Node, state: UiValues, locals: UiValues): unknown {
  switch (node.kind) {
    case "literal":
      return node.value;
    case "identifier": {
      const name = node.name.startsWith("$") ? node.name.slice(1) : node.name;
      if (Object.prototype.hasOwnProperty.call(locals, name)) return locals[name];
      if (Object.prototype.hasOwnProperty.call(locals, node.name)) return locals[node.name];
      if (BLOCKED_KEYS.has(name)) return undefined;
      return Object.prototype.hasOwnProperty.call(state, name) ? state[name] : undefined;
    }
    case "member": {
      const object = evaluateNode(node.object, state, locals);
      if (Array.isArray(object)) {
        if (node.name === "length") return object.length;
        return object.map((item) =>
          item && typeof item === "object" && !Array.isArray(item)
            ? (item as Record<string, unknown>)[node.name]
            : undefined,
        );
      }
      if (typeof object === "string" && node.name === "length") return object.length;
      if (object && typeof object === "object" && !BLOCKED_KEYS.has(node.name)) {
        return (object as Record<string, unknown>)[node.name];
      }
      return undefined;
    }
    case "index": {
      const object = evaluateNode(node.object, state, locals);
      const index = evaluateNode(node.index, state, locals);
      if (Array.isArray(object)) return object[toNumber(index)];
      if (
        object &&
        typeof object === "object" &&
        typeof index === "string" &&
        !BLOCKED_KEYS.has(index)
      ) {
        return (object as Record<string, unknown>)[index];
      }
      return typeof object === "string" ? object[toNumber(index)] : undefined;
    }
    case "call": {
      const fn = FUNCTIONS[node.name];
      return fn(node.args.map((argument) => evaluateNode(argument, state, locals)));
    }
    case "unary": {
      const value = evaluateNode(node.operand, state, locals);
      return node.operator === "!" ? !value : -toNumber(value);
    }
    case "binary":
      return evaluateBinary(node.operator, node.left, node.right, state, locals);
    case "ternary":
      return evaluateNode(node.test, state, locals)
        ? evaluateNode(node.yes, state, locals)
        : evaluateNode(node.no, state, locals);
    case "array":
      return node.items.map((item) => evaluateNode(item, state, locals));
  }
}

function evaluateBinary(
  operator: string,
  leftNode: Node,
  rightNode: Node,
  state: UiValues,
  locals: UiValues,
): unknown {
  const left = evaluateNode(leftNode, state, locals);
  if (operator === "&&") return left ? evaluateNode(rightNode, state, locals) : left;
  if (operator === "||") return left ? left : evaluateNode(rightNode, state, locals);
  const right = evaluateNode(rightNode, state, locals);
  switch (operator) {
    case "+":
      return typeof left === "string" || typeof right === "string"
        ? `${displayValue(left)}${displayValue(right)}`
        : toNumber(left) + toNumber(right);
    case "-":
      return toNumber(left) - toNumber(right);
    case "*":
      return toNumber(left) * toNumber(right);
    case "/":
      return toNumber(left) / toNumber(right);
    case "%":
      return toNumber(left) % toNumber(right);
    case "==":
    case "===":
      return looseEquals(left, right);
    case "!=":
    case "!==":
      return !looseEquals(left, right);
    case "<":
      return toNumber(left) < toNumber(right);
    case ">":
      return toNumber(left) > toNumber(right);
    case "<=":
      return toNumber(left) <= toNumber(right);
    case ">=":
      return toNumber(left) >= toNumber(right);
    default:
      throw new UiExpressionError(`unknown operator ${operator}`);
  }
}

function toNumber(value: unknown): number {
  if (typeof value === "number") return Number.isFinite(value) ? value : 0;
  if (typeof value === "string") {
    const number = Number.parseFloat(value.replace(/,/g, ""));
    return Number.isFinite(number) ? number : 0;
  }
  if (typeof value === "boolean") return value ? 1 : 0;
  return 0;
}

function toList(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function fieldOf(item: unknown, field: unknown): unknown {
  if (!item || typeof item !== "object") return undefined;
  const key = String(field ?? "");
  if (BLOCKED_KEYS.has(key)) return undefined;
  return (item as Record<string, unknown>)[key];
}

function looseEquals(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (left == null || right == null) return false;
  if (typeof left === "object" || typeof right === "object") return false;
  if (
    typeof left === "number" ||
    typeof right === "number" ||
    typeof left === "boolean" ||
    typeof right === "boolean"
  ) {
    return toNumber(left) === toNumber(right);
  }
  return false;
}

function compareValues(left: unknown, right: unknown): number {
  if (typeof left === "number" || typeof right === "number") {
    return toNumber(left) - toNumber(right);
  }
  return displayValue(left).localeCompare(displayValue(right));
}

function matches(left: unknown, operator: string, right: unknown): boolean {
  switch (operator) {
    case "==":
      return looseEquals(left, right);
    case "!=":
      return !looseEquals(left, right);
    case ">":
      return toNumber(left) > toNumber(right);
    case "<":
      return toNumber(left) < toNumber(right);
    case ">=":
      return toNumber(left) >= toNumber(right);
    case "<=":
      return toNumber(left) <= toNumber(right);
    case "contains":
      return displayValue(left).includes(displayValue(right));
    default:
      return false;
  }
}

function compactNumber(value: number): string {
  const absolute = Math.abs(value);
  if (absolute >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (absolute >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

const FUNCTIONS: Record<string, (args: unknown[]) => unknown> = {
  sum: (args) => toList(args[0]).reduce<number>((total, value) => total + toNumber(value), 0),
  count: (args) => {
    const value = args[0];
    if (Array.isArray(value)) return value.length;
    if (typeof value === "string" || value == null) return 0;
    if (typeof value === "object") return Object.keys(value as object).length;
    return 0;
  },
  avg: (args) => {
    const list = toList(args[0]);
    return list.length ? (FUNCTIONS.sum([list]) as number) / list.length : 0;
  },
  min: (args) => {
    const list = toList(args[0]).map(toNumber);
    return list.length ? Math.min(...list) : 0;
  },
  max: (args) => {
    const list = toList(args[0]).map(toNumber);
    return list.length ? Math.max(...list) : 0;
  },
  round: (args) => {
    const digits = toNumber(args[1] ?? 0);
    const scale = 10 ** Math.max(0, Math.min(6, digits));
    return Math.round(toNumber(args[0]) * scale) / scale;
  },
  abs: (args) => Math.abs(toNumber(args[0])),
  floor: (args) => Math.floor(toNumber(args[0])),
  ceil: (args) => Math.ceil(toNumber(args[0])),
  clamp: (args) => {
    const value = toNumber(args[0]);
    const low = toNumber(args[1]);
    const high = toNumber(args[2]);
    return Math.min(Math.max(value, Math.min(low, high)), Math.max(low, high));
  },
  format: (args) => {
    const value = toNumber(args[0]);
    const style = String(args[1] ?? "compact");
    if (style === "percent") return `${compactNumber(value)}%`;
    if (style === "currency")
      return `$${value.toLocaleString("en-US", { maximumFractionDigits: 2 })}`;
    return compactNumber(value);
  },
  join: (args) =>
    toList(args[0])
      .map(displayValue)
      .filter(Boolean)
      .join(String(args[1] ?? ", ")),
  filter: (args) =>
    toList(args[0]).filter((item) =>
      matches(fieldOf(item, args[1]), String(args[2] ?? "=="), args[3]),
    ),
  sort: (args) => {
    const field = args[1];
    const direction = String(args[2] ?? "asc") === "desc" ? -1 : 1;
    return toList(args[0])
      .slice()
      .sort(
        (left, right) => compareValues(fieldOf(left, field), fieldOf(right, field)) * direction,
      );
  },
  includes: (args) =>
    toList(args[0]).some(
      (item) => item === args[1] || displayValue(item) === displayValue(args[1]),
    ),
  toggle: (args) => {
    const list = toList(args[0]);
    return list.some((item) => item === args[1])
      ? list.filter((item) => item !== args[1])
      : [...list, args[1]];
  },
  append: (args) => [...toList(args[0]), args[1]],
  removeAt: (args) => {
    const index = toNumber(args[1]);
    return toList(args[0]).filter((_, position) => position !== index);
  },
  at: (args) => toList(args[0])[toNumber(args[1])],
  range: (args) => {
    const count = Math.max(0, Math.min(200, Math.floor(toNumber(args[0]))));
    return Array.from({ length: count }, (_, index) => index);
  },
  default: (args) => (args[0] == null || args[0] === "" ? args[1] : args[0]),
  upper: (args) => displayValue(args[0]).toUpperCase(),
  lower: (args) => displayValue(args[0]).toLowerCase(),
  plural: (args) =>
    `${toNumber(args[0])} ${toNumber(args[0]) === 1 ? String(args[1] ?? "") : String(args[2] ?? args[1] ?? "")}`,
};

/** Evaluate one expression. Throws `UiExpressionError` on bad input. */
export function evaluate(source: string, state: UiValues = {}, locals: UiValues = {}): unknown {
  const text = source.trim();
  if (!text) return undefined;
  if (text.length > MAX_EXPRESSION_CHARS) {
    throw new UiExpressionError("expression is too long");
  }
  return evaluateNode(new Parser(text).parse(), state, locals);
}

/** A value as it should appear in text. */
export function displayValue(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number") return Number.isFinite(value) ? String(value) : "";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (Array.isArray(value)) return value.map(displayValue).join(", ");
  if (typeof value === "object") {
    try {
      const text = JSON.stringify(value);
      return text.length > 2_000 ? `${text.slice(0, 2_000)}…` : text;
    } catch {
      return "";
    }
  }
  return "";
}

const INTERPOLATION = /\{\{([^{}]*)\}\}/g;

/** Replace `{{ expression }}` in a string. A malformed expression is left as
 * written instead of erasing the surrounding copy. */
export function interpolate(template: string, state: UiValues = {}, locals: UiValues = {}): string {
  const result = template.replace(INTERPOLATION, (match, expression: string) => {
    try {
      return displayValue(evaluate(expression, state, locals));
    } catch {
      return match;
    }
  });
  return result.length > 20_000 ? result.slice(0, 20_000) : result;
}

/** Apply `name = expression` assignments separated by `;`. Returns the names
 * that were written. Invalid assignments are skipped. */
export function applyAssignments(
  spec: string,
  state: UiValues,
  locals: UiValues,
  assign: (name: string, value: unknown) => void,
): string[] {
  const written: string[] = [];
  for (const part of spec.split(";")) {
    const match = part.match(/^\s*\$?([A-Za-z_][A-Za-z0-9_$]*)\s*=(?!=)\s*([\s\S]+)$/);
    if (!match) continue;
    try {
      assign(match[1], evaluate(match[2], state, locals));
      written.push(match[1]);
    } catch {
      // A bad assignment leaves the state untouched.
    }
  }
  return written;
}

export function asList(value: unknown): unknown[] {
  return toList(value);
}

export function hashString(value: string): string {
  let hash = 5381;
  for (let index = 0; index < value.length; index += 1) {
    hash = ((hash << 5) + hash + value.charCodeAt(index)) | 0;
  }
  return `${(hash >>> 0).toString(36)}${value.length.toString(36)}`;
}
