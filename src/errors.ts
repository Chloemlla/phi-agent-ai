/** Tauri commands reject with strings; normalize any thrown value for display. */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}
