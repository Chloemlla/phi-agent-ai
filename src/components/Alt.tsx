// Rich hover/focus bubble. The bubble is portal-rendered and follows the
// pointer, preferring the cursor's top-right corner. Near a viewport edge it
// flips using the measured bubble size, so rows at the bottom of a list keep
// tracking the pointer instead of snapping to a fixed clamp height.
//
// `clickToOpen` turns the bubble into a pinnable panel: a click anchors it to
// the trigger, keeps it interactive, and a second click, an outside press or
// Escape dismisses it. Hover previews still work while unpinned.

import { createEffect, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

interface AltProps {
  /** Bubble content shown on hover, focus or click. */
  content: JSX.Element;
  /** Horizontal offset from the anchor point. */
  offsetX?: number;
  /** Vertical offset from the anchor point (negative is upward). */
  offsetY?: number;
  /** Hover delay in ms before showing the bubble. */
  delay?: number;
  /** Optional class on the bubble wrapper. */
  class?: string;
  /** Keep the bubble interactive so buttons inside it can be clicked. */
  interactive?: boolean;
  /** Toggle a pinned, interactive bubble when the trigger is clicked. */
  clickToOpen?: boolean;
  /** The element that triggers the bubble on hover/focus. */
  children: JSX.Element;
}

export function Alt(props: AltProps) {
  const [open, setOpen] = createSignal(false);
  const [pinned, setPinned] = createSignal(false);
  const [coords, setCoords] = createSignal<{ top: number; left: number } | null>(null);
  let triggerRef: HTMLSpanElement | undefined;
  let bubbleRef: HTMLDivElement | undefined;
  const offsetX = () => props.offsetX ?? 10;
  const offsetY = () => props.offsetY ?? -10;
  const delay = () => props.delay ?? 80;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let pointer = { x: 0, y: 0 };
  let usingPointer = false;

  function clearTimers() {
    if (timer) clearTimeout(timer);
    if (hideTimer) clearTimeout(hideTimer);
    timer = undefined;
    hideTimer = undefined;
  }

  function placeAt(x: number, y: number) {
    const margin = 6;
    // Prefer the measured size once painted; fall back to estimates.
    const size = bubbleRef?.getBoundingClientRect();
    const width = size?.width && size.width > 0 ? size.width : 300;
    const height = size?.height && size.height > 0 ? size.height : 160;

    let left = x + offsetX();
    let top = y + offsetY() - height;

    // If the top-right placement would clip the right edge, flip left.
    if (left + width > window.innerWidth - margin) {
      left = x - width - 6;
    }
    // If it would clip the top edge, flip below the anchor.
    if (top < margin) {
      top = y + 14;
    }
    // Final clamp — keep following the pointer, never pin to a fixed band.
    if (left < margin) left = margin;
    if (left + width > window.innerWidth - margin) {
      left = Math.max(margin, window.innerWidth - width - margin);
    }
    if (top < margin) top = margin;
    if (top + height > window.innerHeight - margin) {
      top = Math.max(margin, window.innerHeight - height - margin);
    }
    setCoords({ top, left });
  }

  const placeAtPointer = () => placeAt(pointer.x, pointer.y);

  function placeAtTrigger() {
    const trigger = triggerRef;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    placeAt(rect.left + 20, rect.top + 8);
  }

  /** Pinned bubbles anchor to the trigger; hover bubbles follow the pointer. */
  function position() {
    if (pinned() || !usingPointer) placeAtTrigger();
    else placeAtPointer();
  }

  function show() {
    if (timer) clearTimeout(timer);
    if (hideTimer) clearTimeout(hideTimer);
    timer = setTimeout(() => {
      if (pinned()) {
        placeAtTrigger();
        setOpen(true);
        return;
      }
      if (usingPointer) placeAtPointer();
      else placeAtTrigger();
      setOpen(true);
      // Re-measure after the first paint so the real bubble size drives
      // placement on the next frame.
      requestAnimationFrame(() => {
        if (open()) position();
      });
    }, delay());
  }

  function close() {
    clearTimers();
    setPinned(false);
    usingPointer = false;
    setOpen(false);
  }

  function hide() {
    if (pinned()) return;
    if (timer) {
      clearTimeout(timer);
      timer = undefined;
    }
    if (props.interactive) {
      if (hideTimer) clearTimeout(hideTimer);
      hideTimer = setTimeout(() => {
        if (pinned()) return;
        const triggerHovered = triggerRef?.matches(":hover") ?? false;
        const bubbleHovered = bubbleRef?.matches(":hover") ?? false;
        if (!triggerHovered && !bubbleHovered) close();
      }, 120);
      return;
    }
    close();
  }

  const onPointerMove = (event: PointerEvent) => {
    usingPointer = true;
    pointer = { x: event.clientX, y: event.clientY };
    if (open() && !pinned()) placeAtPointer();
  };

  const onPointerEnter = (event: PointerEvent) => {
    if (pinned()) return;
    usingPointer = true;
    pointer = { x: event.clientX, y: event.clientY };
    if (open()) placeAtPointer();
    else show();
  };

  const onPointerLeave = () => {
    if (pinned()) return;
    hide();
  };

  const onFocusIn = () => {
    if (pinned()) return;
    usingPointer = false;
    if (!open()) show();
    else placeAtTrigger();
  };

  const onFocusOut = (event: FocusEvent) => {
    if (pinned()) return;
    const next = event.relatedTarget;
    if (next instanceof Node && (triggerRef?.contains(next) || bubbleRef?.contains(next))) {
      return;
    }
    hide();
  };

  const onTriggerClick = () => {
    if (!props.clickToOpen) return;
    if (pinned()) {
      close();
      return;
    }
    clearTimers();
    usingPointer = false;
    setPinned(true);
    placeAtTrigger();
    setOpen(true);
    requestAnimationFrame(() => {
      if (pinned()) placeAtTrigger();
    });
  };

  onMount(() => {
    const element = triggerRef;
    if (!element) return;
    element.addEventListener("pointerenter", onPointerEnter);
    element.addEventListener("pointerleave", onPointerLeave);
    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("focusin", onFocusIn);
    element.addEventListener("focusout", onFocusOut);
    onCleanup(() => {
      element.removeEventListener("pointerenter", onPointerEnter);
      element.removeEventListener("pointerleave", onPointerLeave);
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("focusin", onFocusIn);
      element.removeEventListener("focusout", onFocusOut);
    });
  });

  // A pinned panel dismisses on an outside press without disturbing the click
  // that opened it.
  createEffect(() => {
    if (!pinned()) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef?.contains(target) || bubbleRef?.contains(target)) return;
      close();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    onCleanup(() => document.removeEventListener("pointerdown", onPointerDown, true));
  });

  createEffect(() => {
    if (!open()) return;
    const onScrollOrResize = () => position();
    // WCAG 1.4.13: hover/focus content must be dismissible without moving away.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      close();
    };
    window.addEventListener("scroll", onScrollOrResize, true);
    window.addEventListener("resize", onScrollOrResize);
    document.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      window.removeEventListener("scroll", onScrollOrResize, true);
      window.removeEventListener("resize", onScrollOrResize);
      document.removeEventListener("keydown", onKeyDown);
    });
  });

  onCleanup(() => {
    clearTimers();
  });

  // Only clickable Alt instances (the context ring) attach a click handler.
  // The interactive element is the child button; the wrapper merely observes
  // its bubbled click so pointer and keyboard activation behave identically.
  const clickProps = () => (props.clickToOpen ? { onClick: onTriggerClick } : {});

  return (
    <span class="alt-trigger" ref={triggerRef} {...clickProps()}>
      {props.children}
      <Show when={open() && coords()} keyed>
        {(box) => (
          <Portal>
            <div
              ref={bubbleRef}
              class={`alt-bubble ${props.class ?? ""}`}
              classList={{ "alt-bubble--interactive": !!props.interactive || pinned() }}
              style={{ position: "fixed", top: `${box.top}px`, left: `${box.left}px` }}
              role="tooltip"
              onPointerEnter={() => {
                if (hideTimer) clearTimeout(hideTimer);
              }}
              onPointerLeave={hide}
              onFocusIn={() => {
                if (hideTimer) clearTimeout(hideTimer);
              }}
              onFocusOut={hide}
            >
              {props.content}
            </div>
          </Portal>
        )}
      </Show>
    </span>
  );
}
