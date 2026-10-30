import { Menu } from "@base-ui/react/menu";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errorText, messageLimit, useChatList, usePeople } from "../lib/api";
import { folderName, folderOf, isGroup } from "../lib/chatList";
import { closeChat } from "../lib/nav";
import { clock, dayHeader } from "../lib/time";
import type { Chat } from "../lib/types";
import { notice, setUi } from "../lib/ui";
import { itemClass, popupClass, Separator } from "./kit";
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
function FolderMenu({ chat }: { chat: Chat }) {
  const list = useChatList();
  const folders = (list?.workspaces ?? []).filter((w) => w.id !== "main");
  return (
    <Menu.SubmenuRoot>
      <Menu.SubmenuTrigger className={`${itemClass} justify-between`}>
        เพิ่มในโฟลเดอร์ <span className="opacity-60">›</span>
      </Menu.SubmenuTrigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4}>
          <Menu.Popup className={popupClass}>
            {folders.map((f) => {
              const inside = list?.pins[f.id]?.includes(chat.id) ?? false;
              return (
                <Menu.CheckboxItem
                  key={f.id}
                  className={itemClass}
                  checked={inside}
                  onCheckedChange={(v) => run(api.setPinned(chat.id, f.id, v))}
                >
                  <span className="w-4 text-center">{inside ? "✓" : ""}</span>
                  {folderName(f, list)}
                </Menu.CheckboxItem>
              );
            })}
            {folders.length > 0 && <Separator />}
            <Item onClick={() => setUi({ folderDialog: { chatId: chat.id } })}>สร้างโฟลเดอร์แชท…</Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.SubmenuRoot>
  );
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

/** Right-click on a chat row, in LINE for Mac's order: เปิดในหน้าต่างใหม่ ─
 *  ปักหมุด, ซ่อน, ลบ ─ ปิดแจ้งเตือน, then LAAB's folders. */
export function ChatActions({ chat }: { chat: Chat }) {
  const list = useChatList();
  const pinned = list?.pins.main?.includes(chat.id) ?? false;
  return (
    <>
      <Item onClick={() => run(api.openChatWindow(chat.id, chat.name))}>เปิดในหน้าต่างใหม่</Item>
      <Separator />
      <Item onClick={() => run(api.setPinned(chat.id, "main", !pinned))}>{pinned ? "เลิกปักหมุด" : "ปักหมุด"}</Item>
      <Item onClick={() => run(api.hideChat(chat.id))}>ซ่อน</Item>
      <Item onClick={() => confirmDelete(chat)}>ลบ</Item>
      <Separator />
      <Item onClick={() => run(api.setMuted(chat.id, !chat.muted))}>{chat.muted ? "เปิดแจ้งเตือน" : "ปิดแจ้งเตือน"}</Item>
      <FolderMenu chat={chat} />
      <LeaveFolder chat={chat} />
    </>
  );
}

/** Out of the folder the list shows it in, as dragging it out does. */
function LeaveFolder({ chat }: { chat: Chat }) {
  const list = useChatList();
  const id = list && folderOf(list).get(chat.id);
  const folder = id && list.workspaces.find((w) => w.id === id);
  if (!folder) return null;
  return <Item onClick={() => run(api.moveToFolder(chat.id, folder.id, null))}>นำออกจาก {folderName(folder, list)}</Item>;
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
        } as Record<string, string>
      )[m.contentType] ||
      `[${m.contentType.toLowerCase()}]`;
    lines.push(`${clock(m.createdTime)}\t${who}\t${body}`);
  }
  await api.saveText(path, lines.join("\n") + "\n");
  notice("บันทึกแชทแล้ว");
}

/** The chat header's ⋮ menu, in LINE for Mac's order, without what LAAB
 *  cannot do (เชิญ, โหวต, ตั้งค่า, รายงานปัญหา). */
export function HeaderMenu({ chat, secure }: { chat: Chat; secure: boolean }) {
  const people = usePeople();
  const list = useChatList();
  const group = isGroup(chat);
  const name = (mid: string) => list?.chats.find((c) => c.id === mid)?.name ?? people[mid]?.name ?? "สมาชิก";
  return (
    <>
      <Item onClick={() => run(api.setMuted(chat.id, !chat.muted))}>{chat.muted ? "เปิดแจ้งเตือน" : "ปิดแจ้งเตือน"}</Item>
      <Separator />
      {group && <Item onClick={() => run(api.openAuxWindow("albums", chat.id, chat.name, "albums"))}>อัลบั้ม</Item>}
      <Item onClick={() => run(api.openAuxWindow("media", chat.id, chat.name, "media"))}>รูป & วิดีโอ</Item>
      <Item onClick={() => run(api.openAuxWindow("media", chat.id, chat.name, "files"))}>ไฟล์</Item>
      <Item onClick={() => run(api.openAuxWindow("media", chat.id, chat.name, "links"))}>ลิงก์</Item>
      <Separator />
      <Item onClick={() => run(exportChat(chat, name))}>บันทึกแชท</Item>
      <Separator />
      <Item onClick={() => run(api.reloadChat(chat.id))}>โหลดข้อความใหม่</Item>
      <FolderMenu chat={chat} />
      {group ? (
        <Item
          onClick={() =>
            setUi({
              confirm: {
                title: "ต้องการออกจากกลุ่มนี้หรือไม่",
                body: "ประวัติแชทและรายชื่อสมาชิกจะถูกลบ",
                action: "ออกจากกลุ่ม",
                run: () => api.leaveGroup(chat.id).then(closeChat),
              },
            })
          }
        >
          ออกจากกลุ่ม
        </Item>
      ) : (
        chat.id.startsWith("u") && (
          <Item
            onClick={() =>
              setUi({
                confirm: {
                  title: `ต้องการบล็อก ${chat.name} หรือไม่`,
                  body: "จะไม่ได้รับข้อความหรือการโทรจากบัญชีนี้",
                  action: "บล็อก",
                  run: () => api.setBlocked(chat.id, true),
                },
              })
            }
          >
            บล็อก
          </Item>
        )
      )}
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
