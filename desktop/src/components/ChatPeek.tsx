import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, mediaUrl, messageLimit, q, queryClient, setMessageLimit, usePeople, useSession } from "../lib/api";
import { isGroup, isKeepMemo, unreadBadgeCount } from "../lib/chatList";
import { noticeFor } from "../lib/notices";
import { buildRows } from "../lib/timeline";
import type { Chat, Message } from "../lib/types";
import { Avatar } from "./Avatar";
import { BookmarkIcon } from "./Icons";
import { Spinner } from "./kit";

/** Same curve as folder rows and the drop tray. */
const EASE = [0.32, 0.72, 0, 1] as const;

const cardVariants = {
  enter: { opacity: 1, x: 0, scale: 1, filter: "blur(0px)" },
  exitList: { opacity: 0, x: -10, scale: 0.96, filter: "blur(3px)" },
  exitChat: { opacity: 0, x: 28, scale: 1.03, filter: "blur(3px)" },
};

const cardVariantsReduce = {
  enter: { opacity: 1 },
  exitList: { opacity: 0 },
  exitChat: { opacity: 0 },
};

/** Load older stored / LINE history without acknowledging the chat. */
function usePeekOlder(chatId: string, count: number) {
  const [state, setState] = useState<"idle" | "loading" | "end" | "error">("idle");
  useEffect(() => setState("idle"), [chatId]);
  const busy = useRef(false);
  const load = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setState("loading");
    try {
      const limit = messageLimit(chatId);
      const grow = count >= limit ? 200 : await api.olderMessages(chatId);
      if (grow > 0) {
        setMessageLimit(chatId, Math.max(limit, count) + grow);
        await queryClient.refetchQueries({ queryKey: ["messages", chatId] });
        setState("idle");
      } else setState("end");
    } catch {
      setState("error");
    } finally {
      busy.current = false;
    }
  }, [chatId, count]);
  return { state, load };
}

function peekLabel(m: Message): string {
  switch (m.contentType) {
    case "IMAGE":
      return "รูปภาพ";
    case "VIDEO":
      return "วิดีโอ";
    case "FILE":
      return m.fileName ?? "ไฟล์";
    case "STICKER":
      return "สติกเกอร์";
    case "CONTACT":
      return `ผู้ติดต่อ: ${m.metadata.displayName ?? ""}`;
    case "CALL":
      return "สายเรียกเข้า";
    case "FLEX":
      return m.metadata.ALT_TEXT || "การ์ด";
    case "RICH":
      return "ข้อความ";
    default:
      return m.text.trim() || "ข้อความ";
  }
}

function PeekBubble({ m }: { m: Message }) {
  if (m.contentType === "STICKER" && m.stickerId) {
    return (
      <div className={`flex ${m.mine ? "justify-end" : "justify-start"}`}>
        <img src={mediaUrl.sticker(m.stickerId, false)} alt="" className="h-16 w-16 object-contain" draggable={false} />
      </div>
    );
  }
  if (m.contentType === "IMAGE" || m.contentType === "VIDEO") {
    const src = m.imagePath && m.contentType === "IMAGE" ? mediaUrl.local(m.imagePath) : mediaUrl.message(m.chatId, m.id, true);
    return (
      <div className={`flex ${m.mine ? "justify-end" : "justify-start"}`}>
        <div className="relative overflow-hidden rounded-md bg-incoming">
          <img src={src} alt="" className="block max-h-[120px] max-w-[160px] object-cover" draggable={false} />
          {m.contentType === "VIDEO" && (
            <span className="absolute right-1.5 bottom-1.5 rounded bg-black/50 px-1.5 py-0.5 text-[10px] text-white">วิดีโอ</span>
          )}
        </div>
      </div>
    );
  }
  return (
    <div
      className={`max-w-[88%] rounded-[16.5px] px-[13px] py-[7px] text-[13px] leading-[1.4] break-words whitespace-pre-wrap ${
        m.mine ? "self-end bg-outgoing text-outgoing-text" : "self-start bg-incoming text-primary"
      }`}
    >
      {peekLabel(m)}
    </div>
  );
}

