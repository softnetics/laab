import { Menu } from "@base-ui/react/menu";
import { useQuery } from "@tanstack/react-query";
import { useHotkey } from "@tanstack/react-hotkeys";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText, messageLimit, q, queryClient, setMessageLimit, useChatList, usePeople, useSession } from "../lib/api";
import { adjacentChat, isGroup, isKeepMemo } from "../lib/chatList";
import { findHits } from "../lib/inChatSearch";
import { closeChat, openChat } from "../lib/nav";
import { dismissTop, notice, ui } from "../lib/ui";
import { Announcements } from "./Announcements";
import { HeaderMenu } from "./ChatActions";
import { Composer, type ComposerHandle } from "./Composer";
import { BellSlashIcon, ChevronIcon, CloseIcon, ExternalIcon, MoreVIcon, NoteIcon, SearchIcon, SpeakerIcon } from "./Icons";
import { popupClass } from "./kit";
import type { RowContext } from "./MessageRow";
import { StickerPanel } from "./StickerPanel";
import { type Older, Timeline } from "./Timeline";

/** Something else (a menu or dialog) is open and owns Escape. */
const overlayOpen = () => !!document.querySelector('[role="menu"], [role="dialog"][data-open], [role="alertdialog"]');

/** Scrolling to the top shows more of the stored messages, then asks LINE
 *  for the page before them, 100 at a time. */
