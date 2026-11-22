import { describe, expect, test } from "bun:test";
import { createInlineUiSplitter, splitInlineUi } from "../src/components/inlineUi";

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
    expect(splitInlineUi("~~~html\n<i>x</i>\n~~~")).toEqual([{ kind: "html", html: "<i>x</i>" }]);
  });

  test("leaves non-html fences as prose", () => {
    const input = "```js\nconst x = 1;\n```";
    expect(splitInlineUi(input)).toEqual([{ kind: "text", text: input }]);
  });

  test("splits a top-level raw HTML block out of the stream", () => {
    const input = 'Intro\n<span class="ui-badge">ok</span>\n\nAfter';
    expect(splitInlineUi(input)).toEqual([
      { kind: "text", text: "Intro" },
      { kind: "html", html: '<span class="ui-badge">ok</span>' },
      { kind: "text", text: "After" },
    ]);
  });

  test("keeps HTML-looking lines inside a non-html fence as prose", () => {
    const input = "```js\n<div>not an interface</div>\n```\nAfter";
    expect(splitInlineUi(input)).toEqual([{ kind: "text", text: input }]);
  });

  test("keeps an unterminated raw HTML block as html", () => {
    expect(splitInlineUi("Intro\n<div>open")).toEqual([
      { kind: "text", text: "Intro" },
      { kind: "html", html: "<div>open" },
    ]);
  });

  test("renders an html fence after a raw HTML block", () => {
    const input = "<div>raw</div>\n\n```html\n<b>fence</b>\n```";
    expect(splitInlineUi(input)).toEqual([
      { kind: "html", html: "<div>raw</div>" },
      { kind: "html", html: "<b>fence</b>" },
    ]);
  });

  test("an incremental stream switches from prose to a raw HTML block", () => {
    const splitter = createInlineUiSplitter();
    expect(splitter.update("Intro\n<sp")).toEqual([{ kind: "text", text: "Intro\n<sp" }]);
    expect(splitter.update("Intro\n<span>a</span>")).toEqual([
      { kind: "text", text: "Intro" },
      { kind: "html", html: "<span>a</span>" },
    ]);
    expect(splitter.update("Intro\n<span>a</span>\n\nAfter")).toEqual([
      { kind: "text", text: "Intro" },
      { kind: "html", html: "<span>a</span>" },
      { kind: "text", text: "After" },
    ]);
  });

  test("an incremental stream does not split HTML inside a code fence", () => {
    const splitter = createInlineUiSplitter();
    expect(splitter.update("```js\n<div>")).toEqual([{ kind: "text", text: "```js\n<div>" }]);
    expect(splitter.update("```js\n<div>x</div>\n```\nAfter")).toEqual([
      { kind: "text", text: "```js\n<div>x</div>\n```\nAfter" },
    ]);
  });
});
