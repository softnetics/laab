import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { api, errorText, useChatList } from "../lib/api";
import { notice, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { pressable } from "./kit";
import { buttonClass, Sheet } from "./Sheet";

/** Ask before a LIFF app may see the profile and post to the chat. It never
 *  consents silently. */
export function LiffConsent() {
  const req = useUi((s) => s.liffConsent);
  useEffect(() => {
    const off = listen<{ url: string; chatId: string | null; error: string }>("liff-consent", (e) => {
      if (e.payload.error.includes("liff_consent_required")) setUi({ liffConsent: { url: e.payload.url, chatId: e.payload.chatId } });
      else notice(`เปิด LIFF ไม่สำเร็จ: ${e.payload.error}`);
    });
    return () => {
      off.then((f) => f());
    };
  }, []);
  const close = () => setUi({ liffConsent: null });
  const go = (consent: boolean) => {
    if (!req) return;
    close();
    api.openLiff({ url: req.url, chatId: req.chatId, consent, web: !consent }).catch((e) => notice(`เปิด LIFF ไม่สำเร็จ: ${errorText(e)}`));
  };
  const id = req?.url.match(/\d{6,}-[A-Za-z0-9]{6,}/)?.[0] ?? "";
  return (
    <Sheet open={!!req} onClose={close} className="w-[380px]" title="อนุญาตให้แอป LIFF นี้เข้าถึงบัญชี LINE ไหม">
      <div className="flex flex-col gap-4 p-5 pt-2">
        <p className="text-[13px] leading-relaxed text-secondary">แอปจะเห็นโปรไฟล์ของคุณ และส่งข้อความลงในแชทนี้ได้ ({id})</p>
        <div className="flex justify-end gap-2">
          <button className={buttonClass()} onClick={() => go(false)}>
            เปิดเป็นหน้าเว็บแทน
          </button>
          <button className={buttonClass(true)} onClick={() => go(true)}>
            อนุญาต
          </button>
        </div>
      </div>
    </Sheet>
  );
}

function summary(m: any): string {
  switch (m?.type) {
    case "text":
      return m.text ?? "ข้อความ";
    case "image":
      return "รูปภาพ";
    case "video":
      return "วิดีโอ";
    case "audio":
      return "เสียง";
    case "location":
      return "ตำแหน่ง";
    case "sticker":
      return "สติกเกอร์";
    case "flex":
    case "template":
      return m.altText || "การ์ดข้อความ";
    default:
      return "ข้อความ";
  }
}

/** liff.shareTargetPicker: choose chats, then post through the LIFF app. */
export function SharePicker() {
  const req = useUi((s) => s.sharePicker);
  const chats = useChatList()?.chats ?? [];
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [sending, setSending] = useState(false);
  useEffect(() => {
    const off = listen<NonNullable<typeof req>>("liff-picker", (e) => {
      setPicked([]);
      setQuery("");
      setUi({ sharePicker: e.payload });
    });
    return () => {
      off.then((f) => f());
    };
  }, []);
  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("th");
    return [...chats].sort((a, b) => b.lastActivity - a.lastActivity).filter((c) => !q || c.name.toLocaleLowerCase("th").includes(q));
  }, [chats, query]);
  const finish = async (chatIds: string[]) => {
    if (!req) return;
    setSending(chatIds.length > 0);
    try {
      const r = await api.liffShare(req.window, req.liffId, req.ott, chatIds, req.data.messages ?? []);
      if (r.failed.length) notice(`ส่งไม่สำเร็จบางแชท: ${r.failed.join(", ")}`);
    } catch (e) {
      notice(errorText(e));
    } finally {
      setSending(false);
      setUi({ sharePicker: null });
    }
  };
  const multiple = req?.data.isMultiple !== false;
  return (
    <Sheet open={!!req} onClose={() => finish([])} className="h-[480px] w-[360px]" title="แชร์ไปยังแชท">
      <div className="px-5 pt-2 text-[12px] text-secondary">
        {(req?.data.messages ?? []).map((m, i) => (
          <div key={i} className="truncate">
            {summary(m)}
          </div>
        ))}
      </div>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="ค้นหาแชท"
        className="selectable mx-5 mt-3 h-8 rounded-md bg-field px-2.5 text-[13px] outline-none"
      />
      <div className="min-h-0 flex-1 overflow-y-auto py-2">
        {shown.map((c) => {
          const on = picked.includes(c.id);
          return (
            <div
              key={c.id}
              aria-pressed={on}
              {...pressable(() => setPicked(on ? picked.filter((x) => x !== c.id) : multiple ? [...picked, c.id] : [c.id]))}
              className={`mx-2 flex items-center gap-3 rounded-lg px-3 py-1.5 ${on ? "bg-row-selected" : ""}`}
            >
              <Avatar id={c.id} name={c.name} picture={c.picture} local={c.avatar} size={32} />
              <span className="flex-1 truncate text-[13px]">{c.name}</span>
              <span
                className={`grid h-5 w-5 place-items-center rounded-full text-[11px] text-white ${on ? "bg-line-green" : "ring-1 ring-secondary/50"}`}
              >
                {on ? "✓" : ""}
              </span>
            </div>
          );
        })}
      </div>
      <div className="flex justify-end gap-2 border-t border-separator px-5 py-3">
        <button className={buttonClass()} onClick={() => finish([])}>
          ยกเลิก
        </button>
        <button className={buttonClass(true)} disabled={!picked.length || sending} onClick={() => finish(picked)}>
          {sending ? "กำลังส่ง…" : picked.length > 1 ? `ส่ง (${picked.length})` : "ส่ง"}
        </button>
      </div>
    </Sheet>
  );
}
