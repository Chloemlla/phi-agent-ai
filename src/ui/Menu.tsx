import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import type { JSX } from "solid-js";
import "./menu.css";

export interface FloatingMenuItem {
  label: string;
  icon?: JSX.Element;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

export interface FloatingMenuHeader {
  title: string;
  subtitle?: string;
  icon?: JSX.Element;
}

interface FloatingMenuProps {
  anchor: HTMLElement;
  label: string;
  header?: FloatingMenuHeader;
  items: FloatingMenuItem[];
  onClose: () => void;
}

const MENU_WIDTH = 224;
const MARGIN = 8;
const GAP = 4;
const ROW_HEIGHT = 38;
const HEADER_HEIGHT = 58;

/**
 * A small anchored action menu styled like GitHub's account menu: a quiet
 * header, a separator, then full-width rows with icons. Positioned before it
 * becomes visible so it never flashes at the wrong spot. Dismissed by Escape,
 * an outside press, or a viewport change; focus returns to the anchor.
 */
export function FloatingMenu(props: FloatingMenuProps) {
  const [ready, setReady] = createSignal(false);
  const [coords, setCoords] = createSignal<{ top: number; left: number } | null>(null);
  let menuRef: HTMLDivElement | undefined;

  function place() {
    const rect = props.anchor.getBoundingClientRect();
    const estimated =
      (props.header ? HEADER_HEIGHT : 0) +
      props.items.length * ROW_HEIGHT +
      12 +
      (props.items.length > 1 ? 8 : 0);
    const height = menuRef?.getBoundingClientRect().height ?? estimated;
    const spaceBelow = window.innerHeight - rect.bottom;
    const top =
      spaceBelow < height + MARGIN && rect.top > spaceBelow
        ? Math.max(MARGIN, rect.top - height - GAP)
        : rect.bottom + GAP;
    const left = Math.max(
      MARGIN,
      Math.min(rect.right - MENU_WIDTH, window.innerWidth - MENU_WIDTH - MARGIN),
    );
    setCoords({ top, left });
  }

  function close(returnFocus = true) {
    props.onClose();
    if (returnFocus) {
      queueMicrotask(() => props.anchor.focus());
    }
  }

  onMount(() => {
    place();
    requestAnimationFrame(() => {
      place();
      setReady(true);
      menuRef?.querySelector<HTMLButtonElement>(".ui-menu-item:not(:disabled)")?.focus();
    });

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (menuRef?.contains(target) || props.anchor.contains(target)) return;
      close(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        close(true);
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const items = Array.from(
        menuRef?.querySelectorAll<HTMLButtonElement>(".ui-menu-item:not(:disabled)") ?? [],
      );
      if (items.length === 0) return;
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const delta = event.key === "ArrowDown" ? 1 : -1;
      items[(index + delta + items.length) % items.length]?.focus();
    };
    const onViewportChange = () => close(false);
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("resize", onViewportChange);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("resize", onViewportChange);
    });
  });

  return (
    <Show when={coords()}>
      {(box) => (
        <Portal>
          <div
            ref={menuRef}
            class="ui-menu"
            data-ready={ready() ? "true" : undefined}
            role="menu"
            tabIndex={-1}
            aria-label={props.label}
            style={{ top: `${box().top}px`, left: `${box().left}px` }}
            onKeyDown={(event) => {
              if (event.key !== "Tab") return;
              event.preventDefault();
              close(true);
            }}
          >
            <Show when={props.header}>
              {(header) => (
                <>
                  <div class="ui-menu-header">
                    <Show when={header().icon}>
                      <span class="ui-menu-avatar" aria-hidden="true">
                        {header().icon}
                      </span>
                    </Show>
                    <span class="ui-menu-header-copy">
                      <span class="ui-menu-title">{header().title}</span>
                      <Show when={header().subtitle}>
                        <span class="ui-menu-subtitle">{header().subtitle}</span>
                      </Show>
                    </span>
                  </div>
                  <div class="ui-menu-separator" role="separator" />
                </>
              )}
            </Show>
            <For each={props.items}>
              {(item, index) => (
                <>
                  <Show when={item.danger && index() > 0}>
                    <div class="ui-menu-separator" role="separator" />
                  </Show>
                  <button
                    type="button"
                    role="menuitem"
                    class="ui-menu-item"
                    classList={{ "ui-menu-item--danger": !!item.danger }}
                    disabled={item.disabled}
                    onClick={() => {
                      close(false);
                      item.onSelect();
                    }}
                  >
                    <Show when={item.icon}>
                      <span class="ui-menu-icon" aria-hidden="true">
                        {item.icon}
                      </span>
                    </Show>
                    <span class="ui-menu-label">{item.label}</span>
                  </button>
                </>
              )}
            </For>
          </div>
        </Portal>
      )}
    </Show>
  );
}
