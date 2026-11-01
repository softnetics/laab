import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { api, errorText, q, useChatList, usePeople, useSession } from "../lib/api";
import { isGroup, isKeepMemo } from "../lib/chatList";
import { PEEK_FLAT, peekClarity, peekOpen } from "../lib/peek";
import { openChat } from "../lib/nav";
import { phone, useWide } from "../lib/platform";
import { dismissTop, pushDismiss, setUi, ui, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { useNames, useOlder } from "./ChatView";
import { DuoModel } from "./DuoModel";
import { CloseIcon, EyeIcon } from "./Icons";
import type { RowContext } from "./MessageRow";
import { Sheet } from "./Sheet";
import { Timeline } from "./Timeline";

// A preview stays open while the pointer is over the chat list or the
// preview itself. Crossing the gap between them takes a moment, so leaving
// closes it only after a short wait.
let closing: ReturnType<typeof setTimeout> | undefined;

export function showPreview(chatId: string) {
  clearTimeout(closing);
  setUi({ preview: chatId });
}

export function stayInPreview() {
  clearTimeout(closing);
}

export function leavePreview() {
  clearTimeout(closing);
  if (ui.state.preview) closing = setTimeout(() => setUi({ preview: null }), 250);
}

/** The folded iPhone Duo's cover screen, where ดูโดยไม่อ่าน picks a chat to
 *  peek at instead of showing it. */
export const onCoverScreen = () =>
  phone &&
  // The inner screen in landscape has a vertical bar too; the cover is narrow.
  !window.matchMedia("(min-width: 600px)").matches &&
  (ui.state.hinge?.status === "closed" ||
    // No hinge reading yet (the simulator sends none until it first moves).
    (!ui.state.hinge && ui.state.sideBar != null));

export function pickPeek(chatId: string | null) {
  setUi({ peek: chatId });
}

/** With a preview open, hovering another chat previews that one instead. */
export function hoverPreview(chatId: string) {
  clearTimeout(closing);
  if (ui.state.preview && ui.state.preview !== chatId) setUi({ preview: chatId });
}

/** The previewed chat beside the list (a sheet on a phone). It shows the
 *  messages without opening the chat, so LINE gets no read receipt and the
 *  unread count stays. */
export function ChatPreview({ anchor }: { anchor: RefObject<HTMLElement | null> }) {
  const chatId = useUi((s) => s.preview);
  const wide = useWide();
  const open = chatId != null;
  const [box, setBox] = useState<DOMRect | null>(null);
  useLayoutEffect(() => {
    const el = anchor.current;
    if (!open || phone || !el) return;
    const measure = () => setBox(el.getBoundingClientRect());
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    window.addEventListener("resize", measure);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [open, anchor]);
  // Escape closes it like a popover, also with no chat open to catch the key.
  useEffect(() => {
    if (!open) return;
    const undo = pushDismiss(() => setUi({ preview: null }));
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing || !ui.state.preview) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      dismissTop();
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      undo();
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  if (!chatId) return null;
  // Two panes on a phone: PreviewPane shows it where the chat would open.
  if (phone && wide) return null;
  if (phone)
    return (
      <Sheet open onClose={() => setUi({ preview: null })} className="h-[78vh] w-[calc(100vw-24px)] bg-chat!">
        <PreviewBody chatId={chatId} />
      </Sheet>
    );
  if (!box) return null;
  const width = Math.min(440, window.innerWidth - box.right - 16);
  return createPortal(
    <div
      onMouseEnter={stayInPreview}
      onMouseLeave={leavePreview}
      className="fixed z-40 flex origin-left flex-col overflow-hidden rounded-xl bg-chat text-primary shadow-[0_12px_40px_rgba(0,0,0,0.3)] ring-1 ring-black/10 motion-safe:animate-ghost-in"
      style={{ left: box.right + 6, top: box.top + 6, width, height: Math.min(box.height - 12, 640) }}
    >
      <PreviewBody chatId={chatId} />
    </div>,
    document.body,
  );
}

/** The chat's picture and name, and that it is not marked read. */
function PreviewHeader({ chatId, children }: { chatId: string; children?: ReactNode }) {
  const list = useChatList();
  const chat = list?.chats.find((c) => c.id === chatId);
  const me = useSession()?.profile?.mid ?? "";
  const group = isGroup({ id: chatId, kind: chat?.kind ?? "" });
  const title = isKeepMemo({ id: chatId }, me) ? "Keep Memo" : (chat?.name ?? chatId);
  return (
    <div className="flex h-[52px] shrink-0 items-center gap-2.5 border-b border-separator pr-2 pl-3">
      <Avatar id={chatId} name={title} size={30} picture={chat?.picture} local={chat?.avatar} />
      <div className="min-w-0 flex-1 leading-tight">
        <div className="truncate text-[13.5px] font-semibold">
          {title}
          {group && chat?.memberCount ? ` (${chat.memberCount})` : ""}
        </div>
        <div className="flex items-center gap-1 text-[11px] text-secondary">
          <EyeIcon size={11} className="shrink-0" />
          <span className="truncate">ยังไม่ขึ้นว่าอ่านแล้ว</span>
        </div>
      </div>
      {children}
    </div>
  );
}

/** The chat over the long-press menu on a phone, drawn only while iOS takes
 *  a picture of it (see answerChatMenu). iOS rounds its corners. */
export function MenuCard() {
  const chatId = useUi((s) => s.menuCard);
  if (!chatId) return null;
  return createPortal(
    <div
      id="menu-card"
      className="fixed top-[calc(var(--safe-top)+8px)] left-1/2 z-[200] flex h-[min(44vh,420px)] w-[min(calc(100vw-24px),420px)] -translate-x-1/2 flex-col overflow-hidden bg-chat text-primary"
    >
      <PreviewHeader chatId={chatId} />
      <PreviewMessages chatId={chatId} />
    </div>,
    document.body,
  );
}

/** The previewed chat in the chat pane of a phone's two panes (the iPhone
 *  Duo opened), the size and place of the chat it stands in for. */
export function PreviewPane({ chatId }: { chatId: string }) {
  return (
    <div className="flex h-full flex-col bg-chat pb-[env(safe-area-inset-bottom)]">
      <div className="safe-top shrink-0" />
      <PreviewBody chatId={chatId} />
    </div>
  );
}

function PreviewBody({ chatId }: { chatId: string }) {
  return (
    <>
      <PreviewHeader chatId={chatId}>
        <button
          title="เปิดแชท จะขึ้นว่าอ่านแล้ว"
          onClick={() => openChat(chatId)}
          className="h-7 shrink-0 rounded-md bg-line-green px-3 text-[12px] font-medium text-white"
        >
          เปิดแชท
        </button>
        {phone && (
          <button title="ปิด" onClick={() => setUi({ preview: null })} className="grid h-9 w-9 place-items-center text-secondary">
            <CloseIcon size={18} />
          </button>
        )}
      </PreviewHeader>
      <PreviewMessages chatId={chatId} />
    </>
  );
}

/** The chat's messages, read only. */
function PreviewMessages({ chatId }: { chatId: string }) {
  const list = useChatList();
  const chat = list?.chats.find((c) => c.id === chatId);
  const session = useSession();
  const people = usePeople();
  const group = isGroup({ id: chatId, kind: chat?.kind ?? "" });
  const { data: messages = [], error, isPending } = useQuery(q.messages(chatId));
  const { data: reads = {} } = useQuery(q.reads(chatId));
  const { data: members } = useQuery({ ...q.members(chatId), enabled: group });
  const older = useOlder(chatId, messages.length);
  const { name, picture } = useNames(members);
  const me = session?.profile?.mid ?? "";
  // Fresh history from LINE. Unlike opening the chat, this sends no read.
  // Wait a little, so sweeping over the list does not fetch every chat.
  useEffect(() => {
    const t = setTimeout(() => api.reloadChat(chatId).catch(() => {}), 150);
    return () => clearTimeout(t);
  }, [chatId]);
  const byId = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages]);
  // Read only: no reactions or other actions that would show you were here.
  const ctx: RowContext = useMemo(
    () => ({ chatId, group, me: me || "me", signedIn: false, name, picture, loadMembers: () => {}, find: (id) => byId.get(id) }),
    [chatId, group, me, name, picture, byId],
  );
  return (
    <>
      {error && <div className="px-4 py-2 text-[12px] text-badge-red">โหลดข้อความไม่สำเร็จ: {errorText(error)}</div>}
      <Timeline
        messages={messages}
        loading={isPending}
        failed={!!error}
        older={older}
        reads={reads}
        ctx={ctx}
        members={members}
        people={people}
        search={null}
        follows={false}
      />
    </>
  );
}

/** The iPhone Duo opened a little after ดูโดยไม่อ่าน on its cover screen:
 *  the picked chat fills the inner screen, its name left of the fold and its
 *  messages right of it, revealed by the hinge angle. Opened flat, the chat
 *  opens for real and is read. */
export function PeekView() {
  const chatId = useUi((s) => s.peek);
  const hinge = useUi((s) => s.hinge);
  const fold = useUi((s) => s.fold);
  const wide = useWide();
  const list = useChatList();
  const me = useSession()?.profile?.mid ?? "";
  const flat = !!hinge && hinge.angle >= PEEK_FLAT;
  useEffect(() => {
    const id = ui.state.peek;
    if (flat && id) openChat(id, { focusComposer: false });
  }, [flat]);
  // The angle it first showed at, where the reveal starts.
  const from = useRef<number | null>(null);
  const shown = !!chatId && peekOpen(hinge, wide);
  if (!shown || !hinge) {
    from.current = null;
    return null;
  }
  from.current ??= hinge.angle;
  const chat = list?.chats.find((c) => c.id === chatId);
  const title = isKeepMemo({ id: chatId }, me) ? "Keep Memo" : (chat?.name ?? chatId);
  const clarity = peekClarity(hinge.angle, from.current);
  // Without a reported fold, split the screen in half.
  const left = fold ? fold.x : window.innerWidth / 2;
  const gap = fold ? fold.width : 0;
  return (
    <div className="fixed inset-0 z-50 bg-chat text-primary">
      <div
        className="flex h-full transition-[opacity,filter] duration-150 ease-out"
        style={{ opacity: 0.08 + 0.92 * clarity, filter: clarity < 0.98 ? `blur(${Math.round((1 - clarity) * 16)}px)` : undefined }}
      >
        <div
          className="safe-top safe-bottom flex shrink-0 flex-col items-center justify-center gap-3 px-6 text-center"
          style={{ width: left }}
        >
          <Avatar id={chatId} name={title} size={72} picture={chat?.picture} local={chat?.avatar} />
          <div className="text-[19px] font-semibold">{title}</div>
          <div className="flex items-center gap-1.5 text-[13px] text-secondary">
            <EyeIcon size={14} />
            ยังไม่ขึ้นว่าอ่านแล้ว
          </div>
          <div className="text-[12px] text-secondary">กางจนสุดเมื่อไหร่ จะขึ้นว่าอ่านแล้ว</div>
        </div>
        <div className="safe-top safe-bottom flex min-w-0 flex-1 flex-col" style={{ marginLeft: gap }}>
          <PreviewMessages chatId={chatId} />
        </div>
      </div>
    </div>
  );
}

/** On the cover screen after ดูโดยไม่อ่าน: how to peek, with the device
 *  drawn opening a little. */
export function PeekHint() {
  const chatId = useUi((s) => s.peek);
  const wide = useWide();
  const list = useChatList();
  if (!chatId || wide) return null;
  const chat = list?.chats.find((c) => c.id === chatId);
  const name = chat?.name ?? chatId;
  return (
    <div
      className="fixed z-40 rounded-3xl bg-popover px-4 pt-1 pb-3 text-center text-primary shadow-[0_10px_40px_rgba(0,0,0,0.25)] motion-safe:animate-row-in"
      style={{
        left: "max(12px, env(safe-area-inset-left))",
        right: "max(12px, env(safe-area-inset-right))",
        bottom: "calc(12px + env(safe-area-inset-bottom))",
      }}
    >
      <DuoModel chatId={chatId} />
      <div className="flex items-center justify-center gap-2">
        <Avatar id={chatId} name={name} size={26} picture={chat?.picture} local={chat?.avatar} />
        <span className="truncate text-[17px] font-semibold">{name}</span>
      </div>
      <div className="mt-1 text-[13px] leading-snug text-secondary">
        ค่อย ๆ กางจอครึ่งหนึ่งเพื่อแอบอ่าน
        <br />
        กางจนสุดเมื่อไหร่ จะขึ้นว่าอ่านแล้ว
      </div>
      <button onClick={() => pickPeek(null)} className="mt-1.5 h-9 px-4 text-[15px] text-line-green">
        ยกเลิก
      </button>
    </div>
  );
}
