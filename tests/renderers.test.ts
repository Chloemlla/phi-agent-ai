import { afterEach, describe, expect, test } from "bun:test";
import { render } from "solid-js/web";
import { Markdown } from "../src/components/Markdown";
import { RichHtml } from "../src/components/RichHtml";

let dispose: (() => void) | undefined;
let container: HTMLDivElement | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  container?.remove();
  container = undefined;
});

/** Mount a renderer root into the happy-dom document. */
function mount(build: () => unknown): HTMLDivElement {
  const host = document.createElement("div");
  document.body.appendChild(host);
  container = host;
  dispose = render(build as () => never, host);
  return host;
}

describe("RichHtml sanitizer", () => {
  test("drops executable elements, event handlers and unsafe links", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          "<script>window.__pwned = 1</script>",
          "<style>body { display: none }</style>",
          '<iframe srcdoc="inline"></iframe>',
          '<img src="x" onerror="window.__pwned = 2">',
          '<img src="https://example.com/pixel.png" onerror="window.__pwned = 3" alt="pixel">',
          '<p onclick="window.__pwned = 4">safe text <strong>strong text</strong></p>',
          '<a href="javascript:alert(1)" onclick="window.__pwned = 5">bad link</a>',
          '<a href="https://example.com">good link</a>',
        ].join(""),
      }),
    );

    expect(host.querySelector("script, style, iframe")).toBeNull();
    expect(host.querySelector('img[src="x"]')).toBeNull();

    const images = Array.from(host.querySelectorAll("img"));
    expect(images).toHaveLength(1);
    expect(images[0].getAttribute("src")).toBe("https://example.com/pixel.png");
    expect(images[0].hasAttribute("onerror")).toBe(false);

    for (const element of Array.from(host.querySelectorAll("*"))) {
      for (const attribute of Array.from(element.attributes)) {
        expect(attribute.name.startsWith("on")).toBe(false);
      }
    }

    const unsafeLink = Array.from(host.querySelectorAll("a")).find(
      (link) => link.textContent === "bad link",
    );
    expect(unsafeLink).toBeDefined();
    expect(unsafeLink?.hasAttribute("href")).toBe(false);

    const safeLink = host.querySelector('a[href="https://example.com"]');
    expect(safeLink).not.toBeNull();
    expect(safeLink?.textContent).toBe("good link");

    expect(host.textContent).toContain("safe text");
    expect(host.querySelector("strong")?.textContent).toBe("strong text");
  });

  test("keeps allowlisted host classes and drops arbitrary tokens", () => {
    const host = mount(() =>
      RichHtml({
        html: '<div class="ui-card evil"><span class="ui-muted other">x</span></div>',
      }),
    );

    const card = host.querySelector("div.ui-card");
    expect(card).not.toBeNull();
    expect(card?.className).toBe("ui-card");

    const muted = host.querySelector("span");
    expect(muted?.className).toBe("ui-muted");
  });

  test("unwraps unknown wrappers but keeps their content", () => {
    const host = mount(() =>
      RichHtml({
        html: '<my-widget data-evil="x"><span>inside</span></my-widget>',
      }),
    );

    expect(host.querySelector("my-widget")).toBeNull();
    expect(host.querySelector("span")?.textContent).toBe("inside");
    expect(host.textContent).toContain("inside");
  });

  test("a send button invokes onSend with its label", () => {
    const sent: string[] = [];
    const host = mount(() =>
      RichHtml({
        html: "<button data-ui-send>Send report</button>",
        onSend: (text) => sent.push(text),
      }),
    );

    const button = host.querySelector("button");
    expect(button).not.toBeNull();
    expect(button?.disabled).toBe(false);
    button?.click();
    expect(sent).toEqual(["Send report"]);
  });

  test("a send button is disabled when onSend is absent", () => {
    const host = mount(() =>
      RichHtml({ html: "<button data-ui-send>Send report</button>" }),
    );

    expect(host.querySelector("button")?.disabled).toBe(true);
  });
});

describe("Markdown renderer", () => {
  test("renders fenced code inside pre/code", () => {
    const host = mount(() => Markdown({ text: "```js\nconst x = 1;\n```" }));

    const code = host.querySelector("pre code");
    expect(code).not.toBeNull();
    expect(code?.textContent).toBe("const x = 1;");
  });

  test("does not render a javascript: link as an href", () => {
    const host = mount(() =>
      Markdown({ text: "[click](javascript:alert(1)) and **bold**" }),
    );

    expect(host.querySelectorAll("a")).toHaveLength(0);
    expect(host.querySelector("a[href]")).toBeNull();
    expect(host.querySelector("strong")?.textContent).toBe("bold");
  });

  test("drops raw script elements in a top-level HTML block", () => {
    const host = mount(() =>
      Markdown({
        text: [
          "<div>",
          "<script>window.__pwned = true</script>",
          "<p>safe block</p>",
          "</div>",
        ].join("\n"),
      }),
    );

    expect(host.querySelector("script")).toBeNull();
    expect(host.textContent).toContain("safe block");
  });

  test("renders an html fence through RichHtml", () => {
    const sent: string[] = [];
    const host = mount(() =>
      Markdown({
        text: '```html\n<button data-ui-send="go">Go</button>\n```',
        onSend: (text) => sent.push(text),
      }),
    );

    const button = host.querySelector("button");
    expect(button).not.toBeNull();
    expect(button?.disabled).toBe(false);
    button?.click();
    expect(sent).toEqual(["go"]);
  });
});
