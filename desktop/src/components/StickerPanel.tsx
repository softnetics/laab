import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { api, errorText, mediaUrl, q, queryClient } from "../lib/api";
import basicEmoji from "../lib/basicEmoji.json";
import type { Sticker } from "../lib/types";
import { notice } from "../lib/ui";
import { ClockIcon, CloseIcon } from "./Icons";
import { Spinner } from "./kit";

const EMOJI_RECENT_KEY = "laab.emojiRecent";
const EMOJI_RECENT_MAX = 35;
const emojiIds = Array.from({ length: basicEmoji.count }, (_, i) => String(i + 1).padStart(3, "0"));
const emojiSrc = (id: string) =>
  `https://stickershop.line-scdn.net/sticonshop/v1/sticon/${basicEmoji.productId}/android/${id}.png`;
const packTabSrc = `https://stickershop.line-scdn.net/sticonshop/v1/sticon/${basicEmoji.productId}/android/tab_on.png`;

function readRecent(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(EMOJI_RECENT_KEY) ?? "[]");
    if (!Array.isArray(raw)) return [];
    return raw.filter((id): id is string => typeof id === "string" && emojiIds.includes(id)).slice(0, EMOJI_RECENT_MAX);
  } catch {
    return [];
  }
}

function rememberRecent(id: string) {
  const next = [id, ...readRecent().filter((x) => x !== id)].slice(0, EMOJI_RECENT_MAX);
  localStorage.setItem(EMOJI_RECENT_KEY, JSON.stringify(next));
  return next;
}

/** Shelf pack/recent button. Active = filled field tile; idle has no chrome. */
const shelfTile =
  "grid h-8 w-8 shrink-0 place-items-center rounded-[9px] text-secondary hover:bg-field hover:text-primary focus-visible:outline-2 focus-visible:outline-line-green";
const shelfActive = "bg-field text-primary hover:bg-field hover:text-primary";

export function StickerPanel({ chatId, onClose }: { chatId: string; onClose: () => void }) {
  const [mode, setMode] = useState<"stickers" | "emoji">("stickers");
  return (
    <div className="flex h-[288px] flex-col overflow-hidden border-t border-separator bg-chat">
      <div className="flex h-10 shrink-0 items-center gap-1 border-b border-separator px-2.5">
        {([ ["stickers", "สติกเกอร์"], ["emoji", "อิโมจิ"] ] as const).map(([value, label]) => {
          const active = mode === value;
          return (
            <button
              key={value}
              aria-pressed={active}
              onClick={() => setMode(value)}
              className={`h-7 rounded-full px-2.5 text-[13px] focus-visible:outline-2 focus-visible:outline-line-green ${
                active ? "bg-field font-medium text-primary" : "text-secondary hover:text-primary"
              }`}
            >
              {label}
            </button>
          );
        })}
        <button title="ปิด" aria-label="ปิด" onClick={onClose} className="ml-auto grid h-8 w-8 place-items-center rounded-md text-secondary hover:bg-field">
          <CloseIcon size={16} />
        </button>
      </div>
      {mode === "emoji" ? <EmojiPicker key={chatId} chatId={chatId} /> : <StickerPacks chatId={chatId} />}
    </div>
  );
}

