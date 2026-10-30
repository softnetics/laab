import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, errorText, mediaUrl, q, queryClient } from "../lib/api";
import type { Sticker } from "../lib/types";
import { notice } from "../lib/ui";
import { CloseIcon } from "./Icons";
import { Spinner } from "./kit";

export function StickerPanel({ chatId, onClose }: { chatId: string; onClose: () => void }) {
  const { data: packs, error, isLoading, refetch } = useQuery(q.stickers);
  const [pack, setPack] = useState<string | null>(null);
  useEffect(() => {
    if (packs?.length && !packs.some((p) => p.id === pack)) setPack(packs[0].id);
  }, [packs, pack]);
  const current = packs?.find((p) => p.id === pack);
  const send = async (s: Sticker) => {
    try {
      await api.sendSticker(chatId, s);
      queryClient.invalidateQueries({ queryKey: ["stickers"] });
    } catch (e) {
      const text = errorText(e);
      if (text === "sticker_not_owned") {
        notice("สติกเกอร์นี้ไม่ได้อยู่ในบัญชีของคุณ");
        queryClient.removeQueries({ queryKey: ["stickers"] });
      } else notice(`ส่งสติกเกอร์ไม่สำเร็จ: ${text}`);
    }
  };
  return (
    <div className="flex h-[248px] flex-col overflow-hidden border-t border-separator bg-chat">
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {isLoading && (
          <div className="grid h-full place-items-center">
            <Spinner size={18} />
          </div>
        )}
        {error && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-[13px] text-secondary">
            {errorText(error)}
            <button className="rounded-md bg-field px-3 py-1 text-primary" onClick={() => refetch()}>
              ลองอีกครั้ง
            </button>
          </div>
        )}
        {packs?.length === 0 && (
          <div className="grid h-full place-items-center px-6 text-center text-[13px] text-secondary">
            ยังไม่มีสติกเกอร์ในบัญชีนี้ ดาวน์โหลดสติกเกอร์ได้จาก LINE บนโทรศัพท์
          </div>
        )}
        {current && (
          <div className="grid gap-1" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(76px, 1fr))" }}>
            {current.stickers.map((s) => (
              <button key={s.stickerId} onClick={() => send(s)} className="grid h-[84px] place-items-center rounded-lg hover:bg-field">
                <img src={mediaUrl.sticker(s.stickerId, false)} className="h-[72px] w-[72px] object-contain" alt="" draggable={false} />
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="flex h-12 items-center gap-1 border-t border-separator px-2">
        <div className="flex flex-1 gap-1 overflow-x-auto">
          {packs?.map((p) => (
            <button
              key={p.id}
              title={p.recent ? "ล่าสุด" : p.name}
              onClick={() => setPack(p.id)}
              className="relative grid h-10 w-10 shrink-0 place-items-center"
            >
              {p.recent ? (
                <span className="text-[11px] text-secondary">ล่าสุด</span>
              ) : (
                <img src={mediaUrl.pack(p.id)} className="h-7 w-7 rounded-md object-contain" alt="" draggable={false} />
              )}
              {pack === p.id && <span className="absolute bottom-0.5 h-0.5 w-[18px] rounded bg-line-green" />}
            </button>
          ))}
        </div>
        <button title="ปิด" onClick={onClose} className="grid h-8 w-8 place-items-center rounded-md text-secondary hover:bg-field">
          <CloseIcon size={16} />
        </button>
      </div>
    </div>
  );
}
