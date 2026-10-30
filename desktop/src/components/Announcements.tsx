import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorText, queryClient, useSession } from "../lib/api";
import { openUrl } from "../lib/open";
import { notice, setUi, ui, useUi } from "../lib/ui";
import { listLabel } from "../lib/time";
import { ChevronIcon, MegaphoneIcon } from "./Icons";

/** Pinned group announcements, shown over the timeline like LINE. */
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
  const setOpen = (v: boolean | ((o: boolean) => boolean)) =>
    setUi({ announcementsOpen: typeof v === "function" ? v(ui.state.announcementsOpen) : v });
  const dismissKey = `announcementDismissed:${chatId}`;
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(dismissKey));
  const top = data[0];
  // "ไม่แสดงอีก" hides the bar until a newer announcement arrives.
  if (!top || dismissed === top.seq) return null;
  const jump = (a: (typeof data)[number]) => {
    if (!a.link) return;
    // Announcements of a message link back to it inside this chat.
    const target = /^line:\/\/nv\/chatMsg\?/.test(a.link)
      ? new URL(a.link.replace("line://", "https://x/")).searchParams.get("messageId")
      : null;
    setOpen(false);
    if (target) setUi({ focusMessageId: target });
    else openUrl(a.link, chatId);
  };
  // LINE for Mac: a #3F3F3F bar, 55pt tall; expanded it shows who posted it
  // and "ไม่แสดงอีก / ย่อ" under a divider.
  return (
    <div className="relative z-10 shrink-0 bg-menu text-menu-text">
      {!open ? (
        <button onClick={() => setOpen(true)} className="flex h-[55px] w-full items-center gap-3 px-5 text-left">
          <MegaphoneIcon size={19} className="shrink-0 text-pin" />
          <span className="flex-1 truncate text-[13.5px]">{top.text}</span>
          <ChevronIcon dir="down" size={12} className="text-secondary" />
        </button>
      ) : (
        <div className="overflow-hidden">
          {data.map((a) => (
            <div key={a.seq} onClick={() => jump(a)} className={`flex gap-3 px-5 pt-3 pb-2 ${a.link ? "cursor-pointer" : ""}`}>
              <MegaphoneIcon size={19} className="mt-0.5 shrink-0 text-pin" />
              <div className="min-w-0 flex-1">
                <div className="selectable text-[13.5px] whitespace-pre-wrap">{a.text}</div>
                <div className="mt-0.5 flex gap-3 text-[12.5px] text-secondary">
                  <span>{a.creator ? name(a.creator) : ""}</span>
                  <span>{listLabel(a.createdTime)}</span>
                  <button
                    className="hover:text-primary"
                    onClick={(e) => {
                      e.stopPropagation();
                      api
                        .removeAnnouncement(chatId, Number(a.seq))
                        .then(() => queryClient.invalidateQueries({ queryKey: ["announcements", chatId] }))
                        .catch((err) => notice(errorText(err)));
                    }}
                  >
                    ลบ
                  </button>
                </div>
              </div>
            </div>
          ))}
          <div className="mt-1 flex h-[36px] items-center gap-5 border-t border-white/10 px-4 text-[12.5px] text-secondary">
            <button
              className="hover:text-primary"
              onClick={() => {
                localStorage.setItem(dismissKey, top.seq);
                setDismissed(top.seq);
                setOpen(false);
              }}
            >
              ไม่แสดงอีก
            </button>
            <button className="hover:text-primary" onClick={() => setOpen(false)}>
              ย่อ
            </button>
            <span className="flex-1" />
            <button aria-label="ย่อประกาศ" onClick={() => setOpen(false)}>
              <ChevronIcon dir="up" size={12} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
