import { Menu } from "@base-ui/react/menu";
import { addPluginListener, invoke } from "@tauri-apps/api/core";
import { Fragment, useEffect } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errorText, messageLimit, q, queryClient, useChatList, usePeople } from "../lib/api";
import { folderName, folderOf, isGroup } from "../lib/chatList";
import { closeChat, openChat } from "../lib/nav";
import { phone } from "../lib/platform";
import { clock, dayHeader } from "../lib/time";
import type { Chat, ChatList } from "../lib/types";
import { findEntry, type MenuEntry } from "../lib/nativeTabbar";
import { notice, setUi, ui, useUi } from "../lib/ui";
import { itemClass, popupClass, Separator } from "./kit";
import { onCoverScreen, pickPeek, showPreview } from "./ChatPreview";
import { LockIcon } from "./Icons";

const run = (p: Promise<unknown>, done?: string) => p.then(() => done && notice(done)).catch((e) => notice(errorText(e)));

function Item({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <Menu.Item className={itemClass} onClick={onClick}>
      {children}
    </Menu.Item>
  );
}

/** LAAB's chat folders (LINE edits them from the tab bar instead). */
function folderEntry(chat: Chat, list: ChatList | undefined): MenuEntry {
  const folders = (list?.workspaces ?? []).filter((w) => w.id !== "main");
  const ticks = folders.map((f) => {
    const inside = list?.pins[f.id]?.includes(chat.id) ?? false;
    return { id: `folder:${f.id}`, title: folderName(f, list), checked: inside, run: () => run(api.setPinned(chat.id, f.id, !inside)) };
  });
  const create = { id: "folder:new", title: "สร้างโฟลเดอร์แชท…", run: () => setUi({ folderDialog: { chatId: chat.id } }) };
  return { id: "folder", title: "เพิ่มในโฟลเดอร์", icon: "folder", children: ticks.length ? [ticks, [create]] : [[create]] };
}

function SubmenuEntry({ entry }: { entry: MenuEntry }) {
  return (
    <Menu.SubmenuRoot>
      <Menu.SubmenuTrigger className={`${itemClass} justify-between`}>
        {entry.title} <span className="opacity-60">›</span>
      </Menu.SubmenuTrigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4}>
          <Menu.Popup className={popupClass}>
            <MenuEntries sections={entry.children ?? []} />
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.SubmenuRoot>
  );
}

/** A menu described as data, drawn as LAAB's own menu. */
function MenuEntries({ sections }: { sections: MenuEntry[][] }) {
  return sections.map((section, i) => (
    <Fragment key={i}>
      {i > 0 && <Separator />}
      {section.map((e) =>
        e.children ? (
          <SubmenuEntry key={e.id} entry={e} />
        ) : e.checked !== undefined ? (
          <Menu.CheckboxItem key={e.id} className={itemClass} checked={e.checked} onCheckedChange={() => e.run?.()}>
            <span className="w-4 text-center">{e.checked ? "✓" : ""}</span>
            {e.title}
          </Menu.CheckboxItem>
        ) : (
          <Item key={e.id} onClick={() => e.run?.()}>
            {e.title}
          </Item>
        ),
      )}
    </Fragment>
  ));
}

function confirmDelete(chat: Chat) {
  setUi({
    confirm: {
      title: "ต้องการลบห้องแชทนี้หรือไม่",
      body: "ประวัติแชททั้งหมดจะถูกลบจากบัญชีของคุณ คู่สนทนายังเห็นอยู่",
      action: "ลบ",
      run: () => api.deleteHistory(chat.id).then(() => api.hideChat(chat.id)),
    },
  });
}

/** A chat row's menu, in LINE for Mac's order: เปิดในหน้าต่างใหม่ ─
 *  ปักหมุด, ซ่อน, ลบ ─ ปิดแจ้งเตือน, then LAAB's folders. LAAB's
 *  ดูโดยไม่อ่าน leads. */
