import { openUrl as openExternal } from "@tauri-apps/plugin-opener";
import { api, errorText } from "./api";
import { linkDay } from "./calendar";
import { liffId } from "./links";
import { notice, setUi } from "./ui";

/** LIFF links open as LIFF apps; everything else in the browser. */
export async function openUrl(url: string, chatId: string | null = null) {
  // LINE Calendar links don't show the calendar in a browser; open the
  // chat's calendar window instead, on the event's day when the link has one.
  if (/^https?:\/\/line\.me\/R\/calendar(\/|\?|$)/i.test(url)) {
    if (chatId) await api.openAuxWindow("calendar", chatId, "", undefined, linkDay(url) ?? undefined).catch((e) => notice(errorText(e)));
    else notice("เปิดปฏิทินได้จากเมนู ⋮ ของแชท");
    return;
  }
  if (liffId(url)) {
    try {
      await api.openLiff({ url, chatId });
    } catch (e) {
      const text = errorText(e);
      if (text.includes("liff_consent_required")) setUi({ liffConsent: { url, chatId } });
      else notice(`เปิด LIFF ไม่สำเร็จ: ${text}`);
    }
    return;
  }
  if (/^(https?:|mailto:|tel:)/i.test(url)) {
    await openExternal(url).catch((e) => notice(errorText(e)));
  } else {
    notice(`LAAB เปิดลิงก์นี้ไม่ได้: ${url}`);
  }
}
