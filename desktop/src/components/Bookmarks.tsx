import { useQuery } from "@tanstack/react-query";
import { api, errorText, queryClient, useChatList, useSession } from "../lib/api";
import { openChat } from "../lib/nav";
import { listLabel } from "../lib/time";
import type { Bookmark } from "../lib/types";
import { notice, setUi, ui, useUi } from "../lib/ui";
import { BookmarkFilledIcon, ChevronIcon } from "./Icons";

export function invalidateBookmarks() {
  queryClient.invalidateQueries({ queryKey: ["bookmarks"] });
}

/** Per-chat personal bookmarks — private strip, not the shared ประกาศ banner. */
export function Bookmarks({ chatId }: { chatId: string }) {
  const state = useSession()?.state;
  const ready = state === "signed_in" || state === "demo";
  const { data = [] } = useQuery({
    queryKey: ["bookmarks", chatId],
    queryFn: () => api.bookmarks(chatId),
    enabled: ready,
  });
  const open = useUi((s) => s.bookmarksOpen);
  const setOpen = (v: boolean | ((o: boolean) => boolean)) => {
    const next = typeof v === "function" ? v(ui.state.bookmarksOpen) : v;
    // Collapse ประกาศ when expanding bookmarks so both stay findable.
    setUi({ bookmarksOpen: next, ...(next ? { announcementsOpen: false } : {}) });
  };
  const top = data[0];
  if (!top) return null;

  const jump = (b: Bookmark) => {
    setOpen(false);
    setUi({ focusMessageId: b.messageId });
  };
  const remove = (b: Bookmark) => {
    api
      .setBookmark(b.chatId, b.messageId, b.text, false)
      .then(() => invalidateBookmarks())
      .catch((err) => notice(errorText(err)));
  };

  // Shorter and warmer than ประกาศ's menu bar so the two never read as twins.
  // Rhythm: bind icon↔label tight, open before preview; body↔meta open; list↔footer open.
  return (
    <div className="relative z-10 shrink-0 border-b border-separator bg-field text-primary">
      {!open ? (
        <button onClick={() => setOpen(true)} className="flex h-12 w-full items-center gap-3 px-5 text-left">
          <span className="flex shrink-0 items-center gap-2 text-bookmark">
            <BookmarkFilledIcon size={16} />
            <span className="text-[12px] font-medium">บุ๊กมาร์ก</span>
          </span>
          <span className="min-w-0 flex-1 truncate text-[13.5px] leading-snug">{top.text}</span>
          {data.length > 1 && <span className="shrink-0 tabular-nums text-[12.5px] text-secondary">{data.length}</span>}
          <ChevronIcon dir="down" size={12} className="shrink-0 text-secondary" />
        </button>
      ) : (
        <div className="overflow-hidden">
          <div className="flex items-center gap-2.5 px-5 pt-3.5 pb-2 text-[12px] font-medium text-bookmark">
            <BookmarkFilledIcon size={15} />
            บุ๊กมาร์ก · ส่วนตัว
          </div>
          <div className="max-h-[min(40vh,320px)] overflow-y-auto">
            {data.map((b) => (
              <div key={b.messageId} onClick={() => jump(b)} className="flex cursor-pointer gap-3.5 px-5 py-3.5 hover:bg-row-hover">
                <BookmarkFilledIcon size={15} className="mt-0.5 shrink-0 text-bookmark" />
                <div className="min-w-0 flex-1">
                  <div className="selectable text-[13.5px] leading-snug whitespace-pre-wrap">{b.text}</div>
                  <div className="mt-1.5 flex gap-3.5 text-[12.5px] text-secondary">
                    <span>{listLabel(b.createdAt)}</span>
                    <button
                      className="hover:text-primary"
                      onClick={(e) => {
                        e.stopPropagation();
                        remove(b);
                      }}
                    >
                      ลบ
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
          <div className="flex h-10 items-center gap-5 border-t border-separator px-5 text-[12.5px] text-secondary">
            <button
              className="hover:text-primary"
              onClick={() => {
                setOpen(false);
                setUi({ sheet: { kind: "bookmarks" } });
              }}
            >
              ทั้งหมด
            </button>
            <button className="hover:text-primary" onClick={() => setOpen(false)}>
              ย่อ
            </button>
            <span className="flex-1" />
            <button
              aria-label="ย่อบุ๊กมาร์ก"
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

/** Global personal bookmarks across every chat. */
export function BookmarksSheet({ onDone }: { onDone: () => void }) {
  const state = useSession()?.state;
  const ready = state === "signed_in" || state === "demo";
  const list = useChatList();
  const { data = [], isLoading } = useQuery({
    queryKey: ["bookmarks", null],
    queryFn: () => api.bookmarks(null),
    enabled: ready,
  });
  const chatName = (id: string) => list?.chats.find((c) => c.id === id)?.name ?? id;

  const jump = (b: Bookmark) => {
    onDone();
    openChat(b.chatId, { messageId: b.messageId });
  };
  const remove = (b: Bookmark) => {
    api
      .setBookmark(b.chatId, b.messageId, b.text, false)
      .then(() => invalidateBookmarks())
      .catch((err) => notice(errorText(err)));
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-separator px-5 py-3">
        <BookmarkFilledIcon size={18} className="text-bookmark" />
        <h2 className="text-[15px] font-semibold">บุ๊กมาร์ก</h2>
        <span className="text-[12.5px] text-secondary">มีแค่คุณที่เห็น · ไม่ใช่ประกาศ</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="px-5 py-8 text-center text-[13px] text-secondary">กำลังโหลด…</div>
        ) : data.length === 0 ? (
          <div className="px-5 py-10 text-center text-[13px] text-secondary">
            ยังไม่มีบุ๊กมาร์ก
            <div className="mt-1 text-[12.5px]">คลิกขวาที่ข้อความแล้วเลือก บุ๊กมาร์ก</div>
          </div>
        ) : (
          data.map((b) => (
            <div
              key={`${b.chatId}:${b.messageId}`}
              role="button"
              tabIndex={0}
              onClick={() => jump(b)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  jump(b);
                }
              }}
              className="flex w-full cursor-pointer gap-3 px-5 py-3 text-left hover:bg-row-hover"
            >
              <BookmarkFilledIcon size={14} className="mt-1 shrink-0 text-bookmark" />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2 text-[12px] text-secondary">
                  <span className="truncate">{chatName(b.chatId)}</span>
                  <span className="shrink-0">{listLabel(b.createdAt)}</span>
                </div>
                <div className="mt-0.5 truncate text-[13.5px]">{b.text}</div>
              </div>
              <button
                type="button"
                className="shrink-0 self-center text-[12.5px] text-secondary hover:text-primary"
                onClick={(e) => {
                  e.stopPropagation();
                  remove(b);
                }}
              >
                ลบ
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
