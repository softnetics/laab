import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, errorText, queryClient, useSession } from "../lib/api";
import { openUrl } from "../lib/open";
import { notice, setUi, ui, useUi } from "../lib/ui";
import { ChevronIcon, MegaphoneIcon } from "./Icons";
import { Ctx, CtxItem } from "./kit";

type Announcement = {
  seq: string;
  text: string;
  link: string | null;
  creator: string | null;
  createdTime: number;
};

/** Pinned group/room announcements synced via LINE, over the timeline. */
export function Announcements({ chatId, name }: { chatId: string; name: (mid: string) => string }) {
  // Wait for the LINE session; asking earlier caches an empty list.
  const state = useSession()?.state;
  const ready = state === "signed_in" || state === "demo";
  const { data = [] } = useQuery({
    queryKey: ["announcements", chatId],
    queryFn: () => api.announcements(chatId),
    staleTime: 60_000,
    enabled: ready,
  });
  const open = useUi((s) => s.announcementsOpen);
  const setOpen = (v: boolean | ((o: boolean) => boolean)) => {
    const next = typeof v === "function" ? v(ui.state.announcementsOpen) : v;
    // Only one strip expanded — keep ประกาศ from vanishing behind bookmarks.
    setUi({ announcementsOpen: next, ...(next ? { bookmarksOpen: false } : {}) });
  };
  const dismissKey = `announcementDismissed:${chatId}`;
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(dismissKey));
  // Re-read after a new ประกาศ (menu clears the key) or when switching chats.
  useEffect(() => {
    setDismissed(localStorage.getItem(dismissKey));
  }, [dismissKey, data, open]);
  const top = data[0];
  // "ไม่แสดงอีก" hides the bar until a newer announcement arrives.
  if (!top || dismissed === top.seq) return null;

  const dismiss = () => {
    localStorage.setItem(dismissKey, top.seq);
    setDismissed(top.seq);
    setOpen(false);
  };
  const unannounce = (a: Announcement) => {
    api
      .removeAnnouncement(chatId, Number(a.seq))
      .then(() => queryClient.invalidateQueries({ queryKey: ["announcements", chatId] }))
      .catch((err) => notice(errorText(err)));
  };
  const jump = (a: Announcement) => {
    if (!a.link) return;
    // Announcements of a message link back to it inside this chat.
    const target = /^line:\/\/nv\/chatMsg\?/.test(a.link)
      ? new URL(a.link.replace("line://", "https://x/")).searchParams.get("messageId")
      : null;
    setOpen(false);
    if (target) setUi({ focusMessageId: target });
    else openUrl(a.link, chatId);
  };

  // LINE for Mac: #3F3F3F bar; expanded list is megaphone + text + author,
  // right-click "Don't show again / Unannounce", footer "Don't show again / Minimize".
  // Rhythm: tight icon↔text, open body↔author, open list↔footer — keep menu mass, not a twin of bookmarks.
  return (
    <div className="relative z-10 shrink-0 bg-menu text-menu-text">
      {!open ? (
        <button onClick={() => setOpen(true)} className="flex h-[55px] w-full items-center gap-3.5 px-5 text-left">
          <MegaphoneIcon size={19} className="shrink-0 text-pin" />
          <span className="min-w-0 flex-1 truncate text-[13.5px] leading-snug">{top.text}</span>
          <ChevronIcon dir="down" size={12} className="shrink-0 text-secondary" />
        </button>
      ) : (
        <div className="overflow-hidden">
          <div className="max-h-[min(40vh,320px)] overflow-y-auto">
            {data.map((a, i) => (
              <Ctx
                key={a.seq}
                className="block"
                menu={
                  <>
                    <CtxItem onClick={dismiss}>ไม่แสดงอีก</CtxItem>
                    <CtxItem onClick={() => unannounce(a)}>เลิกประกาศ</CtxItem>
                  </>
                }
              >
                <div
                  onClick={() => jump(a)}
                  className={`flex gap-3.5 px-5 py-3.5 ${a.link ? "cursor-pointer" : ""} ${
                    i > 0 ? "border-t border-white/10" : ""
                  }`}
                >
                  <MegaphoneIcon size={19} className="mt-0.5 shrink-0 text-pin" />
                  <div className="min-w-0 flex-1">
                    <div className="selectable text-[13.5px] leading-snug whitespace-pre-wrap">{a.text}</div>
                    {a.creator && <div className="mt-1.5 text-[12.5px] text-secondary">{name(a.creator)}</div>}
                  </div>
                </div>
              </Ctx>
            ))}
          </div>
          <div className="flex h-10 items-center gap-5 border-t border-white/10 px-5 text-[12.5px] text-secondary">
            <button className="hover:text-primary" onClick={dismiss}>
              ไม่แสดงอีก
            </button>
            <button className="hover:text-primary" onClick={() => setOpen(false)}>
              ย่อ
            </button>
            <span className="flex-1" />
            <button
              aria-label="ย่อประกาศ"
              onClick={() => setOpen(false)}
              className="grid h-8 w-8 place-items-center hover:text-primary"
            >
              <ChevronIcon dir="up" size={12} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
