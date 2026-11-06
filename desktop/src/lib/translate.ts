// Which language "แปล" translates into: the one the user picked last, else
// the Mac's language when LINE can translate into it, else Thai.

/** Languages LINE's translation offers, by the code its API takes. */
export const translateLanguages: { code: string; label: string }[] = [
  { code: "th", label: "ไทย" },
  { code: "en", label: "อังกฤษ" },
  { code: "ja", label: "ญี่ปุ่น" },
  { code: "ko", label: "เกาหลี" },
  { code: "zh-CN", label: "จีนตัวย่อ" },
  { code: "zh-TW", label: "จีนตัวเต็ม" },
  { code: "vi", label: "เวียดนาม" },
  { code: "id", label: "อินโดนีเซีย" },
  { code: "es", label: "สเปน" },
  { code: "fr", label: "ฝรั่งเศส" },
  { code: "de", label: "เยอรมัน" },
  { code: "it", label: "อิตาลี" },
  { code: "ru", label: "รัสเซีย" },
];

const KEY = "laab.translate.target";

/** LINE's code for a BCP 47 locale such as `th-TH` or `zh-Hant-HK`, or
 *  null when LINE can't translate into it. */
export function targetOf(locale: string): string | null {
  const parts = locale.trim().replace(/_/g, "-").split("-");
  const lang = parts[0]?.toLowerCase();
  if (!lang) return null;
  if (lang === "zh") {
    const rest = parts.slice(1).map((p) => p.toLowerCase());
    return rest.some((p) => p === "hant" || p === "tw" || p === "hk" || p === "mo") ? "zh-TW" : "zh-CN";
  }
  return translateLanguages.some((l) => l.code === lang) ? lang : null;
}

/** The language to translate into now. */
export function translateTarget(languages: readonly string[] = typeof navigator === "undefined" ? [] : navigator.languages): string {
  let picked: string | null = null;
  try {
    picked = localStorage.getItem(KEY);
  } catch {}
  if (picked && translateLanguages.some((l) => l.code === picked)) return picked;
  for (const l of languages) {
    const t = targetOf(l);
    if (t) return t;
  }
  return "th";
}

export function setTranslateTarget(code: string) {
  try {
    localStorage.setItem(KEY, code);
  } catch {}
}
