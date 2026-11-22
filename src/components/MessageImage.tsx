// Renderer for stored image assets. The DTO carries metadata only; the bytes
// arrive as a data URL fetched when the component mounts. A module-level cache
// deduplicates the request when the same asset appears in the composer preview
// and later in the transcript.

import { Show, createResource, createSignal } from "solid-js";
import { imageAssetDataUrl } from "../bridge";
import { t } from "../i18n";
import { IconImageOff } from "./Icons";

const cache = new Map<string, string>();
const pending = new Map<string, Promise<string>>();

function loadDataUrl(assetId: string): Promise<string> {
  const cached = cache.get(assetId);
  if (cached) return Promise.resolve(cached);
  const inFlight = pending.get(assetId);
  if (inFlight) return inFlight;
  const request = imageAssetDataUrl(assetId)
    .then((url) => {
      cache.set(assetId, url);
      return url;
    })
    .finally(() => pending.delete(assetId));
  pending.set(assetId, request);
  return request;
}

export function MessageImage(props: {
  assetId: string;
  alt: string;
  variant?: "thumbnail" | "inline";
  class?: string;
}) {
  const [failed, setFailed] = createSignal(false);
  const [data] = createResource(() => props.assetId, loadDataUrl);
  const src = () => (failed() ? undefined : data());
  const unavailable = () => failed() || Boolean(data.error);

  return (
    <span
      class={props.class}
      classList={{
        "message-image": true,
        [`message-image--${props.variant ?? "inline"}`]: true,
      }}
    >
      <Show
        when={src()}
        fallback={
          <span
            class="message-image-placeholder"
            role="img"
            aria-label={unavailable() ? t("Image unavailable") : t("Loading…")}
          >
            <Show when={unavailable()} fallback={<span class="ring" aria-hidden="true" />}>
              <IconImageOff size={18} />
            </Show>
          </span>
        }
      >
        {(url) => (
          <img
            src={url()}
            alt={props.alt}
            loading="lazy"
            decoding="async"
            onError={() => setFailed(true)}
          />
        )}
      </Show>
    </span>
  );
}
