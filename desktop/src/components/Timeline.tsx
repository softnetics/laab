import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Avatar } from "./Avatar";
import { ChevronIcon } from "./Icons";
import { ranges } from "../lib/inChatSearch";
import { readersOf } from "../lib/reads";
import { dayHeader } from "../lib/time";
import { buildRows } from "../lib/timeline";
import type { ChatMember, Message } from "../lib/types";
import { setUi, ui, useUi } from "../lib/ui";
import { MessageRow, NoticeRow, type RowContext } from "./MessageRow";
import { readerList, type Reader } from "./ReadMark";
import { Spinner } from "./kit";

const NO_READERS: Reader[] = [];

export interface Older {
  state: "idle" | "loading" | "end" | "error";
  load: () => void;
}

export function Timeline({
  messages,
  loading,
  failed,
  older,
  reads,
  ctx,
  members,
  people,
  search,
}: {
  messages: Message[];
  /** The first page is still loading (nothing cached yet). */
  loading: boolean;
  /** Loading failed; the error strip above says so. */
  failed: boolean;
  older: Older;
  reads: Record<string, string>;
  ctx: RowContext;
  members?: ChatMember[];
  people: Record<string, { picture: string | null }>;
  search: { query: string; hits: string[]; index: number } | null;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const focusId = useUi((s) => s.focusMessageId);
  const rows = useMemo(() => buildRows(messages, ctx.name), [messages, ctx.name]);
  // #9: place read marks once per render, not once per row.
  const marks = useMemo(() => {
    const cursors = { ...reads };
    delete cursors[ctx.me];
    return readersOf(messages, cursors);
  }, [messages, reads, ctx.group, ctx.me]);
  const hitSet = useMemo(() => new Set(search?.hits ?? []), [search?.hits]);
  const currentHit = search && search.hits.length ? search.hits[search.index] : null;

  // Stay pinned to the newest message while content grows (images load after
  // layout). The scroll position itself decides: any scroll that leaves the
  // bottom unpins (wheel, trackpad, keys, dragging the scrollbar), except our
  // own scrolling, which is marked with `autoUntil`.
  const pinned = useRef(true);
  const autoUntil = useRef(0);
  const inner = useRef<HTMLDivElement>(null);
  const [away, setAway] = useState(false);
  const toBottom = () => {
    const el = scroller.current;
    if (!el) return;
    autoUntil.current = Date.now() + 100;
    el.scrollTop = el.scrollHeight;
  };
  useLayoutEffect(() => {
    if (!inner.current) return;
    const ro = new ResizeObserver(() => pinned.current && !focusId && toBottom());
    ro.observe(inner.current);
    return () => ro.disconnect();
  }, [focusId]);
  const lastId = messages[messages.length - 1]?.id;
  const lastMine = messages[messages.length - 1]?.mine;
  useLayoutEffect(() => {
    // Only a message sent from this app pulls the view down; one sent from
    // your phone while you read older messages does not.
    if (lastMine && Date.now() - ui.state.sentAt < 15_000) pinned.current = true;
    if (pinned.current && !focusId) toBottom();
  }, [lastId, lastMine, focusId]);
  useLayoutEffect(() => {
    pinned.current = true;
    if (!focusId) toBottom();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctx.chatId]);
  // Near the top, load older messages. Keep the same message under the
  // reader when they arrive above it.
  const fromBottomBeforeOlder = useRef<number | null>(null);
  const loadOlder = () => {
    const el = scroller.current;
    if (!el || older.state !== "idle" || !messages.length) return;
    fromBottomBeforeOlder.current = el.scrollHeight - el.scrollTop;
    older.load();
  };
  const firstId = messages[0]?.id;
  useLayoutEffect(() => {
    const el = scroller.current;
    const kept = fromBottomBeforeOlder.current;
    if (!el || kept == null) return;
    fromBottomBeforeOlder.current = null;
    autoUntil.current = Date.now() + 100;
    el.scrollTop = el.scrollHeight - kept;
  }, [firstId]);
  // A chat too short to scroll asks for more on its own.
  useEffect(() => {
    const el = scroller.current;
    if (el && older.state === "idle" && el.scrollHeight <= el.clientHeight) loadOlder();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [older.state, firstId]);
  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const fromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (fromBottom < 40) pinned.current = true;
    else if (Date.now() > autoUntil.current) pinned.current = false;
    setAway(fromBottom > 400);
    if (el.scrollTop < 300) loadOlder();
  };
  // A message that arrives while scrolled up shows as a strip over the
  // composer, as in LINE ("O Finally Human…").
  const [incoming, setIncoming] = useState<Message | null>(null);
  const seenLast = useRef(lastId);
  useEffect(() => {
    const last = messages[messages.length - 1];
    if (last && last.id !== seenLast.current && !last.mine && !pinned.current) setIncoming(last);
    seenLast.current = last?.id;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastId]);
  useEffect(() => {
    if (!away) setIncoming(null);
  }, [away]);
  const jumpDown = () => {
    pinned.current = true;
    setIncoming(null);
    setAway(false);
    // A jump, not a smooth scroll: smooth scrolling a long chat is slow.
    autoUntil.current = Date.now() + 100;
    scroller.current?.scrollTo({ top: scroller.current.scrollHeight });
  };

  // ⌘K or a deep link asked for a message; in-chat search steps through hits.
  const target = currentHit ?? focusId;
  // The target may precede a full page of unsent messages.
  useEffect(() => {
    if (target && !loading && !failed && !messages.some((m) => m.id === target) && older.state === "idle") {
      loadOlder();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, loading, failed, messages, older.state]);
  useLayoutEffect(() => {
    if (!target) return;
    const node = scroller.current?.querySelector(`[data-message="${CSS.escape(target)}"]`);
    if (!node) return;
    pinned.current = false;
    // ⌘G / Return steps through hits: jump, don't smooth-scroll.
    node.scrollIntoView({ block: "center" });
    if (focusId && target === focusId) {
      node.animate([{ background: "color-mix(in srgb, var(--color-line-green) 18%, transparent)" }, { background: "transparent" }], {
        duration: 1400,
      });
      const timer = setTimeout(() => {
        if (ui.state.focusMessageId === focusId) setUi({ focusMessageId: null });
      }, 1500);
      return () => clearTimeout(timer);
    }
  }, [target, focusId, rows.length]);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-[18px] pb-4" onScroll={onScroll}>
        <div ref={inner}>
          {rows.length === 0 && !failed && (
            <div className="grid place-items-center pt-24 text-[13px] text-secondary">
              {loading ? <Spinner size={18} /> : "ยังไม่มีข้อความในแชทนี้"}
            </div>
          )}
          {older.state === "error" && (
            <div className="flex justify-center pt-4 text-[12px] text-secondary">
              <button onClick={older.load} className="hover:text-primary">
                โหลดข้อความก่อนหน้าไม่สำเร็จ · ลองอีกครั้ง
              </button>
            </div>
          )}
          {rows.map((r) => (
            <div key={r.message.id}>
              {r.dayStart && (
                <div className="flex justify-center pt-[18px]">
                  <span className="chip-text rounded-full bg-date-chip px-3 py-[5px] text-[12px] leading-[15px]">
                    {dayHeader(r.message.createdTime)}
                  </span>
                </div>
              )}
              {r.notice ? (
                <NoticeRow m={r.message} ctx={ctx} />
              ) : (
                <MessageRow
                  m={r.message}
                  ctx={ctx}
                  runStart={r.runStart}
                  runEnd={r.runEnd}
                  readers={marks[r.message.id] ? readerList(marks[r.message.id], ctx.name, people, members) : NO_READERS}
                  highlight={search && hitSet.has(r.message.id) ? ranges(r.message.text, search.query) : undefined}
                  current={r.message.id === currentHit}
                />
              )}
            </div>
          ))}
        </div>
      </div>
      {older.state === "loading" && (
        <div className="pointer-events-none absolute inset-x-0 top-3 flex justify-center">
          <span className="grid h-8 w-8 place-items-center rounded-full bg-popover shadow-[0_2px_10px_rgba(0,0,0,0.3)]">
            <Spinner size={14} />
          </span>
        </div>
      )}
      <AnimatePresence>
        {incoming && (
          <motion.button
            key="incoming"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 8 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
            onClick={jumpDown}
            className="absolute inset-x-0 bottom-0 flex h-[30px] items-center gap-2 bg-[#1f1f1f]/90 px-3 text-left text-[12.5px] text-[#e6e6e6]"
          >
            <Avatar
              id={incoming.senderId ?? ""}
              name={ctx.name(incoming.senderId ?? "")}
              picture={ctx.picture(incoming.senderId ?? "")}
              size={20}
            />
            <span className="shrink-0 font-semibold">{ctx.name(incoming.senderId ?? "")}</span>
            <span className="truncate text-secondary">{incoming.text || "ส่งข้อความ"}</span>
          </motion.button>
        )}
        {away && !incoming && (
          <motion.button
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
            onClick={jumpDown}
            title="ไปที่ข้อความล่าสุด"
            className="absolute right-5 bottom-4 grid h-9 w-9 place-items-center rounded-full bg-popover text-secondary shadow-[0_2px_10px_rgba(0,0,0,0.3)] ring-1 ring-black/10"
          >
            <ChevronIcon dir="down" size={16} />
          </motion.button>
        )}
      </AnimatePresence>
    </div>
  );
}
