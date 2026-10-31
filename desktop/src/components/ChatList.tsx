import { useVirtualizer } from "@tanstack/react-virtual";
import { Menu } from "@base-ui/react/menu";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorText, useChatList, useSession } from "../lib/api";
import { clickEndsDrag, isRecent, markRecent, moving, pressChat, useChatDrag } from "../lib/chatDrag";
import { filterChats, folderName, folderOf, groupRows, isKeepMemo, type ListRow, listPreview } from "../lib/chatList";
import { openChat } from "../lib/nav";
import { listLabel } from "../lib/time";
import type { Chat, ChatList, Workspace } from "../lib/types";
import { type ChatFilter, notice, setUi, ui, useUi } from "../lib/ui";
import { installAndRestart, useReadyVersion } from "../lib/updater";
import { Avatar } from "./Avatar";
import { ChatActions } from "./ChatActions";
import {
  BellSlashIcon,
  BookmarkIcon,
  ChevronIcon,
  FolderIcon,
  PhoneOutlineIcon,
  PinIcon,
  PlusBubbleIcon,
  SearchIcon,
  SlidersIcon,
  SortIcon,
} from "./Icons";
import { Badge, Ctx, CtxItem, itemClass, popupClass, Separator, Spinner } from "./kit";

export function ChatListColumn({ selected }: { selected: string | null }) {
  const list = useChatList();
  const filter = useUi((s) => s.filter);
  const sort = useUi((s) => s.sort);
  const query = useUi((s) => s.listQuery);
  const collapsedFolders = useUi((s) => s.collapsedFolders);
  const chats = useMemo(() => (list ? filterChats(list, filter, sort, query) : []), [list, filter, sort, query]);
  // Folders group their chats in "ทั้งหมด"; a search or another tab lists
  // chats flat.
  const grouped = filter === "all" && !query.trim();
  const rows = useMemo<ListRow[]>(
    () =>
      list && grouped
        ? groupRows(list, chats, new Set(list.workspaces.filter((w) => !collapsedFolders.includes(w.id)).map((w) => w.id)))
        : chats.map((chat) => ({ kind: "chat", chat, folder: null })),
    [list, grouped, chats, collapsedFolders],
  );
  const visible = useMemo(() => rows.flatMap((r) => (r.kind === "chat" ? [r.chat] : [])), [rows]);
  const pinned = useMemo(() => new Set(list?.pins.main ?? []), [list]);
  const scroller = useRef<HTMLDivElement>(null);
  // Rows on screen last render: only rows that just appeared animate in.
  const shownKeys = useRef(new Set<string>());
  const rowKey = (r: ListRow) => (r.kind === "folder" ? `f:${r.folder.id}` : `${r.folder ?? ""}:${r.chat.id}`);
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 72,
    overscan: 8,
  });
  useEffect(() => {
    shownKeys.current = new Set(virtual.getVirtualItems().map((v) => rowKey(rows[v.index])));
  });

  // ⌥↑/⌥↓ walk the visible order.
  useEffect(() => setUi({ visibleChatIds: visible.map((c) => c.id) }), [visible]);
  // A chat opened from search or a notification shows inside its folder.
  useEffect(() => {
    const f = list && selected && grouped ? folderOf(list).get(selected) : undefined;
    if (f && ui.state.collapsedFolders.includes(f)) setUi({ collapsedFolders: ui.state.collapsedFolders.filter((id) => id !== f) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected]);
  useEffect(() => {
    const i = rows.findIndex((r) => r.kind === "chat" && r.chat.id === selected);
    if (i >= 0) virtual.scrollToIndex(i, { align: "auto" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected]);

  const onKey = (e: React.KeyboardEvent) => {
    // Plain ↑/↓ switch chats while the list has focus; Return moves to the composer.
    if (e.altKey || e.metaKey || e.ctrlKey || e.target !== e.currentTarget) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const i = visible.findIndex((c) => c.id === selected);
      const j = i < 0 ? (e.key === "ArrowDown" ? 0 : visible.length - 1) : i + (e.key === "ArrowDown" ? 1 : -1);
      const next = visible[j];
      if (next) openChat(next.id, { focusComposer: false });
    } else if (e.key === "Enter" && selected) {
      setUi({ composerFocus: Date.now() });
    }
  };

  return (
    <div className="flex h-full flex-col bg-list">
      <Tabs />
      <div className="flex items-center gap-2 px-4 pt-[8px] pb-2">
        <label className="flex h-[35px] flex-1 items-center gap-2 rounded-[6px] bg-field px-2.5 text-secondary">
          <SearchIcon size={14} />
          <input
            className="selectable min-w-0 flex-1 bg-transparent text-[13px] text-primary outline-none placeholder:text-secondary"
            placeholder="ค้นหาข้อความและห้องแชท"
            value={query}
            onChange={(e) => setUi({ listQuery: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) setUi({ searchOpen: true, searchInitial: query });
              if (e.key === "Escape") setUi({ listQuery: "" });
            }}
          />
          {query && (
            <button
              title="ล้าง"
              aria-label="ล้างการค้นหา"
              onClick={() => setUi({ listQuery: "" })}
              className="grid h-4 w-4 place-items-center rounded-full bg-secondary/70 text-[10px] leading-none text-list"
            >
              ✕
            </button>
          )}
        </label>
        {!query && (
          <Menu.Root>
            <Menu.Trigger className="grid h-8 w-8 place-items-center rounded-md text-secondary hover:bg-row-hover" title="เรียงลำดับ">
              <SortIcon size={16} />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner className="z-50" sideOffset={4} align="end">
                <Menu.Popup className={popupClass}>
                  <Menu.RadioGroup value={sort} onValueChange={(v) => setUi({ sort: v as "recent" | "unread" })}>
                    {(
                      [
                        ["recent", "เรียงตามเวลาที่ได้รับ"],
                        ["unread", "เรียงตามข้อความที่ยังไม่อ่าน"],
                      ] as const
                    ).map(([v, label]) => (
                      <Menu.RadioItem key={v} value={v} className={itemClass}>
                        <span className="w-4 text-center">{sort === v ? "✓" : ""}</span>
                        {label}
                      </Menu.RadioItem>
                    ))}
                  </Menu.RadioGroup>
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
        )}
      </div>
      <div
        ref={scroller}
        tabIndex={0}
        onKeyDown={onKey}
        aria-label="รายชื่อแชท"
        data-drop="list"
        className="relative min-h-0 flex-1 overflow-y-auto outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-secondary"
      >
        {query.trim() && chats.length > 0 && (
          // LINE for Mac: results under a "แชท N" heading.
          <div className="px-4 pt-1 pb-1 text-[12px] text-secondary">แชท {chats.length}</div>
        )}
        {!list ? (
          <div className="grid place-items-center pt-16 text-secondary">
            <Spinner />
          </div>
        ) : chats.length === 0 ? (
          <Empty filter={filter} query={query} />
        ) : (
          <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
            {virtual.getVirtualItems().map((v) => {
              const row = rows[v.index];
              // Right after a folder opens, closes or takes a chat, rows
              // slide to their new places; any other time they jump.
              const slide = moving();
              const key = rowKey(row);
              const enter = !shownKeys.current.has(key) && isRecent(row.kind === "folder" ? row.folder.id : row.folder);
              const nth = row.kind === "chat" && row.folder ? rows.findIndex((r) => r.kind === "folder" && r.folder.id === row.folder) : -1;
              return (
                <div
                  key={key}
                  className={`${slide ? "transition-transform duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none" : ""} ${
                    enter ? (row.kind === "folder" ? "motion-safe:animate-folder-in" : "motion-safe:animate-row-in") : ""
                  }`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    right: 0,
                    transform: `translateY(${v.start}px)`,
                    // A short stagger down the opened folder.
                    animationDelay: enter && nth >= 0 ? `${Math.min(v.index - nth - 1, 4) * 25}ms` : undefined,
                  }}
                >
                  {row.kind === "folder" ? (
                    <FolderRow row={row} list={list} selected={!row.open && row.chats.some((c) => c.id === selected)} />
                  ) : (
                    <ChatRow
                      chat={row.chat}
                      selected={row.chat.id === selected}
                      pinned={pinned.has(row.chat.id)}
                      folder={row.folder}
                      scroller={scroller}
                      draggable={grouped}
                    />
                  )}
                </div>
              );
            })}
          </div>
        )}
        <button
          className="sticky bottom-4 left-full mr-4 grid h-13 w-13 place-items-center rounded-full bg-line-green text-white shadow-[0_3px_12px_rgba(0,0,0,0.25)] transition-transform duration-150 ease-out active:scale-[0.96]"
          style={{ marginTop: -52 }}
          title="แชทใหม่"
          onClick={() => setUi({ searchOpen: true, searchInitial: "" })}
        >
          <PlusBubbleIcon size={24} />
        </button>
      </div>
      <UpdateBar />
      <StatusBar />
      <DragGhost list={list} />
    </div>
  );
}

function Empty({ filter, query }: { filter: ChatFilter; query: string }) {
  const text = query
    ? "ไม่พบแชท… กด Return เพื่อค้นหาในข้อความ"
    : filter.startsWith("folder:")
      ? "คลิกขวาที่แชทแล้วเลือก “เพิ่มในโฟลเดอร์” เพื่อใส่แชทในโฟลเดอร์นี้"
      : "ไม่มีแชท";
  return <div className="px-8 pt-16 text-center text-[13px] leading-relaxed text-secondary">{text}</div>;
}

function Tabs() {
  const list = useChatList();
  const filter = useUi((s) => s.filter);
  const folders = (list?.workspaces ?? []).filter((w) => w.id !== "main");
  const current = filter.startsWith("folder:") ? folders.find((f) => f.id === filter.slice(7)) : undefined;
  const tabs: { id: ChatFilter; label: string }[] = [
    { id: "all", label: "ทั้งหมด" },
    { id: "friends", label: "เพื่อน" },
    ...folders.map((f) => ({ id: `folder:${f.id}` as ChatFilter, label: folderName(f, list) })),
    { id: "groups", label: "กลุ่ม" },
    { id: "official", label: "บัญชีทางการ" },
  ];
  const unreadIn = (id: ChatFilter) => {
    if (!list || id === "all") return false;
    return filterChats(list, id, "recent", "").some((c) => c.unread > 0);
  };
  return (
    <Ctx
      className="block"
      menu={
        <>
          {current && (
            <>
              <div className="px-4 py-1 text-secondary">{list?.pins[current.id]?.length ?? 0} แชท</div>
              <CtxItem onClick={() => setUi({ folderDialog: { chatId: null, rename: current } })}>เปลี่ยนชื่อโฟลเดอร์…</CtxItem>
              <CtxItem onClick={() => deleteFolder(current.id)}>ลบโฟลเดอร์</CtxItem>
              <Separator />
            </>
          )}
          <Menu.Item className={itemClass} onClick={() => setUi({ folderDialog: { chatId: null } })}>
            สร้างโฟลเดอร์แชท…
          </Menu.Item>
        </>
      }
    >
      {/* LINE for Mac: tabs in the title-bar row; the search box under them
          lines up with the chat header. */}
      <div
        className="drag flex h-[60px] items-start gap-[18px] overflow-x-auto px-4 pt-[16px] [scrollbar-width:none]"
        data-tauri-drag-region
      >
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setUi({ filter: t.id })}
            className={`no-drag relative shrink-0 pb-1 text-[13.5px] ${filter === t.id ? "font-bold text-primary" : "text-secondary"}`}
          >
            {t.label}
            {unreadIn(t.id) && <span className="absolute -top-0.5 -right-2 h-[5px] w-[5px] rounded-full bg-line-green" />}
            {filter === t.id && (
              <span className="absolute right-0 bottom-0 left-0 h-0.5 rounded bg-primary" />
            )}
          </button>
        ))}
        <button
          title="จัดการโฟลเดอร์แชท"
          onClick={() => setUi({ folderDialog: { chatId: null } })}
          className="no-drag shrink-0 pt-0.5 text-secondary hover:text-primary"
        >
          <SlidersIcon size={18} />
        </button>
      </div>
    </Ctx>
  );
}

