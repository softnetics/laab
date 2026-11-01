import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { mediaUrl, q, useChatList } from "../lib/api";
import { listPreview } from "../lib/chatList";
import type { DuoData } from "../lib/duoScreens";
import { listLabel } from "../lib/time";
import type { Chat, Message } from "../lib/types";
import { useUi } from "../lib/ui";
import { DuoHint } from "./DuoHint";

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
const useDark = () =>
  useSyncExternalStore(
    (on) => (darkQuery.addEventListener("change", on), () => darkQuery.removeEventListener("change", on)),
    () => darkQuery.matches,
  );

const PLACEHOLDER: Record<string, string> = {
  IMAGE: "รูปภาพ",
  VIDEO: "วิดีโอ",
  STICKER: "สติกเกอร์",
  FILE: "ไฟล์",
  AUDIO: "ข้อความเสียง",
};

/** What the model's screens show: the chat list as it is on screen, with
 *  `chatId` in it, and that chat's latest messages from what is stored
 *  (nothing is fetched, so nothing is marked read). */
function useDuoData(chatId: string): DuoData {
  const list = useChatList();
  const order = useUi((s) => s.visibleChatIds);
  const dark = useDark();
  const { data: messages = [] } = useQuery(q.messages(chatId));
  return useMemo(() => {
    const byId = new Map((list?.chats ?? []).map((c) => [c.id, c]));
    const rows: Chat[] = order.flatMap((id) => byId.get(id) ?? []).slice(0, 6);
    const picked = byId.get(chatId);
    if (picked && !rows.some((c) => c.id === chatId)) rows.splice(1, 0, picked);
    const avatar = (c: Chat) => mediaUrl.avatar(c.id, c.picture, c.avatar);
    return {
      chatId,
      name: picked?.name ?? chatId,
      avatar: picked ? avatar(picked) : mediaUrl.avatar(chatId),
      dark,
      rows: rows.map((c) => ({
        id: c.id,
        name: c.name,
        preview: listPreview(c.preview),
        time: listLabel(c.lastActivity),
        unread: c.unread,
        muted: c.muted,
        avatar: avatar(c),
      })),
      messages: messages
        .filter((m: Message) => !m.unsent && (m.contentType === "NONE" ? !!m.text.trim() : m.contentType in PLACEHOLDER))
        .slice(-12)
        .map((m: Message) => ({ mine: m.mine, text: m.contentType === "NONE" ? m.text : PLACEHOLDER[m.contentType] })),
    };
  }, [list, order, chatId, messages, dark]);
}

/** The peek hint's iPhone Duo as a 3D model, its screens showing the chat
 *  list and the peek as LAAB draws them, cracking open to peek and shutting
 *  again. three.js loads only when the hint first shows. Falls back to the
 *  CSS drawing where WebGL is unavailable. */
export function DuoModel({ chatId }: { chatId: string }) {
  const data = useDuoData(chatId);
  const host = useRef<HTMLDivElement>(null);
  const scene = useRef<{ stop: () => void; update: (d: DuoData) => void } | null>(null);
  const latest = useRef(data);
  latest.current = data;
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let gone = false;
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    import("../lib/duoScene")
      .then(({ mountDuo }) => {
        if (gone || !host.current) return;
        scene.current = mountDuo(host.current, latest.current, still);
      })
      .catch(() => setFailed(true));
    return () => {
      gone = true;
      scene.current?.stop();
      scene.current = null;
    };
  }, [chatId]);
  useEffect(() => scene.current?.update(data), [data]);
  if (failed) return <DuoHint />;
  return <div ref={host} className="mx-auto h-[200px] w-[280px]" aria-hidden />;
}
