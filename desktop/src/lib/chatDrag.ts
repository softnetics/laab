import { createStore, useStore } from "@tanstack/react-store";
import { api, errorText } from "./api";
import type { Chat } from "./types";
import { notice, setUi } from "./ui";

/** Where a dragged chat goes: into a new folder with another chat, into a
 *  folder, or out of its folder back to the plain list. */
export type DropTarget = { kind: "merge"; chatId: string } | { kind: "folder"; id: string } | { kind: "out" };

export interface ChatDrag {
  chat: Chat;
  /** The folder it is dragged out of. */
  from: string | null;
  x: number;
  y: number;
  target: DropTarget | null;
}

export const chatDrag = createStore<ChatDrag | null>(null);
export const useChatDrag = <T>(select: (d: ChatDrag | null) => T) => useStore(chatDrag, select);

/** The folder the user just opened, made or dropped into. For a moment
 *  rows slide to their new places and its rows fade in; rows that mount by
 *  scrolling, or move for a new message, do not animate. */
let recent: { folder: string; at: number } | null = null;
export function markRecent(folder: string) {
  recent = { folder, at: Date.now() };
}
export const moving = () => !!recent && Date.now() - recent.at < 400;
export const isRecent = (folder: string | null) => !!folder && moving() && recent?.folder === folder;

let dropped = false;
/** True for the click that ends a drag, which must not open the chat. */
export function clickEndsDrag() {
  return dropped;
}

/** Rows mark themselves `data-drop="chat|folder|list"` with `data-id` and,
 *  for a chat inside an open folder, `data-folder`. */
function targetAt(x: number, y: number, chatId: string, from: string | null): DropTarget | null {
  const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  const { drop, id, folder } = el?.dataset ?? {};
  if (drop === "list") return from ? { kind: "out" } : null;
  if (!id || id === chatId) return null;
  if (drop === "folder" || folder) {
    const f = folder ?? id;
    return f === from ? null : { kind: "folder", id: f };
  }
  return drop === "chat" ? { kind: "merge", chatId: id } : null;
}

function apply({ chat, from, target }: ChatDrag) {
  const done =
    target?.kind === "merge"
      ? api.createFolder("", [chat.id, target.chatId], from).then((w) => {
          markRecent(w.id);
          setUi({ renamingFolder: w.id });
        })
      : target?.kind === "folder"
        ? (markRecent(target.id), api.moveToFolder(chat.id, from, target.id))
        : (markRecent(""), api.moveToFolder(chat.id, from, null));
  done.catch((e) => notice(errorText(e)));
}

/** A press on a chat row becomes a drag once the pointer moves a few
 *  pixels. Near the list's top or bottom edge the list scrolls. */
export function pressChat(e: React.PointerEvent, chat: Chat, from: string | null, scroller: HTMLElement | null) {
  if (e.button !== 0) return;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let frame = 0;
  const retarget = (x: number, y: number) =>
    chatDrag.setState(() => ({ chat, from, x, y, target: targetAt(x, y, chat.id, from) }));
  const scroll = () => {
    frame = 0;
    const d = chatDrag.state;
    if (!d || !scroller) return;
    const r = scroller.getBoundingClientRect();
    const edge = 48;
    const over = d.y < r.top + edge ? d.y - (r.top + edge) : d.y > r.bottom - edge ? d.y - (r.bottom - edge) : 0;
    if (!over) return;
    scroller.scrollTop += Math.max(-1, Math.min(1, over / edge)) * 14;
    retarget(d.x, d.y);
    frame = requestAnimationFrame(scroll);
  };
  const move = (ev: PointerEvent) => {
    if (!chatDrag.state && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
    retarget(ev.clientX, ev.clientY);
    if (!frame) frame = requestAnimationFrame(scroll);
  };
  const end = (drop: boolean) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", key, true);
    cancelAnimationFrame(frame);
    const d = chatDrag.state;
    chatDrag.setState(() => null);
    if (!d) return;
    dropped = true;
    setTimeout(() => (dropped = false));
    if (drop && d.target) apply(d);
  };
  const up = () => end(true);
  const cancel = () => end(false);
  const key = (ev: KeyboardEvent) => {
    if (ev.key !== "Escape" || !chatDrag.state) return;
    ev.stopPropagation();
    end(false);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", key, true);
}