/** The part of a name that matches the list search, in green as in LINE. */
function Highlight({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  const at = q ? text.toLocaleLowerCase("th").indexOf(q.toLocaleLowerCase("th")) : -1;
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <span className="text-line-green">{text.slice(at, at + q.length)}</span>
      {text.slice(at + q.length)}
    </>
  );
}

/** Calls read "📞 เวลาโทร : 01:25"; LINE draws the phone as a gray outline. */
function Preview({ text }: { text: string }) {
  if (!text.startsWith("📞 ") && !text.startsWith("🎥 ")) return <>{text}</>;
  return (
    <>
      <PhoneOutlineIcon size={11} className="mr-1 inline -translate-y-px" />
      {text.slice(3)}
    </>
  );
}

/** Ungroup a folder; its chats go back to the plain list. */
function deleteFolder(id: string) {
  if (ui.state.filter === `folder:${id}`) setUi({ filter: "all" });
  api.deleteFolder(id).catch((e) => notice(errorText(e)));
}

const toggleFolder = (id: string) => {
  markRecent(id);
  const collapsed = ui.state.collapsedFolders;
  setUi({ collapsedFolders: collapsed.includes(id) ? collapsed.filter((f) => f !== id) : [...collapsed, id] });
};

/** A folder in the list, as Discord shows one: the first chats' pictures
 *  while closed, the unread count of all its chats. Click opens or closes
 *  it; drop a chat on it to move the chat in. */
