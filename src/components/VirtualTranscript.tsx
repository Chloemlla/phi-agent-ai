// Measured windowing for the transcript. Short conversations keep the plain
// flow (the exact original DOM); only long ones switch to an absolutely
// positioned window measured with ResizeObserver. The scroll container stays
// in Conversation and is shared with the auto-scroll logic here.

import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  type Accessor,
  type JSX,
} from "solid-js";

export type VirtualRow = { key: string };

/** Below this row count the transcript stays in normal document flow. */
export const VIRTUALIZE_THRESHOLD = 80;

const OVERSCAN = 3;
const BOTTOM_SLACK = 96;

export function VirtualTranscript<T extends VirtualRow>(props: {
  rows: T[];
  scroll: () => HTMLElement | undefined;
  estimate: (row: T) => number;
  render: (row: T) => JSX.Element;
  /** Changes whenever tail content (stream text, "Stopped") changes. */
  revision: Accessor<unknown>;
}) {
  const [version, setVersion] = createSignal(0);
  const [gap, setGap] = createSignal(24);
  const [viewport, setViewport] = createSignal({ top: 0, height: 0 });
  const [focusedKey, setFocusedKey] = createSignal<string | null>(null);
  const [keyboardFocus, setKeyboardFocus] = createSignal(false);
  const heights = new Map<string, number>();

  let scrollEl: HTMLElement | undefined;
  let listEl: HTMLDivElement | undefined;
  let pinned = false;
  let previousCount = 0;
  let pendingAnchor = 0;
  let scrollFrame: number | undefined;
  let bottomFrame: number | undefined;
  let sizeObserver: ResizeObserver | undefined;
  let innerObserver: ResizeObserver | undefined;

  const virtualized = () => props.rows.length > VIRTUALIZE_THRESHOLD;

  const layout = createMemo(() => {
    version();
    const rows = props.rows;
    const spacing = gap();
    const offsets = new Array<number>(rows.length + 1);
    const offsetByKey = new Map<string, number>();
    const indexByKey = new Map<string, number>();
    let y = 0;
    for (let index = 0; index < rows.length; index += 1) {
      const row = rows[index];
      offsets[index] = y;
      offsetByKey.set(row.key, y);
      indexByKey.set(row.key, index);
      y += (heights.get(row.key) ?? props.estimate(row)) + spacing;
    }
    if (rows.length > 0) y = Math.max(0, y - spacing);
    offsets[rows.length] = y;
    return { offsets, offsetByKey, indexByKey, total: y };
  });

  function firstAtOrAfter(offsets: number[], position: number): number {
    let low = 0;
    let high = offsets.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (offsets[mid] < position) low = mid + 1;
      else high = mid;
    }
    return low;
  }

  const range = createMemo(() => {
    const rows = props.rows;
    if (rows.length === 0) return { start: 0, end: 0 };
    // Rendering every row while keyboard focus is inside keeps native tab
    // order and offscreen links reachable.
    if (keyboardFocus()) return { start: 0, end: rows.length };
    const { offsets, indexByKey } = layout();
    const { top, height } = viewport();
    const bottom = top + Math.max(height, 320);
    let start = firstAtOrAfter(offsets, top) - 1 - OVERSCAN;
    let end = firstAtOrAfter(offsets, bottom) + OVERSCAN;
    start = Math.min(Math.max(0, start), rows.length);
    end = Math.min(Math.max(end, start + 1), rows.length);
    const focused = focusedKey();
    if (focused) {
      const index = indexByKey.get(focused);
      if (index !== undefined) {
        if (index < start) start = index;
        if (index >= end) end = index + 1;
      }
    }
    return { start, end };
  });

  const visible = createMemo(() => {
    const { start, end } = range();
    return props.rows.slice(start, end);
  });

  function nearBottom(element: HTMLElement) {
    return element.scrollHeight - element.scrollTop - element.clientHeight < BOTTOM_SLACK;
  }

  function updateViewport(element: HTMLElement) {
    const list = listEl?.isConnected ? listEl : undefined;
    const top = list
      ? element.getBoundingClientRect().top - list.getBoundingClientRect().top
      : element.scrollTop;
    setViewport({ top: Math.max(0, top), height: element.clientHeight });
  }

  function scrollToBottom() {
    const element = scrollEl;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
    updateViewport(element);
  }

  function scheduleBottom() {
    if (bottomFrame !== undefined) return;
    bottomFrame = requestAnimationFrame(() => {
      bottomFrame = undefined;
      if (pinned) scrollToBottom();
    });
  }

  function measure(key: string, rawHeight: number) {
    const next = Math.round(rawHeight);
    if (next <= 0) return;
    const prev = heights.get(key);
    if (prev === next) return;
    const { offsets, indexByKey } = layout();
    const index = indexByKey.get(key);
    if (index !== undefined) {
      const height = prev ?? props.estimate(props.rows[index]);
      // Content above the viewport changed size: keep the visible rows still.
      if (offsets[index] + height <= viewport().top + 1) {
        pendingAnchor += next - height;
        queueMicrotask(() => {
          const element = scrollEl;
          if (element && pendingAnchor !== 0) {
            const delta = pendingAnchor;
            pendingAnchor = 0;
            element.scrollTop += delta;
          }
        });
      }
    }
    heights.set(key, next);
    setVersion((value) => value + 1);
    if (pinned) scheduleBottom();
  }

  function onScroll() {
    const element = scrollEl;
    if (!element) return;
    pinned = nearBottom(element);
    if (scrollFrame !== undefined) return;
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = undefined;
      if (scrollEl) updateViewport(scrollEl);
    });
  }

  function onFocusIn(event: FocusEvent) {
    setKeyboardFocus(true);
    const target = event.target instanceof Element ? event.target : null;
    const row = target?.closest("[data-row-key]");
    if (row) setFocusedKey(row.getAttribute("data-row-key"));
  }

  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget instanceof Node ? event.relatedTarget : null;
    if (!scrollEl || !next || !scrollEl.contains(next)) {
      setKeyboardFocus(false);
    }
    const row = next instanceof Element ? next.closest("[data-row-key]") : null;
    if (row && row.getAttribute("data-row-key") === focusedKey()) return;
    setFocusedKey(null);
  }

  function readGap(inner: Element | null) {
    if (!inner) return;
    const value = parseFloat(getComputedStyle(inner).rowGap);
    if (Number.isFinite(value)) setGap(value);
  }

  function attach(element: HTMLElement) {
    detach();
    scrollEl = element;
    pinned = nearBottom(element);
    updateViewport(element);
    element.addEventListener("scroll", onScroll, { passive: true });
    element.addEventListener("focusin", onFocusIn);
    element.addEventListener("focusout", onFocusOut);
    const inner = element.firstElementChild;
    readGap(inner);
    if (typeof ResizeObserver === "undefined") return;
    sizeObserver = new ResizeObserver(() => {
      updateViewport(element);
      if (pinned) scheduleBottom();
    });
    sizeObserver.observe(element);
    if (inner) {
      innerObserver = new ResizeObserver(() => readGap(inner));
      innerObserver.observe(inner);
    }
  }

  function detach() {
    scrollEl?.removeEventListener("scroll", onScroll);
    scrollEl?.removeEventListener("focusin", onFocusIn);
    scrollEl?.removeEventListener("focusout", onFocusOut);
    scrollEl = undefined;
    sizeObserver?.disconnect();
    innerObserver?.disconnect();
    sizeObserver = undefined;
    innerObserver = undefined;
  }

  createEffect(() => {
    const element = props.scroll();
    if (element && element !== scrollEl) attach(element);
  });

  createEffect(() => {
    props.revision();
    const count = props.rows.length;
    if (count === 0) {
      previousCount = 0;
      return;
    }
    if (previousCount === 0) {
      // First content (or a new session): match the old behavior by looking
      // at the real position instead of pinning an empty transcript.
      previousCount = count;
      if (scrollEl) pinned = nearBottom(scrollEl);
      return;
    }
    previousCount = count;
    if (pinned) queueMicrotask(scrollToBottom);
  });

  onCleanup(() => {
    detach();
    if (scrollFrame !== undefined) cancelAnimationFrame(scrollFrame);
    if (bottomFrame !== undefined) cancelAnimationFrame(bottomFrame);
  });

  return (
    <Show when={virtualized()} fallback={<For each={props.rows}>{(row) => props.render(row)}</For>}>
      <div
        class="virtual-list"
        ref={(element) => {
          listEl = element;
          queueMicrotask(() => {
            if (scrollEl) updateViewport(scrollEl);
          });
        }}
        style={{ height: `${layout().total}px` }}
      >
        <For each={visible()}>
          {(row) => (
            <MeasuredRow
              row={row}
              top={layout().offsetByKey.get(row.key) ?? 0}
              render={props.render}
              measure={measure}
            />
          )}
        </For>
      </div>
    </Show>
  );
}

function MeasuredRow<T extends VirtualRow>(props: {
  row: T;
  top: number;
  render: (row: T) => JSX.Element;
  measure: (key: string, height: number) => void;
}) {
  // Capture the rendered subtree once; its own props stay reactive.
  const content = props.render(props.row);
  let element: HTMLDivElement | undefined;

  onMount(() => {
    const read = () => {
      const height = element?.offsetHeight ?? 0;
      if (height > 0) props.measure(props.row.key, height);
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(read);
    if (element) observer.observe(element);
    onCleanup(() => observer.disconnect());
  });

  return (
    <div
      class="virtual-row"
      data-row-key={props.row.key}
      ref={element}
      style={{ transform: `translateY(${props.top}px)` }}
    >
      {content}
    </div>
  );
}
