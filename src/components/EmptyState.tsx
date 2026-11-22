// Quiet panel empty state: an icon, one title and an optional sentence. Used
// inside sidebars and other compact panels; the transcript hero has its own
// richer empty state.

import { Show, type JSX } from "solid-js";

export function EmptyState(props: {
  icon: JSX.Element;
  title: string;
  description?: string;
  class?: string;
}) {
  return (
    <div class={`empty-block ${props.class ?? ""}`}>
      <span class="empty-block-icon" aria-hidden="true">
        {props.icon}
      </span>
      <h2 class="empty-block-title">{props.title}</h2>
      <Show when={props.description}>
        <p class="empty-block-text">{props.description}</p>
      </Show>
    </div>
  );
}
