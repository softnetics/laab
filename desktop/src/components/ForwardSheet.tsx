import { useMemo, useState } from "react";
import { api, errorText, useChatList } from "../lib/api";
import type { Message } from "../lib/types";
import { notice, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { pressable } from "./kit";
import { buttonClass, Sheet } from "./Sheet";

/** What is being sent on: a message, or someone's contact card. */
export type Forwarding = { kind: "message"; chatId: string; message: Message } | { kind: "contact"; mid: string; name: string };

const KIND: Record<string, string> = { IMAGE: "รูปภาพ", VIDEO: "วิดีโอ", AUDIO: "ข้อความเสียง", STICKER: "สติกเกอร์", FILE: "ไฟล์", CONTACT: "ผู้ติดต่อ" };

function describe(f: Forwarding): string {
  if (f.kind === "contact") return `ผู้ติดต่อ: ${f.name}`;
  const m = f.message;
  if (m.contentType === "FILE" && m.fileName) return m.fileName;
  if (m.contentType === "CONTACT") return `ผู้ติดต่อ: ${m.metadata.displayName ?? ""}`;
  return KIND[m.contentType] ?? m.text;
}

/** LINE's แชร์: pick chats, then send the message or contact card to each. */
export function ForwardSheet() {
  const f = useUi((s) => s.forwarding);
  const chats = useChatList()?.chats ?? [];
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [sending, setSending] = useState(false);
  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("th");
    return [...chats].sort((a, b) => b.lastActivity - a.lastActivity).filter((c) => !q || c.name.toLocaleLowerCase("th").includes(q));
  }, [chats, query]);
  const close = () => {
    setPicked([]);
    setQuery("");
    setUi({ forwarding: null });
  };
  const send = async () => {
    if (!f || !picked.length) return;
    setSending(true);
    try {
      const failed =
        f.kind === "message" ? await api.forwardMessage(f.chatId, f.message.id, picked) : await api.shareContact(f.mid, f.name, picked);
      const name = (id: string) => chats.find((c) => c.id === id)?.name ?? id;
      if (failed.length) notice(`ส่งไม่สำเร็จ: ${failed.map((x) => `${name(x.chatId)} (${x.error})`).join(", ")}`);
      else notice(picked.length > 1 ? `ส่งไปยัง ${picked.length} แชทแล้ว` : `ส่งไปยัง ${name(picked[0])} แล้ว`);
      close();
    } catch (e) {
      notice(errorText(e));
    } finally {
      setSending(false);
    }
  };
  return (
    <Sheet open={!!f} onClose={close} className="h-[480px] w-[360px]" title="ส่งต่อ">
      <div className="truncate px-5 pt-2 text-[12px] text-secondary">{f ? describe(f) : ""}</div>
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
              {...pressable(() => setPicked(on ? picked.filter((x) => x !== c.id) : [...picked, c.id]))}
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
        <button className={buttonClass()} onClick={close}>
          ยกเลิก
        </button>
        <button className={buttonClass(true)} disabled={!picked.length || sending} onClick={send}>
          {sending ? "กำลังส่ง…" : picked.length > 1 ? `ส่ง (${picked.length})` : "ส่ง"}
        </button>
      </div>
    </Sheet>
  );
}
