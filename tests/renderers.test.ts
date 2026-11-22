import { afterEach, describe, expect, test } from "bun:test";
import { render } from "solid-js/web";
import { Markdown } from "../src/components/Markdown";
import { RichHtml } from "../src/components/RichHtml";
import { sanitizeRichHtml } from "../src/components/RichSandbox";

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

  test("keeps tokenized layout attributes and drops unknown values", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<div class="ui-row" data-gap="3" data-justify="between" data-align="center" data-wrap="nowrap">',
          '<div class="ui-card" data-flex="2" data-pad="4" data-bg="soft" data-radius="lg" data-border>card</div>',
          '<div class="ui-card" data-gap="99" data-tone="chart9">other</div>',
          "</div>",
        ].join(""),
      }),
    );

    const row = host.querySelector(".ui-row");
    expect(row?.getAttribute("data-gap")).toBe("3");
    expect(row?.getAttribute("data-justify")).toBe("between");
    expect(row?.getAttribute("data-align")).toBe("center");
    expect(row?.getAttribute("data-wrap")).toBe("nowrap");

    const cards = Array.from(host.querySelectorAll(".ui-card"));
    expect(cards[0]?.getAttribute("data-flex")).toBe("2");
    expect(cards[0]?.getAttribute("data-pad")).toBe("4");
    expect(cards[0]?.getAttribute("data-bg")).toBe("soft");
    expect(cards[0]?.getAttribute("data-radius")).toBe("lg");
    expect(cards[0]?.hasAttribute("data-border")).toBe(true);
    expect(cards[1]?.hasAttribute("data-gap")).toBe(false);
    expect(cards[1]?.hasAttribute("data-tone")).toBe(false);
  });

  test("resolves SVG token colors and evaluates attribute expressions", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<script type="application/json" data-ui-state>{"n": 3}</script>',
          '<svg viewBox="0 0 10 10" role="img" aria-label="diagram">',
          '<circle cx="{{ n }}" cy="5" r="2" fill="info" stroke="faint"></circle>',
          "</svg>",
        ].join(""),
      }),
    );

    const circle = host.querySelector("circle");
    expect(circle?.getAttribute("cx")).toBe("3");
    expect(circle?.getAttribute("fill")).toBe("var(--accent)");
    expect(circle?.getAttribute("stroke")).toBe("var(--text-muted)");
  });

  test("a range binding updates SVG attributes and computed text", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<script type="application/json" data-ui-state>{"gear": 3, "teeth": [28, 24, 21, 18]}</script>',
          '<input type="range" data-ui-bind="gear" min="1" max="4" step="1">',
          '<svg viewBox="0 0 10 10"><circle cx="{{ 10 + gear }}" cy="5" r="2" fill="info"></circle></svg>',
          '<p class="ui-muted">{{ teeth[gear - 1] }} teeth</p>',
        ].join(""),
      }),
    );

    const input = host.querySelector("input");
    expect(input?.value).toBe("3");
    expect(host.querySelector("circle")?.getAttribute("cx")).toBe("13");
    expect(host.querySelector("p")?.textContent).toBe("21 teeth");

    if (input) {
      input.value = "1";
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }

    expect(host.querySelector("circle")?.getAttribute("cx")).toBe("11");
    expect(host.querySelector("p")?.textContent).toBe("28 teeth");
  });

  test("data-ui-show hides SVG groups conditionally", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<script type="application/json" data-ui-state>{"part": "frame"}</script>',
          '<svg viewBox="0 0 10 10">',
          '<g data-ui-show="part == \'frame\'"><circle cx="3" cy="5" r="2"></circle></g>',
          '<g data-ui-show="part == \'wheel\'"><circle cx="7" cy="5" r="2"></circle></g>',
          "</svg>",
        ].join(""),
      }),
    );

    const groups = Array.from(host.querySelectorAll("g"));
    expect(groups[0]?.style.display).not.toBe("none");
    expect(groups[1]?.style.display).toBe("none");
  });

  test("keeps carousel classes and tokenized item widths", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<div class="ui-carousel" data-item="md">',
          '<figure class="ui-figure"><img src="https://example.com/a.png" alt="A"></figure>',
          '<figure class="ui-figure"><img src="https://example.com/b.png" alt="B"></figure>',
          '<figure class="ui-figure" data-item="huge"><img src="https://example.com/c.png" alt="C"></figure>',
          "</div>",
        ].join(""),
      }),
    );

    const carousel = host.querySelector(".ui-carousel");
    expect(carousel?.getAttribute("data-item")).toBe("md");
    expect(carousel?.querySelectorAll("figure.ui-figure")).toHaveLength(3);
    const figures = Array.from(host.querySelectorAll("figure"));
    expect(figures[2]?.hasAttribute("data-item")).toBe(false);
  });

  test("keeps aria-label on generated controls", () => {
    const host = mount(() =>
      RichHtml({
        html: '<input type="range" data-ui-bind="gear" min="1" max="7" aria-label="Gear">',
      }),
    );

    expect(host.querySelector("input")?.getAttribute("aria-label")).toBe("Gear");
  });

  test("renders host icons by name and falls back for unknown names", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<span class="ui-icon" data-icon="bike" data-size="lg" data-tone="accent"></span>',
          '<span class="ui-icon" data-icon="check" aria-label="Done"></span>',
          '<span class="ui-icon" data-icon="not-a-real-icon">fallback</span>',
        ].join(""),
      }),
    );

    const icons = Array.from(host.querySelectorAll("span.ui-icon"));
    expect(icons).toHaveLength(2);
    expect(icons[0]?.querySelector("svg")).not.toBeNull();
    expect(icons[0]?.getAttribute("data-tone")).toBe("accent");
    expect(icons[0]?.getAttribute("aria-hidden")).toBe("true");
    expect(icons[0]?.querySelector("svg")?.getAttribute("width")).toBe("20");
    expect(icons[1]?.getAttribute("role")).toBe("img");
    expect(icons[1]?.getAttribute("aria-label")).toBe("Done");
    expect(host.textContent).toContain("fallback");
  });

  test("renders a figure with an image and caption", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          '<figure class="ui-figure">',
          '<img src="https://example.com/part.png" alt="Derailleur" data-fit="contain" data-radius="md">',
          '<figcaption class="ui-caption">The rear derailleur</figcaption>',
          "</figure>",
        ].join(""),
      }),
    );

    const image = host.querySelector("figure.ui-figure img");
    expect(image?.getAttribute("src")).toBe("https://example.com/part.png");
    expect(image?.getAttribute("data-fit")).toBe("contain");
    expect(image?.getAttribute("data-radius")).toBe("md");
    expect(host.querySelector("figcaption.ui-caption")?.textContent).toContain("derailleur");
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
    const host = mount(() => RichHtml({ html: "<button data-ui-send>Send report</button>" }));

    expect(host.querySelector("button")?.disabled).toBe(true);
  });
});

