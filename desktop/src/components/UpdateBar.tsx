import { useState } from "react";
import { errorText } from "../lib/api";
import { installAndRestart, useReadyVersion } from "../lib/updater";

/** Offers a downloaded update. Shown under the chat list and on the sign-in
 *  screen, so an update that fixes signing in can still be installed. The
 *  sign-in screen has no status bar, so a failed install shows here. */
export function UpdateBar() {
  const version = useReadyVersion();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!version) return null;
  const restart = () => {
    setBusy(true);
    setError(null);
    installAndRestart().catch((e) => {
      setBusy(false);
      setError(errorText(e));
    });
  };
  return (
    <div className="no-drag flex items-center gap-2 border-t border-separator bg-rail px-3 py-1.5 text-[12px] motion-safe:animate-row-in">
      <span className={`min-w-0 flex-1 truncate text-left ${error ? "text-badge-red" : "text-primary"}`} title={error ?? undefined}>
        {error ? `ติดตั้ง LAAB ${version} ไม่สำเร็จ: ${error}` : `LAAB ${version} พร้อมติดตั้งแล้ว`}
      </span>
      <button
        className="h-6 shrink-0 rounded-md bg-line-green px-2.5 text-[12px] font-medium text-white transition-transform duration-150 ease-out active:scale-[0.96] disabled:opacity-50"
        disabled={busy}
        onClick={restart}
      >
        {busy ? "กำลังติดตั้ง…" : "รีสตาร์ต"}
      </button>
    </div>
  );
}
