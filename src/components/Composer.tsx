// Composer: the primary surface. The run bar above it shows live usage; the
// composer itself stays focused on the task input.

import { t } from "../i18n";
import { IconArrowUp } from "./Icons";

export function Composer(props: {
  value: string;
  disabled: boolean;
  running: boolean;
  onInput: (value: string) => void;
  onSend: () => void;
}) {
  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      props.onSend();
    }
  }

  return (
    <div class="composer">
      <textarea
        value={props.value}
        onInput={(event) => props.onInput(event.currentTarget.value)}
        onKeyDown={handleKeyDown}
        placeholder={
          props.running
            ? t("The agent is working…")
            : t("Describe a task… (Enter to send, Shift+Enter for a newline)")
        }
        disabled={props.disabled}
        rows={2}
        aria-label={t("Message")}
      />
      <div class="composer-actions">
        <button
          class="btn primary send"
          onClick={props.onSend}
          disabled={props.disabled || !props.value.trim()}
        >
          {t("Send")}
          <IconArrowUp size={14} />
        </button>
      </div>
    </div>
  );
}
