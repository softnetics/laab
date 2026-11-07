import { useState } from "react";
import { REACTIONS, emojiFor, topTypes } from "../lib/reactions";
import type { Reaction, ReactionName } from "../lib/types";
import { Avatar } from "./Avatar";
import { SmileIcon } from "./Icons";
import { Popover } from "./Popover";

/** The hover button that opens the six reactions. */
export function ReactButton({
  mine,
  onPick,
  disabled,
  onOpenChange,
}: {
  mine?: ReactionName;
  onPick: (r: ReactionName) => void;
  disabled?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const [open, setOpenState] = useState(false);
  const setOpen = (o: boolean) => {
    setOpenState(o);
    onOpenChange?.(o);
  };
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="top"
      className="flex gap-1 rounded-full p-1.5"
      trigger={
        <button
          disabled={disabled}
          title="รีแอคชัน"
          className="grid h-7 w-7 place-items-center rounded-full text-secondary hover:text-primary disabled:opacity-40"
        >
          <SmileIcon size={22} strokeWidth={1.4} />
        </button>
      }
    >
      {REACTIONS.map((r) => (
        <button
          key={r.name}
          onClick={() => {
            onPick(r.name);
            setOpen(false);
          }}
          className={`grid h-[38px] w-[38px] place-items-center rounded-full text-[24px] transition-transform hover:scale-115 ${mine === r.name ? "bg-field" : ""}`}
        >
          {emojiFor(r.name)}
        </button>
      ))}
    </Popover>
  );
}

/** Under the bubble: up to three emoji and a count; click for who reacted. */
export function ReactionBadge({
  list,
  name,
  picture,
  me,
}: {
  list: Reaction[];
  name: (mid: string) => string;
  picture: (mid: string) => string | null;
  me: string;
}) {
  const [open, setOpen] = useState(false);
  if (!list.length) return null;
  const types = topTypes(list);
  const people = [...list].sort((a, b) =>
    a.userId === me ? -1 : b.userId === me ? 1 : name(a.userId).localeCompare(name(b.userId), "th"),
  );
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      className="w-[240px] overflow-hidden"
      trigger={
        <button title="ดูว่าใครกดรีแอคชัน" className="flex items-center gap-1 rounded-full">
          <span className="flex">
            {types.map((t, i) => (
              <span
                key={t}
                className={`grid h-[22px] w-[22px] place-items-center rounded-full bg-white text-[13px] ring-[1.5px] ring-chat ${i ? "-ml-1.5" : ""}`}
              >
                {emojiFor(t)}
              </span>
            ))}
          </span>
          {list.length > 1 && <span className="pr-1.5 text-[11px] text-secondary">{list.length}</span>}
        </button>
      }
    >
      <div className="border-b border-separator px-3.5 py-2.5 text-[13px] font-semibold">รีแอคชัน {list.length}</div>
      <div className="overflow-y-auto py-1" style={{ maxHeight: Math.min(list.length * 38 + 8, 280) }}>
        {people.map((r) => (
          <div key={r.userId} className="flex items-center gap-2.5 px-3.5 py-[5px]">
            <Avatar id={r.userId} name={name(r.userId)} picture={picture(r.userId)} size={28} />
            <span className="flex-1 truncate text-[13px]">{r.userId === me ? "คุณ" : name(r.userId)}</span>
            <span className="text-[16px]">{emojiFor(r.reaction)}</span>
          </div>
        ))}
      </div>
    </Popover>
  );
}
