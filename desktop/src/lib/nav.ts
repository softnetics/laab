import { router } from "../router";
import { errorText } from "./api";
import { unfoldChat } from "./fold";
import { phone } from "./platform";
import { LAST_CHAT_KEY, notice, setUi, ui } from "./ui";

/** Open a chat in the main window, optionally scrolled to a message. Unfolds
 *  the conversation pane if it was folded. */
export function openChat(chatId: string, opts: { messageId?: string | null; focusComposer?: boolean } = {}) {
  setUi({ focusMessageId: opts.messageId ?? null, searchOpen: false, preview: null });
  unfoldChat().catch((e) => notice(errorText(e)));
  if (ui.state.peek === chatId) setUi({ peek: null });
  localStorage.setItem(LAST_CHAT_KEY, chatId);
  router.navigate({ to: "/chat/$chatId", params: { chatId } });
  // On a phone, focusing the composer would pop the keyboard over the chat.
  if (opts.focusComposer !== false && !phone) setUi({ composerFocus: Date.now() });
}

/** The phone opens LINE's separate albums/notes and media windows as pages. */
export function openAuxPage(kind: "albums" | "media", chatId: string, tab?: string, post?: string) {
  router.navigate({
    to: kind === "albums" ? "/albums/$chatId" : "/media/$chatId",
    params: { chatId },
    // A fresh `at` reopens the asked-for note even when it is already open.
    search: { tab: tab ?? "", post: post ?? "", at: String(Date.now()) },
  });
}

/** Back from a phone page: the previous page, or the chat list. */
export function goBack() {
  if (window.history.length > 1 && router.state.location.pathname !== "/") router.history.back();
  else router.navigate({ to: "/" });
}

/** Escape with nothing else open closes the chat. */
export function closeChat() {
  localStorage.removeItem(LAST_CHAT_KEY);
  router.navigate({ to: "/" });
}
