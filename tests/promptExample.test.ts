import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { render } from "solid-js/web";
import { RichHtml } from "../src/components/RichHtml";

let dispose: (() => void) | undefined;
let container: HTMLDivElement | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  container?.remove();
  container = undefined;
});

function mount(build: () => unknown): HTMLDivElement {
  const host = document.createElement("div");
  document.body.appendChild(host);
  container = host;
  dispose = render(build as () => never, host);
  return host;
}

/** The worked examples are part of the model contract; keep them renderable. */
function exampleAfter(heading: string): string {
  const prompt = readFileSync("src-tauri/assets/inline-ui-system-prompt.txt", "utf8");
  const start = prompt.indexOf(heading);
  if (start === -1) throw new Error(`missing prompt section: ${heading}`);
  const block = prompt.slice(start);
  const fence = block.match(/```html\n([\s\S]*?)```/);
  if (!fence) throw new Error(`missing html example after: ${heading}`);
  return fence[1];
}

describe("system prompt examples", () => {
  test("the visual explanation example renders and reacts to its slider", () => {
    const host = mount(() => RichHtml({ html: exampleAfter("## Example: a visual explanation") }));

    expect(host.querySelectorAll("svg circle")).toHaveLength(7);
    expect(host.querySelectorAll("svg text")).toHaveLength(3);
    expect(host.querySelector("input[type=range]")?.value).toBe("3");
    expect(host.textContent).not.toContain("{{");
    expect(host.textContent).toContain("Gear ratio 1.9 : 1");

    const input = host.querySelector("input[type=range]");
    if (input) {
      input.value = "7";
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }

    expect(host.textContent).toContain("Gear ratio 3.33 : 1");
  });

  test("the rich visual block example renders in the sandbox", () => {
    const host = mount(() => RichHtml({ html: exampleAfter("## Rich visual blocks") }));

    const frame = host.querySelector("iframe.inline-ui-rich");
    expect(frame).not.toBeNull();
    expect(frame?.getAttribute("sandbox")).toBe("allow-same-origin");
    const srcdoc = frame?.getAttribute("srcdoc") ?? "";
    expect(srcdoc).toContain("Inside a 7-speed bike");
    expect(srcdoc).toContain("linear-gradient");
    expect(srcdoc).toContain("default-src 'none'");
  });
});
