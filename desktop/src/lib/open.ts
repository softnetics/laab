import { openUrl as openExternal } from "@tauri-apps/plugin-opener";
import { api, errorText } from "./api";
import { liffId } from "./links";
import { notice, setUi } from "./ui";

/** LIFF links open as LIFF apps; everything else in the browser. */
export async function openUrl(url: string, chatId: string | null = null) {
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