function FolderRow({ row, list, selected }: { row: Extract<ListRow, { kind: "folder" }>; list: ChatList; selected: boolean }) {
  const { folder, chats, open } = row;
  const target = useChatDrag((d) => d?.target?.kind === "folder" && d.target.id === folder.id);
  const unread = chats.reduce((n, c) => n + (c.muted ? 0 : Math.max(0, c.unread)), 0);
  const mutedUnread = chats.reduce((n, c) => n + (c.muted ? Math.max(0, c.unread) : 0), 0);
  const latest = Math.max(...chats.map((c) => c.lastActivity));
  return (
    <Ctx
      className="block"
      menu={
        <>
          <CtxItem onClick={() => setUi({ renamingFolder: folder.id })}>เปลี่ยนชื่อ</CtxItem>
          <CtxItem onClick={() => setUi({ filter: `folder:${folder.id}` })}>เปิดเป็นแท็บ</CtxItem>
          <Separator />
          <CtxItem onClick={() => deleteFolder(folder.id)}>ลบโฟลเดอร์</CtxItem>
        </>
      }
    >
      <div
        data-drop="folder"
        data-id={folder.id}
        aria-expanded={open}
        onClick={() => !clickEndsDrag() && toggleFolder(folder.id)}
        className={`flex h-[72px] cursor-default items-center gap-3 px-4 active:bg-row-selected ${
          target ? "bg-line-green/12 shadow-[inset_0_0_0_2px_var(--color-line-green)]" : selected ? "bg-row-selected" : "hover:bg-row-hover"
        }`}
      >
        <FolderPicture chats={chats} open={open} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <FolderTitle folder={folder} list={list} />
            <span className="shrink-0 text-[13.5px] text-secondary">({chats.length})</span>
            <ChevronIcon
              dir="down"
              size={9}
              className={`shrink-0 text-secondary transition-transform duration-150 ease-out motion-reduce:transition-none ${open ? "rotate-180" : ""}`}
            />
            <span className="ml-auto shrink-0 text-[11px] text-secondary">{listLabel(latest)}</span>
          </div>
          <div className="mt-0.5 flex items-start gap-2">
            <span className="line-clamp-2 flex-1 text-[12px] leading-[1.35] text-secondary">
              {folder.name ? chats.map((c) => c.name).join(", ") : `${chats.length} แชท`}
            </span>
            {unread > 0 ? <Badge count={unread} /> : <Badge count={mutedUnread} className="bg-secondary!" />}
          </div>
        </div>
      </div>
    </Ctx>
  );
}

