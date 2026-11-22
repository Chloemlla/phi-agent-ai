import { describe, expect, test } from "bun:test";
import { splitInlineUi } from "../src/components/inlineUi";

describe("splitInlineUi", () => {
  test("returns plain text as a single segment", () => {
    expect(splitInlineUi("Just prose\nover two lines")).toEqual([
      { kind: "text", text: "Just prose\nover two lines" },
    ]);
  });

  test("splits a closed fence into prose, html and prose", () => {
    const input = "Before\n```html\n<button>Hi</button>\n```\nAfter";
    expect(splitInlineUi(input)).toEqual([
      { kind: "text", text: "Before" },
      { kind: "html", html: "<button>Hi</button>" },
      { kind: "text", text: "After" },
    ]);
  });

  test("keeps the remaining lines of an unterminated fence as html", () => {
    expect(splitInlineUi("Intro\n```html\n<b>open</b>")).toEqual([
      { kind: "text", text: "Intro" },
      { kind: "html", html: "<b>open</b>" },
    ]);
  });

  test("supports tilde fences", () => {
    expect(splitInlineUi("~~~html\n<i>x</i>\n~~~")).toEqual([
      { kind: "html", html: "<i>x</i>" },
    ]);
  });

  test("leaves non-html fences as prose", () => {
    const input = "```js\nconst x = 1;\n```";
    expect(splitInlineUi(input)).toEqual([{ kind: "text", text: input }]);
  });
});
