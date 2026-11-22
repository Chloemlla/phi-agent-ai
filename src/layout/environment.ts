// The single source of environment facts for the frontend.
//
// Layout decisions (how wide the viewport is) and interaction decisions (what
// kind of pointer is available) are deliberately separate signals: a narrow
// desktop window still uses desktop affordances, and a touch laptop still gets
// touch-sized targets. Business components import these signals instead of
// reading `window`, `matchMedia` or platform strings themselves.

import { createSignal } from "solid-js";

/** Below this width the desktop shell switches to drawer navigation. */
export const NARROW_WIDTH = 900;

/** Below this width a page may use compact spacing and inline patterns. */
export const COMPACT_WIDTH = 760;

export type ShellKind = "desktop" | "mobile";

const [viewportWidth, setViewportWidth] = createSignal(
  typeof window === "undefined" ? 1280 : window.innerWidth,
);
const [coarsePointer, setCoarsePointer] = createSignal(false);
const [platformName, setPlatformName] = createSignal("linux");
const [initialized, setInitialized] = createSignal(false);

export { viewportWidth, coarsePointer, platformName };

/** Feed the bootstrap platform string into the layout layer. */
export function setPlatform(platform: string | null | undefined) {
  setPlatformName(platform || "linux");
}

export function isMobilePlatformName(platform: string | null | undefined): boolean {
  return platform === "android" || platform === "ios";
}

export function shellKind(): ShellKind {
  return isMobilePlatformName(platformName()) ? "mobile" : "desktop";
}

export function isMobileShell(): boolean {
  return shellKind() === "mobile";
}

export function isNarrowViewport(): boolean {
  return viewportWidth() <= NARROW_WIDTH;
}

export function isCompactViewport(): boolean {
  return viewportWidth() <= COMPACT_WIDTH;
}

/** Navigation collapses into a drawer on phones and narrow desktop windows. */
export function isDrawerMode(): boolean {
  return isMobileShell() || isNarrowViewport();
}

/** Keyboard labels use Cmd on macOS and Ctrl elsewhere. */
export function isMacPlatform(): boolean {
  return platformName() === "macos";
}

/**
 * Start listening to viewport and pointer changes. Idempotent; returns a
 * cleanup that the app shell calls on unmount.
 */
export function initEnvironment(): () => void {
  if (initialized() || typeof window === "undefined") return () => undefined;
  setInitialized(true);

  const onResize = () => setViewportWidth(window.innerWidth);
  window.addEventListener("resize", onResize, { passive: true });

  const pointerQuery = window.matchMedia("(hover: none), (pointer: coarse)");
  const updatePointer = () => setCoarsePointer(pointerQuery.matches);
  updatePointer();
  pointerQuery.addEventListener("change", updatePointer);

  return () => {
    window.removeEventListener("resize", onResize);
    pointerQuery.removeEventListener("change", updatePointer);
  };
}
