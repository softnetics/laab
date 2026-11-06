import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { FlexAction } from "../components/FlexCard";
import { api, errorText } from "./api";
import { openUrl } from "./open";
import { isPickerMode, pickerNow } from "./datetimePicker";
import { notice, setUi } from "./ui";

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
      case "datetimepicker": {
        if (!isPickerMode(a.mode)) {
          notice("ปุ่มเลือกวันเวลานี้ใช้ไม่ได้");
          return;
        }
        const action = { data: a.data ?? "", mode: a.mode, initial: a.initial ?? null, max: a.max ?? null, min: a.min ?? null };
        const start = await api.datetimePicker(action, pickerNow(a.mode));
        // LINE sends an action whose bounds don't make sense as it is, with no picker.
        if (!start.show) await api.sendDatetimePostback(chatId, messageId, action, null);
        else setUi({ sheet: { kind: "datetime-picker", chatId, messageId, label: a.label ?? null, action, start } });
        return;
      }
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
