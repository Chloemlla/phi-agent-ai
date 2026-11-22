import { describe, expect, test } from "bun:test";
import { evaluate, UiExpressionError } from "../src/components/uiExpression";

describe("evaluate", () => {
  test("evaluates arithmetic with precedence", () => {
    expect(evaluate("2 + 3 * 4")).toBe(14);
    expect(evaluate("(2 + 3) * 4")).toBe(20);
  });

  test("evaluates comparisons, logic and ternaries", () => {
    expect(evaluate("3 > 2 && 2 >= 2")).toBe(true);
    expect(evaluate("1 === 1 ? 'yes' : 'no'")).toBe("yes");
    expect(evaluate("1 != 2 || false")).toBe(true);
  });

  test("concatenates when either operand is a string", () => {
    expect(evaluate("'total: ' + 3")).toBe("total: 3");
  });

  test("reads state and nested members", () => {
    const state = { items: [{ name: "a" }, { name: "b" }], count: 2 };
    expect(evaluate("count + 1", state)).toBe(3);
    expect(evaluate("items[0].name", state)).toBe("a");
    expect(evaluate("items.length", state)).toBe(2);
  });

  test("runs the fixed function set", () => {
    expect(evaluate("sum([1, 2, 3])")).toBe(6);
    expect(evaluate("upper('ok')")).toBe("OK");
  });

  test("rejects blocked prototype member access", () => {
    expect(() => evaluate("value.__proto__", { value: {} })).toThrow(UiExpressionError);
    expect(() => evaluate("value.constructor", { value: {} })).toThrow(UiExpressionError);
  });

  test("does not expose blocked keys through indexing", () => {
    expect(evaluate("value['constructor']", { value: {} })).toBeUndefined();
  });

  test("throws UiExpressionError for malformed input", () => {
    expect(() => evaluate("1 +")).toThrow(UiExpressionError);
    expect(() => evaluate("missing('x')")).toThrow(UiExpressionError);
    expect(() => evaluate("'unterminated")).toThrow(UiExpressionError);
  });
});
