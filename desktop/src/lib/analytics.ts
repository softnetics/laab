import { api, errorText, q, queryClient } from "./api";
import { notice, setUi } from "./ui";

/** What the statistics contain; shown when asking and kept in the README. */
const ABOUT =
  "ส่งจำนวนครั้งที่ใช้แต่ละฟีเจอร์ต่อวัน และรายงานเมื่อแอปขัดข้อง พร้อมเวอร์ชันของแอปและ macOS\n\nไม่ส่งข้อความ ชื่อ บัญชี LINE หรือรายชื่อแชท ปิดได้ทุกเมื่อในเมนู ⋯";

export async function setAnalytics(on: boolean) {
  await api.setAnalytics(on);
  await queryClient.invalidateQueries({ queryKey: q.analytics.queryKey });
}

/** Asks once, after sign-in, whether to send usage statistics. */
export async function askAboutAnalytics() {
  const status = await queryClient.fetchQuery(q.analytics);
  if (status !== "unasked") return;
  setUi({
    confirm: {
      title: "ส่งสถิติการใช้งานแบบไม่ระบุตัวตนไหม",
      body: ABOUT,
      action: "ส่ง",
      dismissLabel: "ไม่ส่ง",
      run: () => setAnalytics(true),
      dismiss: () => setAnalytics(false).catch((e) => notice(errorText(e))),
    },
  });
}

/** Uncaught webview errors go to Rust, which keeps the error name and the
 *  stack frames in LAAB's own scripts and drops the message. At most 20 per
 *  window, and each distinct stack once. */
export function reportUncaughtErrors() {
  const seen = new Set<string>();
  const report = (e: unknown) => {
    const name = e instanceof Error ? e.name : "Error";
    const stack = e instanceof Error ? (e.stack ?? "") : "";
    const key = `${name}\n${stack}`;
    if (seen.has(key) || seen.size >= 20) return;
    seen.add(key);
    api.reportJsError(name, stack).catch(() => {});
  };
  window.addEventListener("error", (e) => report(e.error));
  window.addEventListener("unhandledrejection", (e) => report(e.reason));
}
