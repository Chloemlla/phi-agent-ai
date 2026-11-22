/* @refresh reload */

// Local-first fonts: bundled at build time, never fetched at runtime.
// Figtree covers Latin, Noto Sans SC covers Chinese, Cascadia Code carries data.
import "@fontsource/figtree/latin-400.css";
import "@fontsource/figtree/latin-400-italic.css";
import "@fontsource/figtree/latin-500.css";
import "@fontsource/figtree/latin-600.css";
import "@fontsource/figtree/latin-600-italic.css";
import "@fontsource/figtree/latin-700.css";
import "@fontsource/noto-sans-sc/chinese-simplified-400.css";
import "@fontsource/noto-sans-sc/chinese-simplified-500.css";
import "@fontsource/noto-sans-sc/chinese-simplified-600.css";
import "@fontsource/cascadia-code/latin-400.css";
import "@fontsource/cascadia-code/latin-600.css";

import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import App from "./app/App";
import { setLang, t } from "./i18n";

// Mirror the browser language before bootstrap so the first paint (loading
// state included) has the right document language. The node's saved setting
// corrects it once Bootstrap arrives.
const initialLang = navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
setLang(initialLang);

function errorText(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.length > 240 ? `${message.slice(0, 240)}…` : message;
}

render(
  () => (
    <ErrorBoundary
      fallback={(error) => (
        <div style={{ display: "flex", height: "100%" }}>
          <div class="empty-state">
            <h1 class="empty-title">{t("Something went wrong.")}</h1>
            <p class="empty-text">{errorText(error)}</p>
            <button type="button" class="btn primary" onClick={() => window.location.reload()}>
              {t("Reload")}
            </button>
          </div>
        </div>
      )}
    >
      <App />
    </ErrorBoundary>
  ),
  document.getElementById("root") as HTMLElement,
);
