// Toast stack. Flat surfaces with a tone hairline; never a card wall.

import { For } from "solid-js";
import { t } from "../i18n";
import { dismissToast, toasts } from "../toast";
import { IconClose } from "./Icons";

export function Toasts() {
  return (
    <div class="toasts" aria-live="polite">
      <For each={toasts()}>
        {(toast) => (
          <div
            classList={{ toast: true, [`tone-${toast.tone}`]: true }}
            role={toast.tone === "danger" ? "alert" : "status"}
          >
            <span class="toast-message">{toast.message}</span>
            <button
              type="button"
              class="toast-close"
              title={t("Dismiss")}
              aria-label={t("Dismiss")}
              onClick={() => dismissToast(toast.id)}
            >
              <IconClose size={13} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
