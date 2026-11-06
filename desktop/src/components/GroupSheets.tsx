import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { api, errorText, q, queryClient, useChatList, useSession } from "../lib/api";
import { closeChat, openChat } from "../lib/nav";
import type { Chat } from "../lib/types";
import { notice, setUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { Spinner } from "./kit";
import { buttonClass } from "./Sheet";

const inputClass = "selectable h-9 rounded-md bg-field px-3 text-[14px] outline-none placeholder:text-secondary";

/** Friends to tick, filtered by name. `exclude` are already in the group. */
function FriendPicker({ picked, onChange, exclude }: { picked: string[]; onChange: (mids: string[]) => void; exclude: Set<string> }) {
  const { data: friends, isPending } = useQuery(q.friends);
  const me = useSession()?.profile?.mid;
  const [query, setQuery] = useState("");
  const term = query.trim().toLocaleLowerCase("th");
  const shown = useMemo(
    () =>
      (friends ?? []).filter(
        (f) => f.kind === "dm" && f.mid !== me && !exclude.has(f.mid) && (!term || f.name.toLocaleLowerCase("th").includes(term)),
      ),
    [friends, me, exclude, term],
  );
  const toggle = (mid: string) => onChange(picked.includes(mid) ? picked.filter((m) => m !== mid) : [...picked, mid]);
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="ค้นหาด้วยชื่อ" className={inputClass} />
      <div className="min-h-[120px] flex-1 overflow-y-auto">
        {isPending && <Spinner />}
        {!isPending && !shown.length && <p className="px-1 py-3 text-[12.5px] text-secondary">ไม่พบเพื่อน</p>}
        {shown.map((f) => (
          <label key={f.mid} className="flex h-[46px] cursor-pointer items-center gap-3 rounded-md px-1 hover:bg-row-hover">
            <input type="checkbox" checked={picked.includes(f.mid)} onChange={() => toggle(f.mid)} />
            <Avatar id={f.mid} name={f.name} picture={f.picture} size={32} />
            <span className="truncate text-[13.5px]">{f.name}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

/** "สร้างกลุ่ม": a name and the friends to invite. */
export function CreateGroup({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const none = useMemo(() => new Set<string>(), []);
  const create = async () => {
    setBusy(true);
    try {
      const id = await api.createGroup(name.trim(), picked);
      notice("สร้างกลุ่มแล้ว");
      onDone();
      openChat(id);
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 px-5 pt-3 pb-4">
      <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="ชื่อกลุ่ม" className={inputClass} />
      <div className="text-[12px] font-medium text-secondary">เชิญเพื่อน {picked.length ? `(${picked.length})` : ""}</div>
      <FriendPicker picked={picked} onChange={setPicked} exclude={none} />
      <div className="flex justify-end gap-2">
        <button className={buttonClass()} onClick={onDone}>
          ยกเลิก
        </button>
        <button className={buttonClass(true)} disabled={busy || !name.trim() || !picked.length} onClick={create}>
          {busy ? "…" : "สร้าง"}
        </button>
      </div>
    </div>
  );
}

function Person({
  mid,
  name,
  picture,
  note,
  action,
}: {
  mid: string;
  name: string;
  picture: string | null;
  note?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="group flex h-[48px] items-center gap-3 px-1">
      <Avatar id={mid} name={name} picture={picture} size={34} />
      <span className="min-w-0 flex-1 truncate text-[13.5px]">
        {name}
        {note && <span className="ml-1.5 text-[12px] text-secondary">{note}</span>}
      </span>
      {action}
    </div>
  );
}

const smallButton =
  "h-7 rounded-md bg-field px-2.5 text-[12px] text-primary opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-visible:opacity-100";

/** A group's members and invitations: rename, invite, remove a member,
 *  cancel an invitation. */
export function GroupMembers({ chat, onDone }: { chat: Chat; onDone: () => void }) {
  const me = useSession()?.profile;
  const { data: members, isPending } = useQuery({ ...q.members(chat.id) });
  const { data: invitees = [] } = useQuery({ queryKey: ["invitees", chat.id], queryFn: () => api.invitees(chat.id), enabled: !!members });
  const [name, setName] = useState(chat.name);
  const [inviting, setInviting] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["members", chat.id] });
    queryClient.invalidateQueries({ queryKey: ["invitees", chat.id] });
  };
  const run = async (p: Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await p;
      notice(done);
      refresh();
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const inGroup = useMemo(() => new Set([...(members ?? []).map((m) => m.mid), ...invitees.map((m) => m.mid)]), [members, invitees]);
  if (inviting) {
    return (
      <div className="flex min-h-0 flex-1 flex-col gap-3 px-5 pt-3 pb-4">
        <FriendPicker picked={picked} onChange={setPicked} exclude={inGroup} />
        <div className="flex justify-end gap-2">
          <button className={buttonClass()} onClick={() => setInviting(false)}>
            ย้อนกลับ
          </button>
          <button
            className={buttonClass(true)}
            disabled={busy || !picked.length}
            onClick={() => run(api.inviteMembers(chat.id, picked), "เชิญแล้ว").then(() => (setPicked([]), setInviting(false)))}
          >
            {busy ? "…" : `เชิญ${picked.length ? ` (${picked.length})` : ""}`}
          </button>
        </div>
      </div>
    );
  }
  const renamed = name.trim() && name.trim() !== chat.name;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 px-5 pt-3 pb-4">
      <div className="flex gap-2">
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="ชื่อกลุ่ม" className={`${inputClass} flex-1`} />
        <button
          className={buttonClass(true)}
          disabled={busy || !renamed}
          onClick={() => run(api.renameGroup(chat.id, name), "เปลี่ยนชื่อกลุ่มแล้ว")}
        >
          บันทึก
        </button>
      </div>
      <div className="min-h-[160px] flex-1 overflow-y-auto">
        <div className="flex items-center justify-between px-1 pb-1 text-[12px] font-medium text-secondary">
          <span>สมาชิก {members ? members.length + 1 : ""}</span>
          <button className="text-line-green hover:underline" onClick={() => setInviting(true)}>
            เชิญ
          </button>
        </div>
        {isPending && <Spinner />}
        {me && <Person mid={me.mid} name={me.displayName} picture={me.picture} note="(คุณ)" />}
        {(members ?? []).map((m) => (
          <Person
            key={m.mid}
            mid={m.mid}
            name={m.name}
            picture={m.picture ?? null}
            action={
              <button
                className={smallButton}
                disabled={busy}
                onClick={() =>
                  setUi({
                    confirm: {
                      title: `ลบ ${m.name} ออกจากกลุ่มหรือไม่`,
                      action: "ลบ",
                      danger: true,
                      run: () => api.removeMember(chat.id, m.mid).then(refresh),
                    },
                  })
                }
              >
                ลบ
              </button>
            }
          />
        ))}
        {invitees.length > 0 && <div className="px-1 pt-3 pb-1 text-[12px] font-medium text-secondary">เชิญแล้ว {invitees.length}</div>}
        {invitees.map((m) => (
          <Person
            key={m.mid}
            mid={m.mid}
            name={m.name}
            picture={m.picture ?? null}
            action={
              <button
                className={smallButton}
                disabled={busy}
                onClick={() => run(api.cancelInvitation(chat.id, [m.mid]), "ยกเลิกคำเชิญแล้ว")}
              >
                ยกเลิกคำเชิญ
              </button>
            }
          />
        ))}
      </div>
      <div className="flex justify-end">
        <button className={buttonClass()} onClick={onDone}>
          ปิด
        </button>
      </div>
    </div>
  );
}

/** In place of the composer for a group we are invited to. */
export function InvitationBar({ chat }: { chat: Chat }) {
  const [busy, setBusy] = useState(false);
  const answer = async (accept: boolean) => {
    setBusy(true);
    try {
      if (accept) {
        await api.acceptInvitation(chat.id);
        notice("เข้าร่วมกลุ่มแล้ว");
      } else {
        await api.rejectInvitation(chat.id);
        closeChat();
      }
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex shrink-0 flex-col items-center gap-2 border-t border-black/10 px-5 py-4">
      <p className="text-[13px] text-secondary">คุณได้รับคำเชิญเข้ากลุ่ม {chat.name}</p>
      <div className="flex gap-2">
        <button className={buttonClass()} disabled={busy} onClick={() => answer(false)}>
          ปฏิเสธ
        </button>
        <button className={buttonClass(true)} disabled={busy} onClick={() => answer(true)}>
          เข้าร่วม
        </button>
      </div>
    </div>
  );
}

/** Over the message box of a chat from someone who is not a friend, until
 *  it is accepted (LINE keeps these in its message request box). */
export function MessageRequestBar({ chat }: { chat: Chat }) {
  const [busy, setBusy] = useState(false);
  const accept = async () => {
    setBusy(true);
    try {
      await api.acceptMessageRequest(chat.id);
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex shrink-0 items-center gap-3 border-t border-black/10 px-4 py-2.5">
      <p className="flex-1 text-[12.5px] text-secondary">
        {chat.id.startsWith("u") ? "ผู้ส่งยังไม่ได้เป็นเพื่อนกับคุณ" : "คุณถูกเพิ่มในแชทนี้โดยคนที่ไม่ได้เป็นเพื่อน"}
      </p>
      {chat.id.startsWith("u") && (
        <button
          className={buttonClass()}
          disabled={busy}
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
        </button>
      )}
      <button className={buttonClass(true)} disabled={busy} onClick={accept}>
        ยอมรับ
      </button>
    </div>
  );
}

/** The group named in the sheet, from the chat list. */
export function useSheetChat(chatId: string | undefined) {
  return useChatList()?.chats.find((c) => c.id === chatId);
}