function rowMenu(chat: Chat, list: ChatList | undefined, peeked: boolean): MenuEntry[][] {
  const pinned = list?.pins.main?.includes(chat.id) ?? false;
  const inFolder = list && folderOf(list).get(chat.id);
  const folder = inFolder ? list.workspaces.find((w) => w.id === inFolder) : undefined;
  return [
    [
      peeked
        ? { id: "unpeek", title: "เลิกแอบดู", icon: "eye.slash", run: () => pickPeek(null) }
        : { id: "peek", title: "ดูโดยไม่อ่าน", icon: "eye", run: () => (onCoverScreen() ? pickPeek(chat.id) : showPreview(chat.id)) },
      ...(phone ? [] : [{ id: "window", title: "เปิดในหน้าต่างใหม่", run: () => run(api.openChatWindow(chat.id, chat.name)) }]),
    ],
    [
      {
        id: "pin",
        title: pinned ? "เลิกปักหมุด" : "ปักหมุด",
        icon: pinned ? "pin.slash" : "pin",
        run: () => run(api.setPinned(chat.id, "main", !pinned)),
      },
      { id: "hide", title: "ซ่อน", icon: "archivebox", run: () => run(api.hideChat(chat.id)) },
      { id: "delete", title: "ลบ", icon: "trash", destructive: phone, run: () => confirmDelete(chat) },
    ],
    [
      {
        id: "mute",
        title: chat.muted ? "เปิดแจ้งเตือน" : "ปิดแจ้งเตือน",
        icon: chat.muted ? "bell" : "bell.slash",
        run: () => run(api.setMuted(chat.id, !chat.muted)),
      },
      folderEntry(chat, list),
      // Out of the folder the list shows it in, as dragging it out does.
      ...(folder
        ? [
            {
              id: "unfolder",
              title: `นำออกจาก ${folderName(folder, list)}`,
              icon: "folder.badge.minus",
              run: () => run(api.moveToFolder(chat.id, folder.id, null)),
            },
          ]
        : []),
    ],
  ];
}

/** Right-click on a chat row. */
export function ChatActions({ chat }: { chat: Chat }) {
  const list = useChatList();
  const peeked = useUi((s) => s.peek === chat.id);
  return <MenuEntries sections={rowMenu(chat, list, peeked)} />;
}

// The menu iOS shows now, for the item chosen from it.
let shown: MenuEntry[][] = [];
const nextFrame = () => new Promise((done) => requestAnimationFrame(done));

/** A long press on a chat row (a link, see ChatRow) asks for its menu. Away
 *  from the iPhone Duo's cover screen the chat's card is drawn for a moment
 *  for iOS to take a picture of and show over the menu; it shows the
 *  messages without opening the chat, so LINE gets no read receipt. */
async function answerChatMenu(token: number, chatId: string) {
  const list = queryClient.getQueryData<ChatList>(q.chats.queryKey);
  const chat = list?.chats.find((c) => c.id === chatId);
  shown = chat ? rowMenu(chat, list, ui.state.peek === chatId) : [];
  let card: { x: number; y: number; width: number; height: number } | null = null;
  if (chat && !onCoverScreen()) {
    // The card shows a spinner until the messages are in.
    await Promise.race([queryClient.prefetchQuery(q.messages(chatId)), new Promise((done) => setTimeout(done, 400))]);
    setUi({ menuCard: chatId });
    await nextFrame();
    await nextFrame();
    const r = document.getElementById("menu-card")?.getBoundingClientRect();
    if (r) card = { x: r.x, y: r.y, width: r.width, height: r.height };
  }
  try {
    await invoke("plugin:native-tabbar|menu", { token, menu: shown, card });
  } finally {
    // The card is drawn under a picture of the page (the plugin's freeze),
    // which comes off once the card is gone.
    setUi({ menuCard: null });
    await nextFrame();
    await nextFrame();
    await invoke("plugin:native-tabbar|thaw").catch(() => {});
  }
}

/** The system context menu for chat rows on iOS (the native-tabbar plugin's
 *  ChatMenus). A tap on the card opens the chat, which reads it. */
export function useNativeChatMenu() {
  useEffect(() => {
    if (!phone) return;
    const listeners = [
      addPluginListener<{ token: number; chatId: string }>("native-tabbar", "chatMenu", (e) => {
        answerChatMenu(e.token, e.chatId).catch((err) => notice(errorText(err)));
      }),
      addPluginListener<{ id: string }>("native-tabbar", "menuAction", (e) => findEntry(shown, e.id)?.run?.()),
      addPluginListener<{ chatId: string }>("native-tabbar", "menuCommit", (e) => openChat(e.chatId)),
    ];
    return () => listeners.forEach((l) => l.then((x) => x.unregister()));
  }, []);
}

