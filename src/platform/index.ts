// Platform capabilities. Everything the webview cannot portably do — native
// window chrome, folder pickers, external links, the OS back button — enters
// the app through this module. Business and view code consume these functions
// instead of importing Tauri plugins directly.

import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

type WindowHandle = ReturnType<typeof getCurrentWindow>;

function windowHandle(): WindowHandle | null {
  try {
    return getCurrentWindow();
  } catch {
    // Browser previews have no native window.
    return null;
  }
}

export function isNativeDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function ignoreFailure(promise: Promise<unknown>): void {
  void promise.catch(() => undefined);
}

export const platform = {
  minimizeWindow(): void {
    const handle = windowHandle();
    if (handle) ignoreFailure(handle.minimize());
  },

  toggleMaximize(): void {
    const handle = windowHandle();
    if (handle) ignoreFailure(handle.toggleMaximize());
  },

  closeWindow(): void {
    const handle = windowHandle();
    if (handle) ignoreFailure(handle.close());
  },

  async isMaximized(): Promise<boolean> {
    const handle = windowHandle();
    if (!handle) return false;
    try {
      return await handle.isMaximized();
    } catch {
      return false;
    }
  },

  /** Subscribe to window resize events; resolves to an unsubscribe function. */
  async onWindowResized(handler: () => void): Promise<() => void> {
    const handle = windowHandle();
    if (!handle) return () => undefined;
    try {
      const unlisten = await handle.onResized(handler);
      return () => unlisten();
    } catch {
      return () => undefined;
    }
  },

  async openDirectory(defaultPath?: string): Promise<string | null> {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        ...(defaultPath ? { defaultPath } : {}),
      });
      return typeof picked === "string" && picked ? picked : null;
    } catch (error) {
      throw error instanceof Error ? error : new Error(String(error));
    }
  },

  async openExternal(url: string): Promise<void> {
    await openUrl(url);
  },

  async exitApp(): Promise<void> {
    await exit(0);
  },

  /** Android hardware back. Resolves to an unsubscribe function. */
  async onBackButton(handler: (canGoBack: boolean) => void): Promise<() => void> {
    try {
      const unlisten = await onBackButtonPress(({ canGoBack }) => handler(canGoBack));
      return () => {
        void unlisten.unregister();
      };
    } catch {
      return () => undefined;
    }
  },
};
