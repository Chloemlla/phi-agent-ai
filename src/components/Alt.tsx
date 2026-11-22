// Rich hover/focus bubble. The bubble is portal-rendered and anchored to the
// trigger, not the pointer: a tooltip that chases the cursor flickers and
// teleports near viewport edges. The first paint stays invisible until the
// measured size has been applied, so the bubble never jumps into place.
//
// `clickToOpen` turns the bubble into a pinnable panel: a click anchors it to
// the trigger, keeps it interactive, and a second click, an outside press or
// Escape dismisses it. Hover previews still work while unpinned.

import { createEffect, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

interface AltProps {
  /** Bubble content shown on hover, focus or click. */
  content: JSX.Element;
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

const GAP = 10;
const MARGIN = 8;
const ESTIMATED_WIDTH = 300;
const ESTIMATED_HEIGHT = 160;

export function Alt(props: AltProps) {
  const [open, setOpen] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  const [pinned, setPinned] = createSignal(false);
  const [coords, setCoords] = createSignal<{ top: number; left: number } | null>(null);
  let triggerRef: HTMLSpanElement | undefined;
  let bubbleRef: HTMLDivElement | undefined;
  const delay = () => props.delay ?? 80;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function clearTimers() {
    if (timer) clearTimeout(timer);
    if (hideTimer) clearTimeout(hideTimer);
    timer = undefined;
    hideTimer = undefined;
  }

  function placeAtTrigger() {
    const trigger = triggerRef;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const size = bubbleRef?.getBoundingClientRect();
    const width = size?.width && size.width > 0 ? size.width : ESTIMATED_WIDTH;
    const height = size?.height && size.height > 0 ? size.height : ESTIMATED_HEIGHT;

    let left = rect.right + GAP;
    if (left + width > window.innerWidth - MARGIN) {
      left = rect.left - width - GAP;
    }
    left = Math.max(MARGIN, Math.min(left, window.innerWidth - width - MARGIN));

    let top = rect.top;
    if (top + height > window.innerHeight - MARGIN) {
      top = window.innerHeight - height - MARGIN;
    }
    top = Math.max(MARGIN, top);

    setCoords({ top, left });
  }

  function reveal() {
    placeAtTrigger();
    if (open()) {
      setReady(true);
      return;
    }
    setOpen(true);
    setReady(false);
    // Measure the real bubble after it enters the DOM, then reveal it at the
    // final position on the next frame.
    requestAnimationFrame(() => {
      placeAtTrigger();
      requestAnimationFrame(() => setReady(true));
    });
  }

  function show() {
    if (timer) clearTimeout(timer);
    if (hideTimer) clearTimeout(hideTimer);
    timer = setTimeout(reveal, delay());
  }

  function close() {
    clearTimers();
    setPinned(false);
    setReady(false);
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

  const onPointerEnter = () => {
    if (pinned()) return;
    if (!open()) show();
    else clearTimers();
  };

  const onPointerLeave = () => {
    if (pinned()) return;
    hide();
  };

  const onFocusIn = () => {
    if (pinned()) return;
    if (!open()) show();
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
    setPinned(true);
    reveal();
  };

  onMount(() => {
    const element = triggerRef;
    if (!element) return;
    element.addEventListener("pointerenter", onPointerEnter);
    element.addEventListener("pointerleave", onPointerLeave);
    element.addEventListener("focusin", onFocusIn);
    element.addEventListener("focusout", onFocusOut);
    onCleanup(() => {
      element.removeEventListener("pointerenter", onPointerEnter);
      element.removeEventListener("pointerleave", onPointerLeave);
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
    const onScrollOrResize = () => placeAtTrigger();
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

  onCleanup(clearTimers);

  // Only clickable Alt instances (the context ring) attach a click handler.
  // The interactive element is the child button; the wrapper merely observes
  // its bubbled click so pointer and keyboard activation behave identically.
  const clickProps = () => (props.clickToOpen ? { onClick: onTriggerClick } : {});

  return (
    <span class="alt-trigger" ref={triggerRef} {...clickProps()}>
      {props.children}
      <Show when={open()}>
        <Portal>
          <div
            ref={bubbleRef}
            class={`alt-bubble ${props.class ?? ""}`}
            classList={{ "alt-bubble--interactive": !!props.interactive || pinned() }}
            data-ready={ready() ? "true" : undefined}
            style={{
              position: "fixed",
              top: `${coords()?.top ?? 0}px`,
              left: `${coords()?.left ?? 0}px`,
            }}
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
      </Show>
    </span>
  );
}
