import { useMemo, useState } from "react";
import { useChatList, useSession } from "../lib/api";
import { isKeepMemo } from "../lib/chatList";
import { openChat } from "../lib/nav";
import type { Chat } from "../lib/types";
import { setUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { BookmarkIcon, ChevronIcon, PersonPlusIcon, SearchIcon } from "./Icons";
import { Spinner } from "./kit";

const open = (id: string) => {
  setUi({ railPage: "chats" });
  openChat(id);
};

function Section({ title, items }: { title: string; items: Chat[] }) {
  const [shown, setShown] = useState(true);
  if (!items.length) return null;
  return (
    <div className="pt-2">
      <button onClick={() => setShown(!shown)} className="flex w-full items-center px-4 py-1.5 text-[12px] text-secondary">
        <span className="flex-1 text-left">
          {title} {items.length}
        </span>
        <ChevronIcon dir={shown ? "up" : "down"} size={11} />
      </button>
      {shown &&
        items.map((c) => (
          <button
            key={c.id}
            onClick={() => open(c.id)}
            className="flex h-[50px] w-full items-center gap-3 px-4 text-left hover:bg-row-hover"
          >
            <Avatar id={c.id} name={c.name} size={38} picture={c.picture} local={c.avatar} />
            <span className="truncate text-[13.5px]">
              {c.name}
              {c.kind === "group" && c.memberCount ? ` (${c.memberCount})` : ""}
            </span>
          </button>
        ))}
    </div>
  );
}

/** LINE for Mac's friends page: search by name, your profile with Keep
 *  Memo, then groups and friends. */
export function FriendsPage() {
  const list = useChatList();
  const s = useSession();
  const [query, setQuery] = useState("");
  const me = s?.profile;
  const q = query.trim().toLocaleLowerCase("th");
  const match = (c: Chat) => !q || c.name.toLocaleLowerCase("th").includes(q);
  const byName = (a: Chat, b: Chat) => a.name.localeCompare(b.name, "th");
  const groups = useMemo(() => (list?.chats ?? []).filter((c) => c.kind === "group" && match(c)).sort(byName), [list, q]);
  const friends = useMemo(
    () => (list?.chats ?? []).filter((c) => c.kind === "dm" && !isKeepMemo(c, me?.mid) && match(c)).sort(byName),
    [list, q, me?.mid],
  );
  return (
    <div className="flex h-full flex-col bg-list">
      <div className="drag px-4 pt-[34px] pb-2" data-tauri-drag-region>
        <label className="no-drag flex h-8 items-center gap-2 rounded-md bg-field px-2.5 text-secondary">
          <SearchIcon size={14} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="ค้นหาด้วยชื่อ"
            className="selectable min-w-0 flex-1 bg-transparent text-[13px] text-primary outline-none placeholder:text-secondary"
          />
        </label>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-4">
        {me && !q && (
          <div className="flex items-center gap-3 px-4 py-2.5">
            <Avatar id={me.mid} name={me.displayName} picture={me.picture} size={52} />
            <div className="min-w-0">
              <div className="truncate text-[14px]">{me.displayName}</div>
              <button
                onClick={() => open(me.mid)}
                className="mt-1 flex items-center gap-1 rounded-full border border-secondary/40 px-2.5 py-0.5 text-[11.5px] hover:bg-row-hover"
              >
                <BookmarkIcon size={11} /> Keep Memo
              </button>
            </div>
          </div>
        )}
        <Section title="กลุ่ม" items={groups} />
        <Section title="เพื่อน" items={friends} />
        {!list ? (
          <div className="grid place-items-center pt-16 text-secondary">
            <Spinner />
          </div>
        ) : (
          !groups.length &&
          !friends.length && (
            <div className="px-8 pt-12 text-center text-[13px] leading-relaxed text-secondary">
              {q ? "ไม่พบเพื่อนหรือกลุ่มที่ชื่อตรงกัน" : "ยังไม่มีเพื่อนหรือกลุ่ม เพิ่มเพื่อนได้จากหน้าเพิ่มเพื่อน"}
            </div>
          )
        )}
      </div>
    </div>
  );
}

/** LINE for Mac's add-friend page; LAAB can search by ID or link and join
 *  groups by invite link (creating groups and OpenChat are not supported). */
export function AddFriendPage() {
  return (
    <div className="flex h-full flex-col bg-list pt-[38px]">
      <button
        onClick={() => setUi({ sheet: { kind: "add-friend" } })}
        className="flex h-[50px] items-center gap-3 px-4 text-left text-[13.5px] hover:bg-row-hover"
      >
        <span className="grid h-9 w-9 place-items-center rounded-full bg-field">
          <PersonPlusIcon size={18} />
        </span>
        ค้นหาเพื่อน
      </button>
      <p className="px-4 pt-3 text-[12px] leading-relaxed text-secondary">
        ค้นหาด้วย LINE ID หรือลิงก์เพิ่มเพื่อน หรือวางลิงก์เชิญเพื่อเข้ากลุ่ม
      </p>
    </div>
  );
}