/** Closed: up to four of its chats' pictures in a rounded square. Open: a
 *  folder, its chats listed below. */
function FolderPicture({ chats, open }: { chats: Chat[]; open: boolean }) {
  const swap = "transition-[opacity,scale] duration-150 ease-out motion-reduce:transition-none";
  return (
    <div className="relative h-[52px] w-[52px] shrink-0 rounded-[16px] bg-field">
      <div className={`absolute inset-0 grid place-items-center text-line-green ${swap} ${open ? "" : "scale-75 opacity-0"}`}>
        <FolderIcon size={26} />
      </div>
      <div className={`absolute inset-0 flex flex-wrap content-center justify-center gap-[3px] p-[5px] ${swap} ${open ? "scale-75 opacity-0" : ""}`}>
        {chats.slice(0, 4).map((c) => (
          <Avatar key={c.id} id={c.id} name={c.name} size={19} picture={c.picture} local={c.avatar} />
        ))}
      </div>
    </div>
  );
}

/** The folder's name, or a field to type it in right after the folder is
 *  made by dropping one chat on another. Return or leaving saves; Escape
 *  keeps the old name. */
function FolderTitle({ folder, list }: { folder: Workspace; list: ChatList }) {
  const editing = useUi((s) => s.renamingFolder === folder.id);
  const done = useRef(false);
  useEffect(() => {
    if (editing) done.current = false;
  }, [editing]);
  const shown = folderName(folder, list);
  if (!editing) {
    return <span className={`truncate text-[13.5px] ${folder.name ? "text-primary" : "text-secondary"}`}>{shown}</span>;
  }
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    setUi({ renamingFolder: null });
    if (name != null && name.trim() !== folder.name) api.renameFolder(folder.id, name).catch((e) => notice(errorText(e)));
  };
  return (
    <input
      autoFocus
      defaultValue={folder.name}
      placeholder={shown}
      aria-label="ชื่อโฟลเดอร์"
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter" && !e.nativeEvent.isComposing) finish(e.currentTarget.value);
        if (e.key === "Escape") finish(null);
      }}
      onBlur={(e) => finish(e.currentTarget.value)}
      className="selectable h-6 min-w-0 flex-1 rounded-[5px] bg-field px-1.5 text-[13.5px] text-primary outline-none placeholder:text-secondary"
    />
  );
}

