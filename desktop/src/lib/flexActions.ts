import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { FlexAction } from "../components/FlexCard";
import { api, errorText } from "./api";
import { openUrl } from "./open";
import { notice } from "./ui";

export async function performFlexAction(a: FlexAction, chatId: string, messageId: string) {
  try {
    switch (a.type) {
      case "uri": {
        const url = a.altUri?.desktop || a.uri;
        if (url) await openUrl(url, chatId);
        return;
      }
      // LINE sends the text through the bot's postback, not as a message
      // of its own; LINE's server posts it in the chat.
      case "message":
        if (a.text) await api.sendMessageButton(chatId, messageId, a.text);
        return;
      case "postback":
        await api.sendPostback(chatId, messageId, a.data ?? "", a.displayText ?? null, a.text ?? null);
        return;
      case "clipboard":
        if (a.clipboardText) await writeText(a.clipboardText);
        notice("คัดลอกแล้ว");
        return;
      default:
        notice(`LAAB ยังไม่รองรับปุ่มแบบ ${a.type ?? "?"}`);
    }
  } catch (e) {
    notice(errorText(e));
  }
}
