import { useSession } from "../lib/api";
import { LockIcon } from "./Icons";

/** Under the chat list when LINE lists a newer Letter Sealing key than this
 *  Mac holds. Messages sealed to it show as unreadable until the user scans
 *  a QR code again, which brings the phone's keys. */
export function LetterSealingBar() {
  const s = useSession();
  const ls = s?.letterSealing;
  if (ls?.state !== "stale") return null;
  // No key at all: the last login brought none from the phone.
  const none = ls.heldKeyId === null;
  return (
    <div
      className="flex items-start gap-2 border-t border-separator bg-rail px-3 py-1.5 text-[12px] leading-snug text-primary motion-safe:animate-row-in"
      title="บัญชีนี้มีกุญแจ Letter Sealing ใหม่ที่เครื่องนี้ยังไม่มี ข้อความที่เข้ารหัสด้วยกุญแจนี้จะถอดรหัสไม่สำเร็จ ออกจากระบบแล้วสแกน QR ด้วย LINE บนโทรศัพท์อีกครั้งเพื่อรับกุญแจ"
    >
      <LockIcon size={12} className="mt-0.5 shrink-0 text-badge-red" />
      <span className="min-w-0 flex-1">
        {none ? "เครื่องนี้ยังไม่มีกุญแจ Letter Sealing ของบัญชี" : "เครื่องนี้ยังไม่มีกุญแจ Letter Sealing ล่าสุด"} ข้อความที่เข้ารหัสจะถอดรหัสไม่สำเร็จ
        <span className="text-secondary">{none ? " · ลองสแกน QR อีกครั้ง" : " · สแกน QR อีกครั้งเพื่อแก้"}</span>
      </span>
    </div>
  );
}