/** What follows the pointer while a chat is dragged, saying what dropping
 *  it there does. */
function DragGhost({ list }: { list: ChatList | undefined }) {
  const d = useChatDrag((x) => x);
  if (!d) return null;
  const t = d.target;
  const hint =
    t?.kind === "merge"
      ? `สร้างโฟลเดอร์กับ ${list?.chats.find((c) => c.id === t.chatId)?.name ?? ""}`
      : t?.kind === "folder"
        ? `ย้ายไป ${folderName(list?.workspaces.find((w) => w.id === t.id) ?? { id: t.id, name: "", position: 0 }, list)}`
        : t?.kind === "out"
          ? "นำออกจากโฟลเดอร์"
          : null;
  return (
    <div
      className="pointer-events-none fixed z-50 flex max-w-[260px] origin-top-left items-center gap-2 rounded-full bg-popover py-1 pr-3.5 pl-1 shadow-[0_4px_16px_rgba(0,0,0,0.18)] motion-safe:animate-ghost-in"
      style={{ left: d.x + 12, top: d.y + 10 }}
    >
      <Avatar id={d.chat.id} name={d.chat.name} size={28} picture={d.chat.picture} local={d.chat.avatar} />
      <div className="min-w-0 leading-tight">
        <div className="truncate text-[12.5px] text-primary">{d.chat.name}</div>
        {hint && <div className="truncate text-[11px] text-line-green">{hint}</div>}
      </div>
    </div>
  );
}