function EmojiPicker({ chatId }: { chatId: string }) {
  const sending = useRef(false);
  const [pending, setPending] = useState<string | null>(null);
  const [shelf, setShelf] = useState<"recent" | "pack">("pack");
  const [recent, setRecent] = useState(readRecent);
  const ids = shelf === "recent" ? recent : emojiIds;

  const send = async (id: string) => {
    if (sending.current) return;
    sending.current = true;
    setPending(id);
    try {
      await api.sendEmoji(chatId, id);
      setRecent(rememberRecent(id));
    } catch (e) {
      notice(`ส่งอิโมจิไม่สำเร็จ: ${errorText(e)} กรุณาลองอีกครั้ง`);
    } finally {
      sending.current = false;
      setPending(null);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex h-11 shrink-0 items-center gap-2.5 border-b border-separator px-2.5">
        <button
          title="ล่าสุด"
          aria-label="อิโมจิล่าสุด"
          aria-pressed={shelf === "recent"}
          onClick={() => setShelf("recent")}
          className={`${shelfTile} ${shelf === "recent" ? shelfActive : ""}`}
        >
          <ClockIcon size={15} strokeWidth={2} />
        </button>
        <button
          title="อิโมจิ LINE"
          aria-label="อิโมจิ LINE"
          aria-pressed={shelf === "pack"}
          onClick={() => setShelf("pack")}
          className={`${shelfTile} ${shelf === "pack" ? shelfActive : ""}`}
        >
          <img src={packTabSrc} alt="" className="h-6 w-6 object-contain" draggable={false} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2" aria-label="อิโมจิ LINE" aria-busy={pending !== null}>
        {shelf === "recent" && recent.length === 0 ? (
          <div className="grid h-full place-items-center px-6 text-center text-[13px] text-secondary">
            ยังไม่มีอิโมจิล่าสุด ส่งจากชุดอิโมจิเพื่อเก็บไว้ที่นี่
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(40px,1fr))] gap-1">
            {ids.map((id) => (
              <button
                key={id}
                title={`ส่งอิโมจิ LINE ${id}`}
                aria-label={`ส่งอิโมจิ LINE ${id}`}
                disabled={pending !== null}
                onClick={() => send(id)}
                className="grid h-10 place-items-center rounded-md hover:bg-field focus-visible:outline-2 focus-visible:outline-line-green disabled:opacity-50"
              >
                {pending === id ? (
                  <Spinner size={16} />
                ) : (
                  <img src={emojiSrc(id)} alt="" loading="lazy" className="h-7 w-7 object-contain" draggable={false} />
                )}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function StickerPacks({ chatId }: { chatId: string }) {
  const { data: packs, error, isLoading, refetch } = useQuery(q.stickers);
  const [pack, setPack] = useState<string | null>(null);
  useEffect(() => {
    if (packs?.length && !packs.some((p) => p.id === pack)) setPack(packs[0].id);
  }, [packs, pack]);
  const current = packs?.find((p) => p.id === pack);
  // Packages come without their stickers; the one opened loads them.
  const lazy = !!current && !current.recent && current.stickers.length === 0;
  const opened = useQuery({
    queryKey: ["sticker-pack", current?.id],
    queryFn: () => api.stickerPack(current!.id, current!.name, current!.version ?? null),
    enabled: lazy,
    staleTime: Infinity,
  });
  const stickers = lazy ? (opened.data?.stickers ?? []) : (current?.stickers ?? []);
  // ⌥-click sends without a notification, as ⌥Return does for text.
  const send = async (s: Sticker, silent: boolean) => {
    try {
      await api.sendSticker(chatId, s, silent);
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
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex h-11 shrink-0 items-center border-b border-separator px-2.5">
        <div className="flex flex-1 gap-2.5 overflow-x-auto">
          {packs?.map((p) => (
            <button
              key={p.id}
              title={p.recent ? "ล่าสุด" : p.name}
              aria-label={p.recent ? "สติกเกอร์ล่าสุด" : p.name}
              aria-pressed={pack === p.id}
              onClick={() => setPack(p.id)}
              className={`${shelfTile} ${pack === p.id ? shelfActive : ""}`}
            >
              {p.recent ? (
                <ClockIcon size={15} strokeWidth={2} />
              ) : (
                <img src={mediaUrl.pack(p.id)} loading="lazy" className="h-6 w-6 rounded-md object-contain" alt="" draggable={false} />
              )}
            </button>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {(isLoading || (lazy && opened.isLoading)) && (
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
        {lazy && opened.error && (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-[13px] text-secondary">
            โหลดสติกเกอร์ชุดนี้ไม่ได้
            <button className="rounded-md bg-field px-3 py-1 text-primary" onClick={() => opened.refetch()}>
              ลองอีกครั้ง
            </button>
          </div>
        )}
        {current && (
          <div className="grid gap-1" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(76px, 1fr))" }}>
            {stickers.map((s) => (
              <button key={s.stickerId} onClick={(e) => send(s, e.altKey)} className="grid h-[84px] place-items-center rounded-lg hover:bg-field">
                <img
                  src={mediaUrl.sticker(s.stickerId, false)}
                  loading="lazy"
                  className="h-[72px] w-[72px] object-contain"
                  alt=""
                  draggable={false}
                />
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
