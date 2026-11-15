// Composer: the primary surface. Run bar and composer share one rounded shell
// so the caps read as part of the input, not as a separate toolbar.

import { formatElapsed, formatMoney } from "../format";
import { t, tArgs } from "../i18n";
import { IconArrowUp } from "./Icons";

export function Composer(props: {
  value: string;
  disabled: boolean;
  running: boolean;
  budgetSteps: number;
  budgetCost: number;
  budgetWallSecs: number;
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
        <span class="composer-hint mono">
          {tArgs("Budget: {steps} steps / ${cost} / {time}", {
            steps: props.budgetSteps,
            cost: formatMoney(props.budgetCost, 2),
            time: formatElapsed(props.budgetWallSecs),
          })}
        </span>
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
