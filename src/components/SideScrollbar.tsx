// Self-drawn, square, ultra-thin scrollbar overlay for a scroll container.
// The host stays the accessible scroll surface; this component only mirrors
// its scrollTop/scrollHeight into a 3px thumb. We paint our own thumb because
// platform overlay-scrollbar settings can make Chromium ignore the native
// ::-webkit-scrollbar chrome, and a native scrollbar also takes layout width.

import { createEffect, createSignal, on, onCleanup, Show } from "solid-js";

const THUMB_MIN = 24;

interface SideScrollbarProps {
  /** The scroll container to mirror. May start null until the ref resolves. */
  host: () => HTMLElement | null;
}

export function SideScrollbar(props: SideScrollbarProps) {
  const [visible, setVisible] = createSignal(false);
  const [thumbTop, setThumbTop] = createSignal(0);
  const [thumbH, setThumbH] = createSignal(0);
  const [hovering, setHovering] = createSignal(false);
  const [dragging, setDragging] = createSignal(false);

  let host: HTMLElement | null = null;
  let resize: ResizeObserver | null = null;
  let mut: MutationObserver | null = null;
  let frame: number | undefined;

  function update() {
    const element = host;
    if (!element) return;
    const overflow = element.scrollHeight - element.clientHeight;
    if (overflow <= 1) {
      setVisible(false);
      return;
    }
    const ratio = element.clientHeight / element.scrollHeight;
    const height = Math.max(THUMB_MIN, Math.round(element.clientHeight * ratio));
    const track = element.clientHeight - height;
    const top = Math.round(track * (element.scrollTop / overflow));
    setThumbH(height);
    setThumbTop(top);
    setVisible(true);
  }

  // Streaming transcripts mutate the host on every token; coalesce the mirror
  // to one measurement per frame.
  function scheduleUpdate() {
    if (frame !== undefined) return;
    frame = requestAnimationFrame(() => {
      frame = undefined;
      update();
    });
  }

  function onScroll() {
    if (dragging()) return; // while dragging we drive scrollTop ourselves
    scheduleUpdate();
  }

  function bindObservers(element: HTMLElement) {
    // The Solid ref setter and the props effect can both resolve the same
    // element on first mount. Rebind only when the host actually changes.
    if (host === element) return;
    detach();
    host = element;
    element.addEventListener("scroll", onScroll, { passive: true });
    resize = new ResizeObserver(scheduleUpdate);
    resize.observe(element);
    // Lazy content (streaming text, expanded rows) changes scrollHeight without
    // a scroll or host resize.
    mut = new MutationObserver(scheduleUpdate);
    mut.observe(element, { childList: true, subtree: true, attributes: true });
    update();
  }

  function detach() {
    if (host) host.removeEventListener("scroll", onScroll);
    resize?.disconnect();
    mut?.disconnect();
    resize = null;
    mut = null;
    if (frame !== undefined) {
      cancelAnimationFrame(frame);
      frame = undefined;
    }
  }

  createEffect(
    on(props.host, (element) => {
      if (element) bindObservers(element);
      else detach();
    }),
  );

  onCleanup(detach);

  // Drag the thumb: translate pointer Y delta into scrollTop through the
  // track/overflow ratio so the thumb follows the cursor faithfully.
  let dragStartY = 0;
  let dragStartScroll = 0;

  function removeDragListeners() {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  }

  function onPointerDown(event: PointerEvent) {
    const element = host;
    if (!element) return;
    event.preventDefault();
    dragStartY = event.clientY;
    dragStartScroll = element.scrollTop;
    setDragging(true);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
  }

  function onPointerMove(event: PointerEvent) {
    const element = host;
    if (!element) return;
    const overflow = element.scrollHeight - element.clientHeight;
    if (overflow <= 0) return;
    const track = element.clientHeight - thumbH();
    if (track <= 0) return;
    const delta = event.clientY - dragStartY;
    element.scrollTop = dragStartScroll + (delta / track) * overflow;
    update();
  }

  function onPointerUp() {
    setDragging(false);
    removeDragListeners();
  }

  onCleanup(removeDragListeners);

  return (
    <Show when={visible()}>
      <div class="side-scroll-track" aria-hidden="true">
        {/* The thumb mirrors the host's scrollbar decoratively; the host stays
            the accessible scroll surface and keyboard/wheel scrolling target. */}
        <div
          class="side-scroll-thumb"
          classList={{ "side-scroll-thumb--active": dragging() || hovering() }}
          style={{ transform: `translateY(${thumbTop()}px)`, height: `${thumbH()}px` }}
          onPointerDown={onPointerDown}
          onMouseEnter={() => setHovering(true)}
          onMouseLeave={() => setHovering(false)}
        />
      </div>
    </Show>
  );
}
