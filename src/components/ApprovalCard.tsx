// Approval card. Visual weight follows risk exactly as §5 of the design:
// low = a quiet row, medium = a card with parameters, high = a card with a
// danger edge, emphasized digest and a red countdown.

import { Show, createSignal } from "solid-js";
import { formatCountdown } from "../format";
import { t, tArgs } from "../i18n";
import { riskLevel, riskTagText } from "../status";
import type { PendingApproval } from "../types";

function previewText(value: unknown): string {
  if (value && typeof value === "object" && "command" in value) {
    const command = (value as { command?: unknown }).command;
    if (typeof command === "string") return `$ ${command}`;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function ApprovalCard(props: {
  approval: PendingApproval;
  now: number;
  onResolve: (approved: boolean) => Promise<void>;
}) {
  const [busy, setBusy] = createSignal(false);
  const level = () => riskLevel(props.approval.toolId, props.approval.risk);
  const expired = () => {
    const expires = Date.parse(props.approval.expiresAt);
    return Number.isFinite(expires) && expires <= props.now;
  };
  const countdown = () => formatCountdown(props.approval.expiresAt, props.now);

  async function resolve(approved: boolean) {
    if (busy()) return;
    setBusy(true);
    try {
      await props.onResolve(approved);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      classList={{ approval: true, [`level-${level()}`]: true }}
      role="group"
      aria-label={t("Approval required")}
    >
      <div class="approval-head">
        <span class="risk-tag">{riskTagText(level())}</span>
        <span class="approval-tool mono">{props.approval.toolId}</span>
        <Show when={level() !== "low"}>
          <span
            classList={{
              countdown: true,
              mono: true,
              expired: expired(),
            }}
          >
            <Show
              when={!expired()}
              fallback={<span>{t("Expired")}</span>}
            >
              {tArgs("Expires in {time}", { time: countdown() })}
            </Show>
          </span>
        </Show>
      </div>

      <div class="approval-reason">{props.approval.risk}</div>

      <Show when={level() !== "low"}>
        <pre class="approval-preview mono">
          {previewText(props.approval.argsPreview)}
        </pre>
        <div
          classList={{ "approval-digest": true, mono: true, strong: level() === "high" }}
        >
          {tArgs("Action digest {digest}", {
            digest: `${props.approval.actionDigest.slice(0, 24)}…`,
          })}
        </div>
      </Show>

      <div class="approval-actions">
        <button
          type="button"
          classList={{
            btn: true,
            primary: true,
            small: level() === "low",
          }}
          disabled={busy() || expired()}
          onClick={() => void resolve(true)}
        >
          {level() === "high" ? t("Approve and run") : t("Approve")}
        </button>
        <button
          type="button"
          classList={{ btn: true, default: true, small: level() === "low", reject: true }}
          disabled={busy() || expired()}
          onClick={() => void resolve(false)}
        >
          {t("Reject")}
        </button>
        <Show when={level() === "high"}>
          <span class="approval-note">
            {t("This approval applies only to this command and these arguments.")}
          </span>
        </Show>
      </div>
    </section>
  );
}