/** Write the loaded messages as plain text, like LINE's "บันทึกแชท". */
async function exportChat(chat: Chat, name: (mid: string) => string) {
  const path = await save({ defaultPath: `[LINE] ${chat.name}.txt`, filters: [{ name: "Text", extensions: ["txt"] }] });
  if (!path) return;
  const messages = await api.messages(chat.id, messageLimit(chat.id));
  let day = "";
  const lines = [`[LINE] ประวัติแชทใน ${chat.name}`, ""];
  for (const m of messages) {
    if (m.unsent) continue;
    const d = dayHeader(m.createdTime);
    if (d !== day) {
      lines.push("", d);
      day = d;
    }
    const who = m.mine ? "คุณ" : m.senderId ? name(m.senderId) : "";
    const body =
      m.text ||
      (
        {
          IMAGE: "[รูปภาพ]",
          VIDEO: "[วิดีโอ]",
          STICKER: "[สติกเกอร์]",
          FILE: `[ไฟล์] ${m.fileName ?? ""}`,
          AUDIO: "[ข้อความเสียง]",
          CONTACT: `[ผู้ติดต่อ] ${m.metadata.displayName ?? ""}`,
        } as Record<string, string>
      )[m.contentType] ||
      `[${m.contentType.toLowerCase()}]`;
    lines.push(`${clock(m.createdTime)}\t${who}\t${body}`);
  }
  await api.saveText(path, lines.join("\n") + "\n");
  notice("บันทึกแชทแล้ว");
}

/** The chat's menu, in LINE for Mac's order, without what LAAB cannot do
 *  (เชิญ, โหวต, ตั้งค่า, รายงานปัญหา): the header's ⋮, and the system's
 *  overflow menu where iOS puts the chat's buttons in the vertical bar. */
export function useChatMenu(chat: Chat | undefined): MenuEntry[][] {
  const people = usePeople();
  const list = useChatList();
  if (!chat) return [];
  const group = isGroup(chat);
  const name = (mid: string) => list?.chats.find((c) => c.id === mid)?.name ?? people[mid]?.name ?? "สมาชิก";
  const aux = (kind: "albums" | "media", tab: string) => () => run(api.openAuxWindow(kind, chat.id, chat.name, tab));
  const confirm = (title: string, body: string, action: string, go: () => Promise<unknown>) => () =>
    setUi({ confirm: { title, body, action, run: go } });
  const last: MenuEntry[] = [
    { id: "reload", title: "โหลดข้อความใหม่", icon: "arrow.clockwise", run: () => run(api.reloadChat(chat.id)) },
    folderEntry(chat, list),
  ];
  if (group)
    last.push({
      id: "leave",
      title: "ออกจากกลุ่ม",
      icon: "rectangle.portrait.and.arrow.right",
      destructive: true,
      run: confirm("ต้องการออกจากกลุ่มนี้หรือไม่", "ประวัติแชทและรายชื่อสมาชิกจะถูกลบ", "ออกจากกลุ่ม", () =>
        api.leaveGroup(chat.id).then(closeChat),
      ),
    });
  else if (chat.id.startsWith("u"))
    last.push({
      id: "block",
      title: "บล็อก",
      icon: "hand.raised",
      destructive: true,
      run: confirm(`ต้องการบล็อก ${chat.name} หรือไม่`, "จะไม่ได้รับข้อความหรือการโทรจากบัญชีนี้", "บล็อก", () =>
        api.setBlocked(chat.id, true),
      ),
    });
  return [
    [
      {
        id: "mute",
        title: chat.muted ? "เปิดแจ้งเตือน" : "ปิดแจ้งเตือน",
        icon: chat.muted ? "bell" : "bell.slash",
        run: () => run(api.setMuted(chat.id, !chat.muted)),
      },
    ],
    [
      ...(group ? [{ id: "albums", title: "อัลบั้ม", icon: "photo.on.rectangle", run: aux("albums", "albums") }] : []),
      { id: "media", title: "รูป & วิดีโอ", icon: "photo", run: aux("media", "media") },
      { id: "files", title: "ไฟล์", icon: "doc", run: aux("media", "files") },
      { id: "links", title: "ลิงก์", icon: "link", run: aux("media", "links") },
    ],
    [{ id: "export", title: "บันทึกแชท", icon: "square.and.arrow.down", run: () => run(exportChat(chat, name)) }],
    last,
  ];
}

export function HeaderMenu({ chat, secure }: { chat: Chat; secure: boolean }) {
  const sections = useChatMenu(chat);
  return (
    <>
      <MenuEntries sections={sections} />
      {secure && <Separator />}
      {secure && (
        <div className="flex gap-1.5 px-4 py-1 text-[12px] leading-snug text-secondary">
          <LockIcon size={12} className="mt-0.5 shrink-0" />
          <span>
            ห้องแชทนี้ปลอดภัยด้วย
            <br />
            Letter Sealing
          </span>
        </div>
      )}
    </>
  );
}