export function ChatRow({
  chat,
  selected,
  pinned,
  folder = null,
  scroller,
  draggable = false,
}: {
  chat: Chat;
  selected: boolean;
  pinned: boolean;
  /** The open folder this row is listed in. */
  folder?: string | null;
  scroller?: React.RefObject<HTMLDivElement | null>;
  /** Drag onto another chat to make a folder ("ทั้งหมด" tab). */
  draggable?: boolean;
}) {
  const [hover, setHover] = useState(false);
  const query = useUi((s) => s.listQuery);
  const keep = isKeepMemo(chat, useSession()?.profile?.mid);
  const dragged = useChatDrag((d) => d?.chat.id === chat.id);
  const merge = useChatDrag((d) => d?.target?.kind === "merge" && d.target.chatId === chat.id);
  const intoFolder = useChatDrag((d) => !!folder && d?.target?.kind === "folder" && d.target.id === folder);
  return (
    <Ctx menu={<ChatActions chat={chat} />} className="block">
      <div
        data-drop="chat"
        data-id={chat.id}
        data-folder={folder ?? undefined}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        onPointerDown={draggable ? (e) => pressChat(e, chat, folder, scroller?.current ?? null) : undefined}
        onClick={() => !clickEndsDrag() && openChat(chat.id)}
        className={`flex h-[72px] cursor-default items-center gap-3 px-4 transition-opacity duration-150 ease-out active:bg-row-selected ${folder ? "pl-7" : ""} ${dragged ? "opacity-40" : ""} ${
          merge || intoFolder ? "bg-line-green/12" : selected ? "bg-row-selected" : hover ? "bg-row-hover" : folder ? "bg-row-hover/60" : ""
        }`}
      >
        {/* The chat it would share a folder with grows a little, as in Discord. */}
        <div
          className={`relative rounded-full transition-[scale] duration-150 ease-out motion-reduce:transition-none ${
            merge ? "scale-[1.06] ring-2 ring-line-green ring-offset-2 ring-offset-list" : ""
          }`}
        >
          {keep ? (
            <div className="grid h-[52px] w-[52px] place-items-center rounded-full bg-line-green text-white">
              <BookmarkIcon size={24} strokeWidth={2.2} />
            </div>
          ) : (
            <Avatar id={chat.id} name={chat.name} size={52} picture={chat.picture} local={chat.avatar} />
          )}
          {pinned && (
            <span
              className="absolute -top-0.5 -left-0.5 grid place-items-center rounded-full bg-pin text-white ring-2 ring-list"
              style={{ width: 52 * 0.36, height: 52 * 0.36 }}
            >
              <PinIcon size={11} style={{ transform: "rotate(45deg)" }} />
            </span>
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-[13.5px] text-primary">{keep ? "Keep Memo" : <Highlight text={chat.name} query={query} />}</span>
            {chat.kind === "group" && chat.memberCount ? (
              <span className="shrink-0 text-[13.5px] text-primary">({chat.memberCount})</span>
            ) : null}
            {chat.muted && <BellSlashIcon size={10} className="shrink-0 text-secondary" />}
            <span className="ml-auto shrink-0 text-[11px] text-secondary">{listLabel(chat.lastActivity)}</span>
          </div>
          <div className="mt-0.5 flex items-start gap-2">
            <span className="line-clamp-2 flex-1 text-[12px] leading-[1.35] text-secondary">
              <Preview text={listPreview(chat.preview)} />
            </span>
            <Badge count={chat.unread} className={chat.muted ? "bg-secondary!" : ""} />
          </div>
        </div>
      </div>
    </Ctx>
  );
}

function UpdateBar() {
  const version = useReadyVersion();
  const [busy, setBusy] = useState(false);
  if (!version) return null;
  const restart = () => {
    setBusy(true);
    installAndRestart().catch((e) => {
      setBusy(false);
      notice(errorText(e));
    });
  };
  return (
    <div className="flex items-center gap-2 border-t border-separator bg-rail px-3 py-1.5 text-[12px] motion-safe:animate-row-in">
      <span className="min-w-0 flex-1 truncate text-primary">LAAB {version} พร้อมติดตั้งแล้ว</span>
      <button
        className="h-6 shrink-0 rounded-md bg-line-green px-2.5 text-[12px] font-medium text-white transition-transform duration-150 ease-out active:scale-[0.96] disabled:opacity-50"
        disabled={busy}
        onClick={restart}
      >
        {busy ? "กำลังติดตั้ง…" : "รีสตาร์ต"}
      </button>
    </div>
  );
}

function StatusBar() {
  const s = useSession();
  const text = useUi((u) => u.notice);
  const syncing = s && s.syncTotal > 0;
  const line = text ?? (syncing ? s.status : s && s.status !== "เชื่อม LINE แล้ว" && s.state !== "demo" ? s.status : null);
  if (!line) return null;
  return (
    <div className="flex h-[26px] items-center gap-2 border-t border-separator bg-rail px-3 text-[11px] text-secondary">
      {syncing && <Spinner size={10} />}
      <span className="truncate">{line}</span>
    </div>
  );
}