describe("RichSandbox", () => {
  test("sanitizes active content but keeps CSS and markup", () => {
    const clean = sanitizeRichHtml(
      [
        "<script>window.__pwned = 1</script>",
        '<meta http-equiv="refresh" content="0;url=https://example.com">',
        '<div class="hero" style="background: linear-gradient(135deg, #fee2e2, #fff)" onclick="alert(1)">',
        '<a href="javascript:alert(1)">bad</a>',
        "<strong>kept</strong>",
        "</div>",
      ].join(""),
    );

    expect(clean).not.toContain("<script");
    expect(clean).not.toContain("<meta");
    expect(clean).not.toContain("onclick");
    expect(clean).not.toContain("javascript:");
    expect(clean).toContain("linear-gradient");
    expect(clean).toContain("<strong>kept</strong>");
    expect(sanitizeRichHtml("x".repeat(100_001))).toBe("");
  });

  test("renders a template data-ui-rich block in a script-disabled iframe", () => {
    const host = mount(() =>
      RichHtml({
        html: [
          "<template data-ui-rich>",
          "<style>.hero { background: linear-gradient(135deg, #fee2e2, #fff); }</style>",
          '<div class="hero"><script>window.__pwned = 2</script><strong>Rich block</strong></div>',
          "</template>",
        ].join(""),
      }),
    );

    const frame = host.querySelector("iframe.inline-ui-rich");
    expect(frame).not.toBeNull();
    expect(frame?.getAttribute("sandbox")).toBe("allow-same-origin");
    expect(host.querySelector(".inline-ui-rich-bar")?.textContent).toContain("Generated interface");
    const srcdoc = frame?.getAttribute("srcdoc") ?? "";
    expect(srcdoc).toContain("Rich block");
    expect(srcdoc).toContain("linear-gradient");
    expect(srcdoc).not.toContain("<script");
    expect(srcdoc).toContain("default-src 'none'");
    expect(srcdoc).toContain("style-src 'unsafe-inline'");
    expect(srcdoc).not.toContain("allow-scripts");
    expect(host.querySelector("template")).toBeNull();
    expect(document.querySelector("iframe.inline-ui-rich")).not.toBeNull();
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
    const host = mount(() => Markdown({ text: "[click](javascript:alert(1)) and **bold**" }));

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

  test("renders a top-level inline HTML block instead of leaking source", () => {
    const host = mount(() =>
      Markdown({
        text: '<span style="color:red">标签</span> <span>渲染</span>\n\nAfter',
      }),
    );

    const spans = Array.from(host.querySelectorAll("span"));
    expect(spans.map((span) => span.textContent)).toEqual(["标签", "渲染"]);
    expect(spans[0]?.hasAttribute("style")).toBe(false);
    expect(host.textContent).not.toContain("<span");
    expect(host.textContent).toContain("After");
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
