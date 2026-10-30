import { useHotkey } from "@tanstack/react-hotkeys";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorText, useChatList, useLoginPage, useSession } from "../lib/api";
import { adjacentChat } from "../lib/chatList";
import { openChat } from "../lib/nav";
import { LAST_CHAT_KEY, notice, setUi, ui, useUi } from "../lib/ui";
import { foldChat, foldIfNarrow, unfoldChat } from "../lib/fold";
import { startUpdateChecks } from "../lib/updater";
import { ChatListColumn } from "./ChatList";
import { ChatView } from "./ChatView";
import { ConfirmDialog, Sheets } from "./Dialogs";
import { FolderDialog } from "./FolderDialog";
import { BubblesIcon, ChevronIcon } from "./Icons";
import { LiffConsent, SharePicker } from "./LiffDialogs";
import { AddFriendPage, FriendsPage } from "./ListPages";
import { LoginView } from "./LoginView";
import { Rail } from "./Rail";
import { ProfileSheet } from "./ProfileSheet";
import { RawInspector } from "./RawInspector";
import { SearchPalette } from "./SearchPalette";

const RAIL = 63;
const MIN_DETAIL = 360;

export function MainLayout({ chatId }: { chatId: string | null }) {
  const session = useSession();
  const loginPage = useLoginPage();
  const list = useChatList();
  const folded = useUi((s) => s.chatFolded);
  const frame = useUi((s) => s.foldFrame);
  const page = useUi((s) => s.railPage);
  const savedWidth = useUi((s) => s.listWidth);
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  // While the window slides open or shut, every animation frame is a resize;
  // re-rendering the layout for each one is what made it stutter. The panes
  // have fixed widths then, so catch up once the slide ends.
  useEffect(() => {
    const on = () => {
      if (!ui.state.foldFrame) setWindowWidth(window.innerWidth);
    };
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  // Before paint: with a plain effect, the first frame after the window
  // slid open could still use the folded width, and the chat list flashed
  // at its 240 px minimum.
  useLayoutEffect(() => {
    if (!frame) setWindowWidth(window.innerWidth);
  }, [frame]);
  const width = Math.min(Math.max(240, savedWidth), Math.max(240, windowWidth - RAIL - MIN_DETAIL));

  // Reopen the last chat on launch (demo opens the team chat).
  const restored = useRef(false);
  useEffect(() => {
    if (restored.current || !list || chatId) return;
    restored.current = true;
    const last = [localStorage.getItem(LAST_CHAT_KEY), session?.state === "demo" ? "c_harvest" : null].find(
      (id) => id && list.chats.some((c) => c.id === id),
    );
    if (last) openChat(last, { focusComposer: false });
  }, [list, chatId, session?.state]);

  useEffect(startUpdateChecks, []);
  useEffect(foldIfNarrow, []);

  // Banners and Raycast deep links open a chat.
  useEffect(() => {
    const off = listen<{ chatId: string; messageId?: string | null }>("open-chat", (e) => {
      if (ui.state.searchOpen) setUi({ searchOpen: false });
      openChat(e.payload.chatId, { messageId: e.payload.messageId ?? null });
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  useHotkey("Mod+K", () => setUi({ searchOpen: true, searchInitial: "" }));
  useHotkey("Mod+R", () => api.refresh(true).catch((e) => notice(errorText(e))));
  // ⌥⌘L: LIFF DevTools. By key code, since ⌥L types "¬".
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code === "KeyL" && e.metaKey && e.altKey && !e.shiftKey && !e.ctrlKey) {
        e.preventDefault();
        api.openLiffDevtools().catch((err) => notice(errorText(err)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  // ⌥↑ / ⌥↓ walk the chat list in its visible order.
  useHotkey(
    "Alt+ArrowUp",
    () => {
      const next = adjacentChat(ui.state.visibleChatIds, chatId, -1);
      if (next) openChat(next, { focusComposer: false });
    },
    { ignoreInputs: false },
  );
  useHotkey(
    "Alt+ArrowDown",
    () => {
      const next = adjacentChat(ui.state.visibleChatIds, chatId, 1);
      if (next) openChat(next, { focusComposer: false });
    },
    { ignoreInputs: false },
  );

  if (loginPage) {
    return (
      <div className="drag h-full" data-tauri-drag-region>
        <LoginView />
      </div>
    );
  }
  return (
    <div className="flex h-full overflow-hidden">
      <Rail />
      <div className="w-px bg-separator" />
      {/* No width animation: it reflowed the timeline every frame and made
          drag-resizing chase the cursor. */}
      <div
        style={folded && !frame ? undefined : { width: frame?.list ?? width }}
        className={`relative h-full overflow-hidden ${folded && !frame ? "min-w-0 flex-1" : "shrink-0"} ${frame ? "[contain:strict]" : ""}`}
      >
        {page === "friends" ? <FriendsPage /> : page === "add" ? <AddFriendPage /> : <ChatListColumn selected={chatId} />}
      </div>
      <div className="relative w-px shrink-0 bg-separator">
        {!folded && !frame && <ResizeHandle width={width} />}
        <button
          title={folded ? "แสดงแชท" : "ซ่อนแชท"}
          disabled={frame != null}
          onClick={() => (folded ? unfoldChat() : foldChat(RAIL + 2, width)).catch((e) => notice(errorText(e)))}
          className="absolute top-[70px] -left-3.5 z-20 grid h-9 w-3.5 place-items-center rounded-l-md bg-separator/70 text-secondary hover:bg-separator"
        >
          <ChevronIcon dir={folded ? "left" : "right"} size={10} />
        </button>
      </div>
      <main
        className={folded ? "hidden" : frame ? "h-full shrink-0 [contain:strict]" : "min-w-0 flex-1"}
        style={frame && !folded ? { width: frame.chat } : undefined}
      >
        {chatId ? (
          <ChatView key={chatId} chatId={chatId} main />
        ) : (
          <div className="drag flex h-full flex-col items-center justify-center gap-3 text-secondary" data-tauri-drag-region>
            <BubblesIcon size={52} />
            <span className="text-[14px]">เลือกแชทเพื่อเริ่มสนทนา</span>
          </div>
        )}
      </main>
      <SearchPalette />
      <ConfirmDialog />
      <Sheets />
      <FolderDialog />
      <ProfileSheet main />
      <LiffConsent />
      <SharePicker />
      {chatId && <RawInspector chatId={chatId} />}
    </div>
  );
}

/** Drag to resize the chat list; double-click resets it. One update
 *  per frame while dragging; the width is saved once, on release. */
function ResizeHandle({ width }: { width: number }) {
  const start = useRef<{ x: number; w: number } | null>(null);
  const frame = useRef(0);
  return (
    <div
      className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize"
      onDoubleClick={() => setUi({ listWidth: 320 })}
      onPointerDown={(e) => {
        start.current = { x: e.clientX, w: width };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const s = start.current;
        if (!s) return;
        const x = e.clientX;
        cancelAnimationFrame(frame.current);
        frame.current = requestAnimationFrame(() => {
          const max = Math.max(240, window.innerWidth - RAIL - MIN_DETAIL);
          ui.setState((u) => ({ ...u, listWidth: Math.min(max, Math.max(240, s.w + x - s.x)) }));
        });
      }}
      onPointerUp={() => {
        if (!start.current) return;
        start.current = null;
        cancelAnimationFrame(frame.current);
        setUi({ listWidth: ui.state.listWidth });
      }}
    />
  );
}
