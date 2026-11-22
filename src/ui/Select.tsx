import ChevronDown from "lucide-solid/icons/chevron-down";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { Portal } from "solid-js/web";
import { IconCheck } from "../components/Icons";
import "./select.css";

export interface SelectOption {
  readonly value: string;
  readonly label: string;
}

interface SelectProps {
  value: string;
  options: readonly SelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  "aria-label"?: string;
  title?: string;
  class?: string;
  /** Menu width in px; defaults to the trigger width. */
  menuWidth?: number;
}

const OPEN_KEYS = new Set(["Enter", " ", "ArrowDown", "ArrowUp"]);
const LIST_GAP = 4;
const VIEWPORT_MARGIN = 8;
const ESTIMATED_LIST_HEIGHT = 220;

interface Coords {
  top: number;
  left: number;
  width: number;
  above: boolean;
}

/**
 * A custom listbox. Native selects cannot follow the app's design tokens and
 * render differently per platform, so the trigger and list are drawn here.
 * Opening happens on pointerdown, matching the hand: the menu appears on press,
 * not on release.
 */
export function Select(props: SelectProps) {
  const [open, setOpen] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  const [coords, setCoords] = createSignal<Coords | null>(null);
  let triggerRef: HTMLButtonElement | undefined;
  let listRef: HTMLDivElement | undefined;
  let openedByKeyboard = false;

  const selectedLabel = () =>
    props.options.find((option) => option.value === props.value)?.label ?? props.value;

  const menuWidth = () => {
    const wanted = props.menuWidth ?? triggerRef?.getBoundingClientRect().width ?? 220;
    return Math.min(wanted, window.innerWidth - VIEWPORT_MARGIN * 2);
  };

  function computeCoords(listHeight = ESTIMATED_LIST_HEIGHT): Coords {
    const rect = triggerRef?.getBoundingClientRect();
    if (!rect) return { top: 0, left: 0, width: menuWidth(), above: false };
    const width = menuWidth();
    const spaceBelow = window.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    const above =
      spaceBelow < Math.min(listHeight, ESTIMATED_LIST_HEIGHT) && spaceAbove > spaceBelow;
    const top = above ? rect.top - listHeight - LIST_GAP : rect.bottom + LIST_GAP;
    let left = rect.left;
    if (left + width > window.innerWidth - VIEWPORT_MARGIN) {
      left = Math.max(VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN);
    }
    return { top: Math.max(VIEWPORT_MARGIN, top), left, width, above };
  }

  function position() {
    setCoords(computeCoords());
  }

  function refine() {
    const height = listRef?.getBoundingClientRect().height ?? ESTIMATED_LIST_HEIGHT;
    setCoords(computeCoords(height));
  }

  const optionButtons = (): HTMLButtonElement[] =>
    listRef ? Array.from(listRef.querySelectorAll<HTMLButtonElement>(".ui-select-option")) : [];

  function focusOption(value: string | null) {
    const buttons = optionButtons();
    if (buttons.length === 0) return;
    const index = value ? props.options.findIndex((option) => option.value === value) : -1;
    buttons[index < 0 ? 0 : index]?.focus();
  }

  function openList(viaKeyboard = false) {
    if (props.disabled) return;
    openedByKeyboard = viaKeyboard;
    setReady(false);
    position();
    setOpen(true);
  }

  function closeList(returnFocus = false) {
    if (returnFocus) queueMicrotask(() => triggerRef?.focus());
    setReady(false);
    setOpen(false);
    setCoords(null);
  }

  function choose(value: string) {
    props.onChange(value);
    closeList(true);
  }

  function onTriggerPointerDown(event: PointerEvent) {
    if (event.button !== 0 || props.disabled) return;
    event.preventDefault();
    if (open()) closeList();
    else openList();
  }

  function onTriggerKeyDown(event: KeyboardEvent) {
    if (!OPEN_KEYS.has(event.key)) return;
    event.preventDefault();
    if (!open()) openList(true);
  }

  function onListKeyDown(event: KeyboardEvent) {
    const buttons = optionButtons();
    if (buttons.length === 0) return;
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        buttons[(index + 1 + buttons.length) % buttons.length]?.focus();
        break;
      case "ArrowUp":
        event.preventDefault();
        buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
        break;
      case "Home":
        event.preventDefault();
        buttons[0]?.focus();
        break;
      case "End":
        event.preventDefault();
        buttons[buttons.length - 1]?.focus();
        break;
      case "Tab":
        event.preventDefault();
        closeList(true);
        break;
    }
  }

  createEffect(() => {
    if (!open()) return;
    requestAnimationFrame(() => {
      refine();
      requestAnimationFrame(() => setReady(true));
      if (openedByKeyboard) {
        const selected = props.options.findIndex((option) => option.value === props.value);
        focusOption(selected < 0 ? null : (props.options[selected]?.value ?? null));
      }
    });

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef?.contains(target) || listRef?.contains(target)) return;
      closeList();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      closeList(true);
    };
    const onScroll = (event: Event) => {
      if (event.target instanceof Node && listRef?.contains(event.target)) return;
      position();
      requestAnimationFrame(refine);
    };
    const onResize = () => closeList();

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onResize);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onResize);
    });
  });

  createEffect(() => {
    if (props.disabled && open()) closeList();
  });

  return (
    <div class={`ui-select ${props.class ?? ""}`} classList={{ "ui-select--open": open() }}>
      <button
        ref={triggerRef}
        type="button"
        class="ui-select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-label={props["aria-label"]}
        title={props.title}
        disabled={props.disabled}
        onPointerDown={onTriggerPointerDown}
        onKeyDown={onTriggerKeyDown}
      >
        <span class="ui-select-value">{selectedLabel()}</span>
        <ChevronDown
          classList={{ "ui-select-chevron": true, "ui-select-chevron--open": open() }}
          size={14}
          aria-hidden="true"
        />
      </button>
      <Show when={open() && coords()}>
        {(box) => (
          <Portal>
            <div
              ref={listRef}
              class="ui-select-list"
              data-ready={ready() ? "true" : undefined}
              role="listbox"
              tabIndex={-1}
              aria-label={props["aria-label"]}
              onKeyDown={onListKeyDown}
              style={{
                top: `${box().top}px`,
                left: `${box().left}px`,
                width: `${box().width}px`,
              }}
            >
              <For each={props.options}>
                {(option) => (
                  <button
                    type="button"
                    role="option"
                    aria-selected={option.value === props.value}
                    classList={{
                      "ui-select-option": true,
                      "ui-select-option--selected": option.value === props.value,
                    }}
                    onKeyDown={(event) => {
                      if (event.key !== "Enter" && event.key !== " ") return;
                      event.preventDefault();
                      choose(option.value);
                    }}
                    onPointerDown={(event) => {
                      if (event.button !== 0) return;
                      event.preventDefault();
                      event.stopPropagation();
                      choose(option.value);
                    }}
                  >
                    <span class="ui-select-option-label">{option.label}</span>
                    <Show when={option.value === props.value}>
                      <IconCheck size={15} class="ui-select-check" aria-hidden="true" />
                    </Show>
                  </button>
                )}
              </For>
            </div>
          </Portal>
        )}
      </Show>
    </div>
  );
}
