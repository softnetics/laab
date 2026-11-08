import { useState } from "react";
import { mediaUrl } from "../lib/api";
import { REACTIONS, emojiFor, paidEmoji, topTypes } from "../lib/reactions";
import type { Reaction, ReactionName } from "../lib/types";
import { Avatar } from "./Avatar";
import { SmileIcon } from "./Icons";
import { Popover } from "./Popover";

/** A reaction's emoji; a paid one's LINE emoji as a picture. */
function Glyph({ reaction, size }: { reaction: ReactionName; size: number }) {
  const paid = paidEmoji(reaction);
  if (!paid) return <>{emojiFor(reaction)}</>;
  const still = mediaUrl.sticon(paid.productId, paid.emojiId, paid.version);
  return (
    <img
      src={mediaUrl.sticon(paid.productId, paid.emojiId, paid.version, paid.animated)}
      onError={(e) => e.currentTarget.src !== still && (e.currentTarget.src = still)}
      style={{ width: size, height: size }}
      className="object-contain"
      draggable={false}
      alt=""
    />
  );
}

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

/** Nested on the bubble's bottom corner: up to three art chips and a count. */
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
        <button
          title="ดูว่าใครกดรีแอคชัน"
          className="flex items-center gap-0.5 rounded-full bg-white py-[3px] pr-1.5 pl-[3px] shadow-[0_1px_4px_rgba(0,0,0,0.16)] ring-1 ring-black/6"
        >
          <span className="flex items-center">
            {types.map((t, i) => (
              <span key={t} className={`grid h-[20px] w-[20px] place-items-center leading-none ${i ? "-ml-0.5" : ""}`}>
                <Glyph reaction={t} size={18} />
              </span>
            ))}
          </span>
          {list.length > 1 && <span className="pr-0.5 text-[11px] leading-none text-secondary">{list.length}</span>}
        </button>
      }
    >
      <div className="border-b border-separator px-3.5 py-2.5 text-[13px] font-semibold">รีแอคชัน {list.length}</div>
      <div className="overflow-y-auto py-1" style={{ maxHeight: Math.min(list.length * 38 + 8, 280) }}>
        {people.map((r) => (
          <div key={r.userId} className="flex items-center gap-2.5 px-3.5 py-[5px]">
            <Avatar id={r.userId} name={name(r.userId)} picture={picture(r.userId)} size={28} />
            <span className="flex-1 truncate text-[13px]">{r.userId === me ? "คุณ" : name(r.userId)}</span>
            <Glyph reaction={r.reaction} size={20} />
          </div>
        ))}
      </div>
    </Popover>
  );
}
