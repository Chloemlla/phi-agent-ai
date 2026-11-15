// Workbench pane. Identity comes from three small things: the title, the
// version badge and an "Updated" label that pulses once after an agent update.

import { Show, createEffect } from "solid-js";
import { t, tArgs } from "../i18n";
import type { ViewAction, WorkbenchView } from "../workbench/types";
import { ViewRenderer } from "../workbench/ViewRenderer";

function restartAnimation(element: HTMLElement | undefined, className: string) {
  if (!element) return;
  element.classList.remove(className);
  // Force a reflow so the animation can run again on the same node.
  void element.offsetWidth;
  element.classList.add(className);
}

export function WorkbenchPane(props: {
  view: WorkbenchView | null;
  onAction: (action: ViewAction) => void;
}) {
  let updatedRef: HTMLElement | undefined;
  let bodyRef: HTMLElement | undefined;
  let lastVersion = -1;

  createEffect(() => {
    const view = props.view;
    if (!view) {
      lastVersion = -1;
      return;
    }
    if (view.version === lastVersion) return;
    lastVersion = view.version;
    queueMicrotask(() => {
      restartAnimation(updatedRef, "pulse");
      restartAnimation(bodyRef, "wb-in");
    });
  });

  return (
    <aside class="workbench">
      <header class="wb-head">
        <span class="wb-title">{props.view?.title || t("Workbench")}</span>
        <div class="wb-meta">
          <Show when={props.view}>
            <span
              class="wb-updated"
              ref={(element) => (updatedRef = element)}
              onAnimationEnd={(event) =>
                (event.currentTarget as HTMLElement).classList.remove("pulse")
              }
            >
              {t("Updated")}
            </span>
            <span class="wb-badge mono">
              {tArgs("v{version}", { version: props.view?.version ?? 0 })}
            </span>
          </Show>
        </div>
      </header>
      <div
        class="wb-body"
        ref={(element) => (bodyRef = element)}
        onAnimationEnd={(event) =>
          (event.currentTarget as HTMLElement).classList.remove("wb-in")
        }
      >
        <Show
          when={props.view}
          fallback={
            <p class="wb-empty">
              {t("The agent will lay out the task here when a visual overview helps.")}
            </p>
          }
        >
          {(view) => <ViewRenderer view={view()} onAction={props.onAction} />}
        </Show>
      </div>
    </aside>
  );
}
