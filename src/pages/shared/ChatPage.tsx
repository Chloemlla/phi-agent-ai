// Shared chat page. Both shells mount this same component; the shell owns the
// rail and window chrome, the page owns the transcript and composer.

import { Conversation } from "../../components/Conversation";
import { Composer } from "../../components/Composer";
import { useChat } from "../../core/chat/ChatProvider";
import { platform } from "../../platform";

export function ChatPage() {
  const chat = useChat();

  function handleOpenUrl(url: string) {
    void platform.openExternal(url).catch(chat.notifyError);
  }

  return (
    <section class="conversation">
      <Conversation
        detail={chat.detail()}
        run={chat.currentRun()}
        streamText={chat.streamText()}
        streamEnded={chat.streamEnded()}
        modeOk={chat.modeOk()}
        modeReason={chat.boot()?.modeReason ?? ""}
        modelConfigured={chat.boot()?.model.configured ?? false}
        now={chat.now()}
        onResolve={chat.resolveApproval}
        onOpenUrl={handleOpenUrl}
        onSend={chat.handleInlineSend}
      />
      <div class="dock-wrap">
        <div class="dock">
          <Composer
            value={chat.input()}
            disabled={chat.composerDisabled()}
            disabledHint={chat.composerDisabledHint()}
            running={chat.isRunning()}
            provider={chat.provider()}
            fallbackModel={chat.boot()?.model.model ?? chat.provider()?.model ?? ""}
            selection={chat.selection()}
            usage={chat.detail()?.session.contextUsage ?? null}
            onModelChange={chat.updateSelection}
            onInput={chat.setInput}
            onSend={chat.send}
            onStop={chat.stopCurrentRun}
          />
        </div>
      </div>
    </section>
  );
}