export function ChatPeek({
  chat,
  anchor,
  onClose,
  onOpen,
}: {
  chat: Chat;
  /** List column element — peek sits just to its right. */
  anchor: HTMLElement | null;
  onClose: () => void;
  onOpen: () => void;
}) {
  const reduce = useReducedMotion();
  const session = useSession();
  const people = usePeople();
  const me = session?.profile?.mid ?? "";
  const keep = isKeepMemo(chat, me);
  const group = isGroup(chat);
  const unread = unreadBadgeCount(chat);
  const { data: messages = [], isPending, isError } = useQuery(q.messages(chat.id));
  const older = usePeekOlder(chat.id, messages.length);
  // Names in groups need the member list; force a load when peeking.
  const membersQuery = useQuery({ ...q.members(chat.id), enabled: group });
  useEffect(() => {
    if (group) membersQuery.refetch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat.id, group]);
  // Pull latest from LINE without open_chat / mark_as_read, so DMs and
  // groups that were never opened still have something to show.
  useEffect(() => {
    let cancelled = false;
    api.reloadChat(chat.id)
      .then(() => {
        if (!cancelled) return queryClient.refetchQueries({ queryKey: ["messages", chat.id] });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [chat.id]);
  const nameOf = useCallback(
    (mid: string) => {
      if (mid === me || mid === "me") return session?.profile?.displayName || "คุณ";
      return membersQuery.data?.find((m) => m.mid === mid)?.name ?? people[mid]?.name ?? "สมาชิก";
    },
    [me, session?.profile?.displayName, membersQuery.data, people],
  );
  const rows = useMemo(() => buildRows(messages, nameOf), [messages, nameOf]);

  const scroller = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const fromBottomBeforeOlder = useRef<number | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  // Drive exit with variants so Enter can fly into the chat pane.
  const [phase, setPhase] = useState<"enter" | "exitList" | "exitChat">("enter");
  const leaving = phase !== "enter";
  const requestClose = useCallback(() => setPhase("exitList"), []);
  const requestOpen = useCallback(() => setPhase("exitChat"), []);

  useLayoutEffect(() => {
    const place = () => {
      if (!anchor) {
        setPos({ left: 280, top: 72 });
        return;
      }
      const r = anchor.getBoundingClientRect();
      const width = 360;
      const height = 320;
      const left = Math.min(r.right + 8, window.innerWidth - width - 12);
      const top = Math.min(Math.max(56, r.top + 48), window.innerHeight - height - 16);
      setPos({ left, top });
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [anchor, chat.id]);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chat.id, isPending]);

  const loadOlder = useCallback(() => {
    const el = scroller.current;
    if (!el || older.state !== "idle" || !messages.length) return;
    fromBottomBeforeOlder.current = el.scrollHeight - el.scrollTop;
    older.load();
  }, [older.state, older.load, messages.length]);
  const loadOlderRef = useRef(loadOlder);
  loadOlderRef.current = loadOlder;
  const firstId = messages[0]?.id;
  useLayoutEffect(() => {
    const el = scroller.current;
    const kept = fromBottomBeforeOlder.current;
    if (!el || kept == null) return;
    fromBottomBeforeOlder.current = null;
    el.scrollTop = el.scrollHeight - kept;
  }, [firstId]);
  useEffect(() => {
    const el = scroller.current;
    if (el && older.state === "idle" && messages.length > 0 && el.scrollHeight <= el.clientHeight) loadOlder();
  }, [older.state, firstId, messages.length, loadOlder]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || leaving) return;
      if (e.key === "Escape" || e.code === "Space") {
        e.preventDefault();
        e.stopPropagation();
        requestClose();
      } else if (e.key === "Enter" && !e.isComposing) {
        e.preventDefault();
        e.stopPropagation();
        requestOpen();
      }
    };
    const onWheel = (e: WheelEvent) => {
      if (leaving) return;
      const el = scroller.current;
      if (!el) return;
      if (el.contains(e.target as Node)) return;
      e.preventDefault();
      el.scrollTop += e.deltaY;
      if (el.scrollTop < 80) loadOlderRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("wheel", onWheel, { passive: false, capture: true });
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("wheel", onWheel, true);
    };
  }, [leaving, requestClose, requestOpen]);

  const staggerFrom = Math.max(0, rows.length - 6);
  const variants = reduce ? cardVariantsReduce : cardVariants;

  if (!pos) return null;

  return createPortal(
    <>
      <motion.div
        aria-hidden
        className="pointer-events-none fixed inset-0 z-40 bg-black/20"
        initial={{ opacity: 0 }}
        animate={{ opacity: leaving ? 0 : 1 }}
        transition={{ duration: leaving ? 0.12 : 0.22, ease: leaving ? "easeIn" : EASE }}
      />
      <motion.div
        role="dialog"
        aria-label={`พรีวิวแชท ${keep ? "Keep Memo" : chat.name}`}
        className="fixed z-50 flex w-[360px] flex-col overflow-hidden rounded-[14px] border border-separator bg-popover shadow-[0_18px_48px_rgba(0,0,0,0.45),0_0_0_1px_rgba(6,199,85,0.18)]"
        style={{ left: pos.left, top: pos.top, height: 320, transformOrigin: "left center" }}
        onMouseDown={(e) => e.stopPropagation()}
        variants={variants}
        initial={reduce ? { opacity: 0 } : { opacity: 0, x: -16, scale: 0.94, filter: "blur(5px)" }}
        animate={phase}
        transition={
          phase === "enter"
            ? { duration: 0.3, ease: EASE }
            : phase === "exitChat"
              ? { duration: 0.18, ease: EASE }
              : { duration: 0.14, ease: "easeIn" }
        }
        onAnimationComplete={(def) => {
          if (def === "exitChat") onOpen();
          else if (def === "exitList") onClose();
        }}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-separator px-3 py-2.5">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.div
              key={chat.id}
              className="flex min-w-0 flex-1 items-center gap-2"
              initial={reduce ? false : { opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={reduce ? undefined : { opacity: 0, y: -4, transition: { duration: 0.12 } }}
              transition={{ duration: 0.18, ease: EASE }}
            >
              {keep ? (
                <div className="grid h-7 w-7 place-items-center rounded-full bg-line-green text-white">
                  <BookmarkIcon size={14} strokeWidth={2.2} />
                </div>
              ) : (
                <Avatar id={chat.id} name={chat.name} size={28} picture={chat.picture} local={chat.avatar} />
              )}
              <div className="min-w-0 flex-1 truncate text-[13px] font-semibold text-primary">
                {keep ? "Keep Memo" : chat.name}
                {group && chat.memberCount ? ` (${chat.memberCount})` : ""}
              </div>
            </motion.div>
          </AnimatePresence>
          {unread > 0 && (
            <motion.span
              key={`unread-${chat.id}`}
              className="shrink-0 rounded-full bg-line-green/15 px-2 py-0.5 text-[10px] font-bold text-line-green"
              initial={reduce ? false : { opacity: 0, scale: 0.85 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ duration: 0.28, ease: EASE, delay: reduce ? 0 : 0.12 }}
            >
              ยังไม่อ่าน · {unread > 999 ? "999+" : unread}
            </motion.span>
          )}
        </div>

        <div
          ref={scroller}
          className="relative min-h-0 flex-1 overflow-y-auto bg-chat px-3 py-2"
          onScroll={(e) => {
            if (e.currentTarget.scrollTop < 80) loadOlder();
          }}
        >
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={chat.id}
              ref={inner}
              className="flex min-h-full flex-col justify-end gap-1.5"
              initial={reduce ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={reduce ? undefined : { opacity: 0, transition: { duration: 0.1 } }}
              transition={{ duration: 0.16, ease: EASE }}
            >
              {older.state === "loading" && (
                <div className="flex justify-center py-1">
                  <Spinner size={14} />
                </div>
              )}
              {isPending ? (
                <div className="grid flex-1 place-items-center text-secondary">
                  <Spinner />
                </div>
              ) : isError ? (
                <div className="grid flex-1 place-items-center text-[12px] text-secondary">โหลดข้อความไม่ได้</div>
              ) : rows.length === 0 ? (
                <div className="grid flex-1 place-items-center text-[12px] text-secondary">ยังไม่มีข้อความ</div>
              ) : (
                rows.map((r, i) => {
                  const stagger = !reduce && i >= staggerFrom;
                  const delay = stagger ? (i - staggerFrom) * 0.035 : 0;
                  const body = r.notice ? (
                    <div className="px-2 py-1 text-center text-[11px] text-secondary">{noticeFor(r.message, nameOf)?.text}</div>
                  ) : (
                    <div className={`flex flex-col ${r.message.mine ? "items-end" : "items-start"}`}>
                      {group && r.runStart && !r.message.mine && r.message.senderId && (
                        <div className="mb-0.5 px-1 text-[11px] text-secondary">{nameOf(r.message.senderId)}</div>
                      )}
                      <PeekBubble m={r.message} />
                    </div>
                  );
                  if (!stagger) return <div key={r.message.id}>{body}</div>;
                  return (
                    <motion.div
                      key={r.message.id}
                      initial={{ opacity: 0, y: 8 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.22, ease: EASE, delay }}
                    >
                      {body}
                    </motion.div>
                  );
                })
              )}
            </motion.div>
          </AnimatePresence>
        </div>

        <div className="flex shrink-0 items-center justify-between border-t border-separator bg-chat px-3 py-2 text-[11px] text-secondary">
          <span>
            <kbd className="rounded border border-separator bg-field px-1.5 py-0.5 font-sans text-[10px] font-semibold text-primary">Space</kbd>{" "}
            เพื่อปิด
          </span>
          <span>
            เปิดแชท{" "}
            <kbd className="rounded border border-separator bg-field px-1.5 py-0.5 font-sans text-[10px] font-semibold text-primary">↵</kbd>
          </span>
        </div>
      </motion.div>
    </>,
    document.body,
  );
}
