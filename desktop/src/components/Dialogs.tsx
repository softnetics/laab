import { useRef, useState } from "react";
import { api, errorText } from "../lib/api";
import { openChat } from "../lib/nav";
import { notice, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { Spinner } from "./kit";
import { buttonClass, Sheet } from "./Sheet";

const inputClass = "selectable h-9 rounded-md bg-field px-3 text-[14px] outline-none placeholder:text-secondary";

/** "Are you sure?" for unsend, delete history, block, and similar. */
export function ConfirmDialog() {
  const c = useUi((s) => s.confirm);
  const [busy, setBusy] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  const close = () => setUi({ confirm: null });
  const go = async () => {
    if (!c) return;
    setBusy(true);
    try {
      await c.run();
      close();
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  // LINE for Mac: centred title and text, a wide green button, "ปิด" under it.
  return (
    // Focus starts on "ปิด", so a stray Return does not unsend or delete.
    <Sheet open={!!c} onClose={close} initialFocus={cancel} className="w-[280px] rounded-[12px]! bg-menu!">
      <div className="flex flex-col items-center gap-3 px-6 pt-6 pb-3 text-center">
        <div className="text-[15px] font-bold text-menu-text">{c?.title}</div>
        {c?.body && <p className="text-[12.5px] leading-[1.45] whitespace-pre-line text-secondary">{c.body}</p>}
        <button
          className={`mt-2 h-10 w-[200px] rounded-[4px] text-[13.5px] font-semibold text-white disabled:opacity-60 ${c?.danger ? "bg-badge-red" : "bg-line-green"}`}
          disabled={busy}
          onClick={go}
        >
          {busy ? "…" : c?.action}
        </button>
        <button ref={cancel} className="h-9 text-[13.5px] font-semibold text-menu-text" onClick={close}>
          ปิด
        </button>
      </div>
    </Sheet>
  );
}

/** Add a friend by LINE ID or link, or join a group by its invite link. */
function AddFriend() {
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<{ mid: string; name: string; picture: string | null } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const groupLink = /line\.me\/R\/ti\/g\/|line:\/\/ti\/g\//i.test(query);
  const search = async () => {
    if (!query.trim()) return;
    setBusy(true);
    setError(null);
    setFound(null);
    try {
      if (groupLink) {
        const mid = await api.joinGroupLink(query);
        setUi({ sheet: null });
        notice("เข้ากลุ่มแล้ว");
        if (mid) openChat(mid);
      } else {
        setFound(await api.findContact(query));
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const add = async () => {
    if (!found) return;
    setBusy(true);
    try {
      await api.addFriend(found.mid);
      notice(`เพิ่ม ${found.name} เป็นเพื่อนแล้ว`);
      setUi({ sheet: null });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-3 p-5 pt-2">
      <p className="text-[12.5px] text-secondary">ใส่ LINE ID, ลิงก์เพิ่มเพื่อน หรือลิงก์เชิญเข้ากลุ่ม</p>
      <div className="flex gap-2">
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && search()}
          placeholder="LINE ID หรือ https://line.me/…"
          className={`${inputClass} flex-1`}
        />
        <button className={buttonClass(true)} disabled={busy || !query.trim()} onClick={search}>
          {groupLink ? "เข้ากลุ่ม" : "ค้นหา"}
        </button>
      </div>
      {busy && <Spinner />}
      {error && <p className="text-[12.5px] text-badge-red">{error}</p>}
      {found && (
        <div className="flex items-center gap-3 rounded-lg bg-field p-3">
          <Avatar id={found.mid} name={found.name} picture={found.picture} size={44} />
          <span className="flex-1 truncate text-[14px] font-medium">{found.name}</span>
          <button className={buttonClass(true)} disabled={busy} onClick={add}>
            เพิ่มเพื่อน
          </button>
        </div>
      )}
    </div>
  );
}

/** The sheet for adding friends, notes or albums, whichever is open. */
export function Sheets() {
  const sheet = useUi((s) => s.sheet);
  const close = () => setUi({ sheet: null });
  return (
    <Sheet open={sheet?.kind === "add-friend"} onClose={close} className="w-[440px]" title="ค้นหาเพื่อน">
      <AddFriend />
    </Sheet>
  );
}
