// Form message extraction. Values come from the live DOM after submit, not
// from the parsed template, and each control is reduced to a readable line.

import { cap, cleanText } from "./attributes";

/** Turn the submitted controls into a readable message for the agent. */
export function formMessage(form: HTMLFormElement): string {
  const lines: string[] = [];
  for (const control of Array.from(form.elements)) {
    const line = controlLine(control);
    if (line) lines.push(line);
  }
  return lines.join("\n");
}

function controlLine(control: Element): string | null {
  const tag = control.tagName.toLowerCase();
  if (tag !== "input" && tag !== "select" && tag !== "textarea") return null;
  if (control.hasAttribute("disabled")) return null;
  const name = control.getAttribute("name");
  if (!name) return null;

  if (tag === "input") {
    const input = control as HTMLInputElement;
    const type = (input.type || "text").toLowerCase();
    if (type === "submit" || type === "button" || type === "reset") return null;
    if (type === "checkbox" || type === "radio") {
      if (!input.checked) return null;
      const label = groupLabel(control, name);
      const value = cleanText(input.value) || "yes";
      return `${label}: ${value === "on" ? "yes" : value}`;
    }
    const value = cleanText(input.value);
    return value ? `${fieldLabel(control, name)}: ${value}` : null;
  }

  if (tag === "select") {
    const select = control as HTMLSelectElement;
    const values = Array.from(select.selectedOptions)
      .map((option) => cleanText(option.value || option.textContent))
      .filter(Boolean);
    return values.length > 0
      ? `${fieldLabel(control, name)}: ${values.join(", ")}`
      : null;
  }

  const value = cleanText((control as HTMLTextAreaElement).value);
  return value ? `${fieldLabel(control, name)}: ${value}` : null;
}

function fieldLabel(control: Element, name: string): string {
  const label = control.closest("label");
  if (label) {
    const clone = label.cloneNode(true) as Element;
    clone
      .querySelectorAll("input, select, textarea, button, output, progress, meter")
      .forEach((node) => node.remove());
    const text = cleanText(clone.textContent);
    if (text) return cap(text, 60);
  }
  return cap(name, 60);
}

/** Radio and checkbox groups read better with the fieldset legend than with
 * the individual option's label. */
function groupLabel(control: Element, name: string): string {
  const fieldset = control.closest("fieldset");
  const legend = fieldset?.querySelector(":scope > legend");
  const text = cleanText(legend?.textContent);
  return text ? cap(text, 60) : fieldLabel(control, name);
}