function useOlder(chatId: string, count: number): Older {
  const [state, setState] = useState<Older["state"]>("idle");
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

export function ChatView({ chatId, main }: { chatId: string; main: boolean }) {
  const list = useChatList();
  const chat = list?.chats.find((c) => c.id === chatId);
  const session = useSession();
  const people = usePeople();
  const group = isGroup({ id: chatId, kind: chat?.kind ?? "" });
  const { data: messages = [], error: loadError, isPending: loadingMessages } = useQuery(q.messages(chatId));
  const older = useOlder(chatId, messages.length);
  const { data: reads = {} } = useQuery(q.reads(chatId));
  const membersQuery = useQuery({ ...q.members(chatId), enabled: false });
  const members = membersQuery.data;
  const loadMembers = useCallback(() => {
    if (group && !membersQuery.data && !membersQuery.isFetching) membersQuery.refetch();
  }, [group, membersQuery]);
  const composer = useRef<ComposerHandle>(null);
  const [stickers, setStickers] = useState(false);
  const [search, setSearch] = useState<{ open: boolean; query: string; debounced: string; index: number }>({
    open: false,
    query: "",
    debounced: "",
    index: 0,
  });
  const [dragging, setDragging] = useState(false);
  const me = session?.profile?.mid ?? "";
  const signedIn = session?.state === "signed_in";

  // Tell the backend what is on screen right away; fetch after the user
  // settles on a chat for 150 ms, so arrow-key browsing stays fast.
  useEffect(() => {
    if (main) api.setFocus(chatId, document.hasFocus());
    const t = setTimeout(() => api.openChat(chatId).catch(() => {}), 150);
    setStickers(false);
    setSearch({ open: false, query: "", debounced: "", index: 0 });
    return () => clearTimeout(t);
  }, [chatId, main]);
  useEffect(() => {
    if (!main) return;
    const off = listen<boolean>("app-active", (e) => {
      api.setFocus(chatId, e.payload);
      if (e.payload) api.markRead(chatId).catch(() => {});
    });
    return () => {
      off.then((f) => f());
      api.setFocus(null, document.hasFocus());
    };
  }, [chatId, main]);

  // Members are needed for names in groups.
  useEffect(() => {
    if (group) loadMembers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId, group]);

  // The chat list changes with every message in any chat. Read it through a
  // ref so that does not rebuild `ctx` and re-render every row here; a
  // renamed friend shows up with the next change to this chat.
  const listRef = useRef(list);
  listRef.current = list;
  const name = useCallback(
    (mid: string) => {
      if (mid === me || mid === "me") return session?.profile?.displayName || "คุณ";
      const c = listRef.current?.chats.find((x) => x.id === mid && x.name !== x.id);
      return c?.name ?? members?.find((m) => m.mid === mid)?.name ?? people[mid]?.name ?? "สมาชิก";
    },
    [me, session?.profile?.displayName, members, people],
  );
  const picture = useCallback(
    (mid: string) => members?.find((m) => m.mid === mid)?.picture ?? people[mid]?.picture ?? null,
    [members, people],
  );
  // Letter Sealing: LINE marks encrypted messages with e2eeVersion.
  const secure = useMemo(() => messages.some((m) => "e2eeVersion" in m.metadata), [messages]);
  const byId = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages]);
  const find = useCallback((id: string) => byId.get(id), [byId]);
  const ctx: RowContext = useMemo(
    () => ({ chatId, group, me: me || "me", signedIn: signedIn || session?.state === "demo", name, picture, loadMembers, find }),
    [chatId, group, me, signedIn, session?.state, name, picture, loadMembers, find],
  );

  // Debounce, newest hit first.
  useEffect(() => {
    const t = setTimeout(() => setSearch((s) => ({ ...s, debounced: s.query, index: 0 })), 200);
    return () => clearTimeout(t);
  }, [search.query]);
  const hits = useMemo(() => (search.open ? findHits(messages, search.debounced) : []), [messages, search.open, search.debounced]);
  const step = (dir: 1 | -1) => setSearch((s) => ({ ...s, index: Math.max(0, Math.min(hits.length - 1, s.index + dir)) }));
  const searchInput = useRef<HTMLInputElement>(null);
  const openSearch = () => {
    setSearch((s) => ({ ...s, open: true }));
    requestAnimationFrame(() => searchInput.current?.select());
  };
  const closeSearch = () => {
    setSearch({ open: false, query: "", debounced: "", index: 0 });
    composer.current?.focus();
  };

  useHotkey("Mod+F", openSearch);
  useHotkey("Mod+G", () => search.open && step(1));
  useHotkey("Mod+Shift+G", () => search.open && step(-1));

  // Escape closes the newest thing first: popovers, the mention
  // picker, in-chat search, stickers, then the chat itself (main window only).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing || e.metaKey || e.altKey || e.ctrlKey || e.shiftKey) return;
      if (dismissTop()) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (overlayOpen()) return;
      e.preventDefault();
      if (composer.current?.dismissPicker()) return;
      if (search.open) return closeSearch();
      if (stickers) return setStickers(false);
      if (main) closeChat();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  // Files dropped on the chat are sent one by one.
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over" || e.payload.type === "enter") setDragging(true);
      else if (e.payload.type === "leave") setDragging(false);
      else if (e.payload.type === "drop") {
        setDragging(false);
        if (e.payload.paths.length) api.sendFiles(chatId, e.payload.paths).catch((err) => notice(errorText(err)));
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [chatId]);

  useEffect(() => {
    if (!main && chat)
      getCurrentWindow()
        .setTitle(chat.name)
        .catch(() => {});
  }, [main, chat]);

  const title = chat?.name ?? chatId;
  return (
    <div className="relative flex h-full flex-col bg-chat">
      {/* LINE for Mac: the header sits under a title-bar strip, level with
          the chat list's search box. */}
      <div className="drag h-[56px] shrink-0" data-tauri-drag-region />
      <header className="drag flex h-[56px] shrink-0 items-center gap-1.5 px-4" data-tauri-drag-region>
        <h2 className="truncate text-[15px] font-semibold">{isKeepMemo({ id: chatId }, me) ? "Keep Memo" : title}</h2>
        {group && chat?.memberCount ? <span className="text-[15px] font-semibold">({chat.memberCount})</span> : null}
        {chat && (
          <button
            title={chat.muted ? "เปิดแจ้งเตือน" : "ปิดแจ้งเตือน"}
            onClick={() => api.setMuted(chatId, !chat.muted).catch((e) => notice(errorText(e)))}
            className="no-drag grid h-6 w-6 place-items-center text-secondary hover:text-primary"
          >
            {chat.muted ? <BellSlashIcon size={13} /> : <SpeakerIcon size={15} />}
          </button>
        )}
        {main && (
          <button
            title="เปิดในหน้าต่างใหม่"
            onClick={() => api.openChatWindow(chatId, title)}
            className="no-drag grid h-6 w-6 place-items-center text-secondary hover:text-primary"
          >
            <ExternalIcon size={15} />
          </button>
        )}
        <div className="flex-1" />
        <button
          title="ค้นหาในแชท ⌘F"
          onClick={openSearch}
          className="no-drag grid h-8 w-8 place-items-center text-secondary hover:text-primary"
        >
          <SearchIcon size={18} />
        </button>
        {group && (
          <button
            title="โน้ต"
            onClick={() => api.openAuxWindow("albums", chatId, title, "notes").catch((e) => notice(errorText(e)))}
            className="no-drag grid h-8 w-8 place-items-center text-secondary hover:text-primary"
          >
            <NoteIcon size={19} />
          </button>
        )}
        {chat && (
          <Menu.Root>
            <Menu.Trigger aria-label="เมนูแชท" className="no-drag grid h-8 w-6 place-items-center text-secondary hover:text-primary">
              <MoreVIcon size={18} />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner className="z-50" align="end" sideOffset={4}>
                <Menu.Popup className={popupClass}>
                  <HeaderMenu chat={chat} secure={secure} />
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
        )}
      </header>
      {group && !search.open && <Announcements chatId={chatId} name={name} />}
      {search.open && (
        // LINE for Mac: a bordered field with the count and arrows inside,
        // then close, in the announcement bar's place. Opened with ⌘F, so it
        // appears at once.
        <div className="flex h-12 shrink-0 items-center gap-2 overflow-hidden px-4 shadow-[0_2px_4px_rgba(0,0,0,0.15)]">
          <label className="flex h-[30px] flex-1 items-center gap-2 rounded-[6px] border border-secondary/45 px-2.5">
            <SearchIcon size={14} className="shrink-0 text-secondary" />
            <input
              ref={searchInput}
              autoFocus
              value={search.query}
              onChange={(e) => setSearch((s) => ({ ...s, query: e.target.value }))}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  step(e.shiftKey ? -1 : 1);
                }
              }}
              placeholder="ค้นหาข้อความ, ชื่อ"
              className="selectable min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-secondary"
            />
            <span className="shrink-0 text-[12.5px] text-secondary">
              {search.debounced.trim() ? (hits.length ? `${search.index + 1}/${hits.length}` : "ไม่พบ") : ""}
            </span>
            <button
              title="ผลถัดไป (เก่ากว่า) ⌘G"
              onClick={() => step(1)}
              className="grid h-6 w-5 place-items-center text-secondary hover:text-primary"
            >
              <ChevronIcon dir="up" size={13} />
            </button>
            <button
              title="ผลก่อนหน้า (ใหม่กว่า) ⇧⌘G"
              onClick={() => step(-1)}
              className="grid h-6 w-5 place-items-center text-secondary hover:text-primary"
            >
              <ChevronIcon dir="down" size={13} />
            </button>
          </label>
          <button title="ปิด (Esc)" onClick={closeSearch} className="grid h-7 w-7 place-items-center text-secondary hover:text-primary">
            <CloseIcon size={18} />
          </button>
        </div>
      )}
      {loadError && <div className="px-5 py-2 text-[12px] text-badge-red">โหลดข้อความไม่สำเร็จ: {errorText(loadError)}</div>}
      <Timeline
        messages={messages}
        loading={loadingMessages}
        failed={!!loadError}
        older={older}
        reads={reads}
        ctx={ctx}
        members={members}
        people={people}
        search={search.open && search.debounced.trim() ? { query: search.debounced, hits, index: search.index } : null}
      />
      {stickers && <StickerPanel chatId={chatId} onClose={() => setStickers(false)} />}
      <Composer
        ref={composer}
        chatId={chatId}
        group={group}
        disabled={!(signedIn || session?.state === "demo")}
        members={members}
        loadMembers={loadMembers}
        stickerOpen={stickers}
        onToggleStickers={() => setStickers((v) => !v)}
        name={name}
        picture={picture}
        onStep={
          main
            ? (step) => {
                const next = adjacentChat(ui.state.visibleChatIds, chatId, step);
                if (next) openChat(next);
              }
            : undefined
        }
      />
      <AnimatePresence>
        {dragging && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="pointer-events-none absolute inset-1.5 rounded-xl border-[3px] border-line-green bg-line-green/5"
          />
        )}
      </AnimatePresence>
    </div>
  );
}
