import { listen } from "@tauri-apps/api/event";
import { onAction } from "@tauri-apps/plugin-notification";
import { AnimatePresence, motion, useDragControls } from "motion/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText, q, useChatList, useLoginPage, useSession } from "../lib/api";
import { isGroup, unreadTotal } from "../lib/chatList";
import { closeChat, openChat } from "../lib/nav";
import { findEntry, type MenuEntry, type SideButton, useNativeTabbar } from "../lib/nativeTabbar";
import { peekOpen } from "../lib/peek";
import { ios, useWide } from "../lib/platform";
import { LAST_CHAT_KEY, notice, pushDismiss, setUi, ui, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { ChatListColumn } from "./ChatList";
import { useChatMenu, useNativeChatMenu } from "./ChatActions";
import { MenuCard, PeekHint, PeekView, PreviewPane } from "./ChatPreview";
import { ChatView } from "./ChatView";
import { ConfirmDialog, Sheets } from "./Dialogs";
import { FileViewer } from "./FileViewer";
import { FolderDialog } from "./FolderDialog";
import { BookmarkIcon, BubbleIcon, BubblesIcon, MoreIcon, PersonIcon, PersonPlusIcon } from "./Icons";
import { LiffConsent, SharePicker } from "./LiffDialogs";
import { AddFriendPage, FriendsPage } from "./ListPages";
import { LoginView } from "./LoginView";
import { PhoneHeader } from "./PhoneHeader";
import { ProfileSheet } from "./ProfileSheet";
import { RawInspector } from "./RawInspector";
import { SearchDock } from "./SearchDock";
import { SearchPalette } from "./SearchPalette";
import { Viewer } from "./Viewer";

/** LAAB on a phone, laid out like LINE for iPhone. One pane on narrow
 *  screens (the iPhone Duo's cover screen): the tabs, and a chat that slides
 *  over them. Two panes on wide ones (the Duo unfolded): the tabs on the left,
 *  the chat on the right. Folding or unfolding keeps the open chat. */
export function PhoneLayout({ chatId }: { chatId: string | null }) {
  const session = useSession();
  const list = useChatList();
  const wide = useWide();
  const loginPage = useLoginPage();
  useSystemTabBar({ chatId, signedOut: loginPage, wide });
  useNativeChatMenu();

  // Unfolded, reopen the last chat as the desktop does; the cover screen
  // starts on the chat list, as LINE does.
  const restored = useRef(false);
  useEffect(() => {
    if (restored.current || !list || chatId || !wide) return;
    restored.current = true;
    // Unfolded to peek at a chat: open nothing, so nothing is marked read.
    if (ui.state.peek) return;
    const last = localStorage.getItem(LAST_CHAT_KEY) ?? (session?.state === "demo" ? "c_team" : null);
    if (last && list.chats.some((c) => c.id === last)) openChat(last, { focusComposer: false });
  }, [list, chatId, wide, session?.state]);

  // Deep links, and tapping a notification, open its chat.
  useEffect(() => {
    const off = listen<{ chatId: string; messageId?: string | null }>("open-chat", (e) => {
      if (ui.state.searchOpen) setUi({ searchOpen: false });
      openChat(e.payload.chatId, { messageId: e.payload.messageId ?? null, focusComposer: false });
    });
    const tap = onAction((n) => {
      const id = n.notification?.extra?.chatId;
      if (typeof id === "string") openChat(id, { focusComposer: false });
    }).catch(() => null);
    // iOS suspends LAAB in the background, so catch up when it comes back.
    const back = listen<boolean>("app-active", (e) => {
      if (e.payload) api.refresh(false).catch(() => {});
    });
    return () => {
      off.then((f) => f());
      tap.then((l) => l?.unregister());
      back.then((f) => f());
    };
  }, []);

  const overlays = (
    <>
      <SearchPalette />
      <PeekView />
      <PeekHint />
      <MenuCard />
      <ConfirmDialog />
      <Sheets />
      <FolderDialog />
      <LiffConsent />
      <SharePicker />
      <ProfileSheet main />
      <Gallery />
      <FileViewer />
      {chatId && <RawInspector chatId={chatId} />}
    </>
  );

  const searchDock = useUi((s) => !!s.searchDock);
  const preview = useUi((s) => s.preview);
  // Half-open like a book, the list ends at the fold and the chat starts
  // after it, so neither runs into the crease.
  const fold = useUi((s) => s.fold);
  const book = wide && fold && fold.height > fold.width ? fold : null;

  if (loginPage) {
    return (
      <div className="safe-top safe-bottom safe-x h-full bg-list">
        <LoginView />
        {overlays}
      </div>
    );
  }

  if (wide) {
    return (
      <div className="safe-x flex h-full bg-chat">
        <div
          className="h-full shrink-0 border-r border-separator"
          style={{ width: book ? `calc(${book.x}px - env(safe-area-inset-left))` : "clamp(280px, 42vw, 360px)" }}
        >
          {searchDock ? <SearchDock /> : <Home selected={chatId} />}
        </div>
        <main className="min-w-0 flex-1" style={book ? { paddingLeft: book.width } : undefined}>
          {preview ? (
            <PreviewPane key={preview} chatId={preview} />
          ) : chatId ? (
            <ChatView key={chatId} chatId={chatId} main />
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-3 text-secondary">
              <BubblesIcon size={52} />
              <span className="text-[15px]">เลือกแชทเพื่อเริ่มสนทนา</span>
            </div>
          )}
        </main>
        {overlays}
      </div>
    );
  }

  return (
    <div className="safe-x h-full bg-list">
      <div className="relative h-full overflow-hidden">
        <Home selected={null} />
        <AnimatePresence initial={false}>{chatId && <PushedChat key="chat" chatId={chatId} />}</AnimatePresence>
      </div>
      {overlays}
    </div>
  );
}

/** The chat over the tabs. It slides in from the right, and a swipe from the
 *  left edge takes it back, as an iPhone navigation stack does. */
function PushedChat({ chatId }: { chatId: string }) {
  const drag = useDragControls();
  return (
    <motion.div
      initial={{ x: "100%" }}
      animate={{ x: 0 }}
      exit={{ x: "100%" }}
      transition={{ type: "spring", duration: 0.38, bounce: 0 }}
      drag="x"
      dragControls={drag}
      dragListener={false}
      dragConstraints={{ left: 0, right: 0 }}
      dragElastic={{ left: 0, right: 1 }}
      dragMomentum={false}
      onDragEnd={(_, info) => {
        if (info.offset.x > window.innerWidth * 0.33 || info.velocity.x > 600) closeChat();
      }}
      className="absolute inset-0 z-10 shadow-[-8px_0_24px_rgba(0,0,0,0.18)]"
    >
      <ChatView key={chatId} chatId={chatId} main />
      {/* Below the header, so the back button stays tappable. */}
      <div
        className="absolute bottom-0 left-0 z-20 w-4 touch-none"
        style={{ top: "calc(var(--safe-top) + 56px)" }}
        onPointerDown={(e) => drag.start(e)}
      />
    </motion.div>
  );
}

/** The tab pages and LINE's bottom tab bar. */
function Home({ selected }: { selected: string | null }) {
  const page = useUi((s) => s.railPage);
  return (
    <div className="flex h-full flex-col bg-list">
      <div className="min-h-0 flex-1">
        {page === "friends" ? (
          <FriendsPage />
        ) : page === "add" ? (
          <AddFriendPage />
        ) : page === "more" ? (
          <MorePage />
        ) : (
          <ChatListColumn selected={selected} />
        )}
      </div>
      {!ios && <TabBar />}
    </div>
  );
}

const TABS = [
  { id: "friends", title: "เพื่อน", icon: "person", selectedIcon: "person.fill" },
  { id: "chats", title: "แชท", icon: "bubble.left", selectedIcon: "bubble.left.fill" },
  { id: "add", title: "เพิ่มเพื่อน", icon: "person.badge.plus", selectedIcon: "person.fill.badge.plus" },
  { id: "more", title: "ตั้งค่า", icon: "gearshape", selectedIcon: "gearshape.fill" },
] as const;

/** On iOS the bars are the system's own: a tab bar, and the chat's buttons as
 *  bar button items, with Liquid Glass on iOS 26+. Where iOS has a vertical
 *  bar (the iPhone Duo) it puts them all in that column, the chat's menu in
 *  its own overflow menu; elsewhere the tabs are a tab bar at the bottom and
 *  the page's headers keep their buttons. The bars step aside for anything
 *  drawn over the page, which they would otherwise cover. */
function useSystemTabBar({ chatId, signedOut, wide }: { chatId: string | null; signedOut: boolean; wide: boolean }) {
  const page = useUi((s) => s.railPage);
  const list = useChatList();
  const unread = unreadTotal(list);
  const chat = list?.chats.find((c) => c.id === chatId);
  const peeking = useUi((s) => !!s.peek && peekOpen(s.hinge, wide));
  const covered = useUi(
    (s) =>
      !!(
        s.viewer ||
        s.gallery ||
        s.profile ||
        s.searchOpen ||
        s.confirm ||
        s.sheet ||
        s.folderDialog ||
        s.liffConsent ||
        s.sharePicker ||
        s.inspectMessageId ||
        // Two panes show it in the chat pane, not over the bars.
        (s.preview && !wide)
      ),
  );
  const pushed = !wide && !!chatId;
  const badge = unread > 0 ? (unread > 999 ? "999+" : String(unread)) : null;
  const tabs = TABS.map((t) => ({ ...t, badge: t.id === "chats" ? badge : null }));
  const chatButtons: SideButton[] = chatId
    ? [
        { id: "search", icon: "text.magnifyingglass" },
        ...(chat && isGroup(chat) ? [{ id: "notes", icon: "note.text" }] : []),
      ]
    : [];
  // Letter Sealing: LINE marks encrypted messages with e2eeVersion.
  const { data: messages } = useQuery({ ...q.messages(chatId ?? ""), enabled: !!chatId });
  const secure = !!messages?.some((m) => "e2eeVersion" in m.metadata);
  const chatMenu = useChatMenu(chat);
  const menu: MenuEntry[][] = chatId
    ? [
        ...chatMenu,
        ...(secure
          ? [[{ id: "secure", title: "ห้องแชทนี้ปลอดภัยด้วย Letter Sealing", icon: "lock", disabled: true }]]
          : []),
      ]
    : [];
  const tabGroup: SideButton[] = tabs.map((t) => ({
    id: `tab:${t.id}`,
    icon: t.icon,
    selectedIcon: t.selectedIcon,
    selected: t.id === page,
    badge: t.badge,
  }));
  // ⌘K: chats, messages and files. A magnifying glass, not a pencil: it
  // searches more than it starts chats.
  const compose: SideButton[] = [{ id: "compose", icon: "magnifyingglass" }];
  const side = pushed
    ? { top: [[{ id: "back", icon: "chevron.left", badge }]], bottom: [chatButtons] }
    : { top: [compose], bottom: [chatButtons, tabGroup] };

  useNativeTabbar(
    ios
      ? {
          tabs,
          selected: page,
          hidden: signedOut || covered || peeking,
          // The bottom bar leaves for a pushed chat; the vertical bar stays
          // and shows the chat's buttons instead.
          bottomHidden: pushed,
          side,
          menu,
        }
      : null,
    {
      // A tab leaves docked search results for that tab's page.
      onSelect: (id) => setUi({ railPage: id as typeof page, searchDock: null }),
      onAction: (id) => {
        if (id.startsWith("tab:")) setUi({ railPage: id.slice(4) as typeof page, searchDock: null });
        else if (id === "back") closeChat();
        else if (id === "search") setUi({ chatSearch: Date.now() });
        else if (id.startsWith("menu:")) findEntry(menu, id.slice(5))?.run?.();
        else if (id === "compose") setUi({ searchOpen: true, searchInitial: "" });
        else if (id === "notes" && chatId)
          api.openAuxWindow("albums", chatId, chat?.name ?? "", "notes").catch((e) => notice(errorText(e)));
      },
    },
  );
}

/** Photos and videos over the page, as the desktop's viewer window. */
function Gallery() {
  const payload = useUi((s) => s.gallery);
  const close = () => setUi({ gallery: null });
  useEffect(() => (payload ? pushDismiss(close) : undefined), [payload]);
  if (!payload) return null;
  return (
    <div className="safe-top safe-bottom safe-x fixed inset-0 z-50 bg-[#111]">
      <Viewer payload={payload} onClose={close} />
    </div>
  );
}

function TabBar() {
  const page = useUi((s) => s.railPage);
  const list = useChatList();
  const unread = unreadTotal(list);
  const tabs: { id: typeof page; label: string; icon: ReactNode; badge?: number }[] = [
    { id: "friends", label: "เพื่อน", icon: <PersonIcon size={25} /> },
    { id: "chats", label: "แชท", icon: <BubbleIcon size={25} />, badge: unread },
    { id: "add", label: "เพิ่มเพื่อน", icon: <PersonPlusIcon size={25} /> },
    { id: "more", label: "ตั้งค่า", icon: <MoreIcon size={23} /> },
  ];
  return (
    <nav className="safe-bottom flex shrink-0 border-t border-separator bg-list">
      {tabs.map((t) => (
        <button
          key={t.id}
          onClick={() => setUi({ railPage: t.id })}
          className={`relative flex h-[52px] flex-1 flex-col items-center justify-center gap-0.5 text-[10.5px] ${
            page === t.id ? "text-rail-icon-active" : "text-rail-icon"
          }`}
        >
          <span className="relative">
            {t.icon}
            {!!t.badge && (
              <span className="absolute -top-1 left-[17px] min-w-[18px] rounded-full bg-badge-red px-1 text-center text-[10px] leading-[18px] font-bold text-white">
                {t.badge > 999 ? "999+" : t.badge}
              </span>
            )}
          </span>
          {t.label}
        </button>
      ))}
    </nav>
  );
}

/** ตั้งค่า: who is signed in, and what the desktop keeps in the left bar's ⋯ menu. */
function MorePage() {
  const session = useSession();
  const dev = useUi((s) => s.developerMode);
  const [busy, setBusy] = useState(false);
  const p = session?.profile;
  const signedIn = session?.state === "signed_in";
  const row = "flex min-h-[50px] w-full items-center gap-3 px-4 text-left text-[16px] active:bg-row-selected disabled:opacity-40";
  return (
    <div className="flex h-full flex-col bg-list">
      <PhoneHeader title="ตั้งค่า" />
      <div className="min-h-0 flex-1 overflow-y-auto" style={{ paddingBottom: "env(safe-area-inset-bottom)" }}>
        {p && (
          <div className="flex items-center gap-3.5 px-4 py-3">
            <Avatar id={p.mid} name={p.displayName} picture={p.picture} size={60} />
            <div className="min-w-0">
              <div className="truncate text-[18px] font-semibold">{p.displayName}</div>
              <div className="truncate text-[13px] text-secondary">
                {p.device} · {session?.status}
              </div>
            </div>
          </div>
        )}
        <div className="mx-4 my-2 h-px bg-separator" />
        <button
          className={row}
          disabled={!p}
          onClick={() => p && (setUi({ railPage: "chats" }), openChat(p.mid, { focusComposer: false }))}
        >
          <span className="grid h-8 w-8 place-items-center rounded-full bg-line-green text-white">
            <BookmarkIcon size={16} strokeWidth={2.2} />
          </span>
          Keep Memo
        </button>
        <button
          className={row}
          disabled={!signedIn || busy}
          onClick={() => {
            setBusy(true);
            api
              .refresh(true)
              .catch((e) => notice(errorText(e)))
              .finally(() => setBusy(false));
          }}
        >
          รีเฟรชแชท
        </button>
        <label className={row}>
          <span className="flex-1">เครื่องมือนักพัฒนา</span>
          <Switch checked={dev} onChange={(v) => setUi({ developerMode: v })} />
        </label>
        <div className="mx-4 my-2 h-px bg-separator" />
        {signedIn && (
          <button
            className={`${row} text-badge-red`}
            onClick={() =>
              setUi({
                confirm: {
                  title: "ต้องการออกจากระบบหรือไม่",
                  body: "LAAB บนเครื่องนี้จะลบ session ที่เก็บไว้ ต้องเข้าสู่ระบบใหม่ครั้งหน้า",
                  action: "ออกจากระบบ",
                  danger: true,
                  run: () => api.logout().then(closeChat),
                },
              })
            }
          >
            ออกจากระบบ
          </button>
        )}
        <p className="px-4 pt-6 pb-8 text-[12px] leading-relaxed text-secondary">
          LAAB ทำงานเฉพาะตอนเปิดแอปอยู่ ถ้าปิดหรือสลับไปแอปอื่นนาน ๆ จะไม่ได้รับแจ้งเตือนจนกว่าจะเปิด LAAB อีกครั้ง
        </p>
      </div>
    </div>
  );
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      onClick={(e) => (e.preventDefault(), onChange(!checked))}
      className={`relative h-[31px] w-[51px] shrink-0 rounded-full transition-colors ${checked ? "bg-line-green" : "bg-field"}`}
    >
      <motion.span
        animate={{ x: checked ? 22 : 2 }}
        transition={{ type: "spring", duration: 0.25, bounce: 0.2 }}
        className="absolute top-[2px] left-0 h-[27px] w-[27px] rounded-full bg-white shadow"
      />
    </button>
  );
}
