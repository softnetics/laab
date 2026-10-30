import { router } from "../router";
import { errorText } from "./api";
import { unfoldChat } from "./fold";
import { LAST_CHAT_KEY, notice, setUi } from "./ui";

/** Open a chat in the main window, optionally scrolled to a message. Unfolds
 *  the conversation pane if it was folded. */
export function openChat(chatId: string, opts: { messageId?: string | null; focusComposer?: boolean } = {}) {
  setUi({ focusMessageId: opts.messageId ?? null, searchOpen: false });
  unfoldChat().catch((e) => notice(errorText(e)));
  localStorage.setItem(LAST_CHAT_KEY, chatId);
  router.navigate({ to: "/chat/$chatId", params: { chatId } });
  if (opts.focusComposer !== false) setUi({ composerFocus: Date.now() });
}

/** Escape with nothing else open closes the chat. */
export function closeChat() {
  localStorage.removeItem(LAST_CHAT_KEY);
  router.navigate({ to: "/" });
}
