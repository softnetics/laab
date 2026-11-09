import { useState, type ReactNode } from "react";
import { mediaUrl } from "../lib/api";
import basicEmoji from "../lib/basicEmoji.json";
import {
  REACTIONS,
  emojiFor,
  packReaction,
  packTabUrl,
  paidEmoji,
  predefinedArt,
  reactionPackIds,
  readRecentReactions,
  rememberReaction,
  topTypes,
} from "../lib/reactions";
import type { Reaction, ReactionName } from "../lib/types";
import { Avatar } from "./Avatar";
import { ClockIcon, SmileIcon } from "./Icons";
import { Popover } from "./Popover";

type Shelf = "recent" | "basic" | "pack";

/** One 6-track grid for every shelf — basic spans 2 tracks (3-up), pack/recent use 1. */
const shelfGrid = "grid grid-cols-6 gap-1";
/** Two rows of h-9 cells + one gap + p-2. */
const shelfBody = "h-[92px] overflow-y-auto p-2 [scrollbar-gutter:stable]";

/** LINE art for a reaction — bundled Moon faces, or a paid sticon. */
function Glyph({ reaction, size }: { reaction: ReactionName; size: number }) {
  const art = predefinedArt(reaction);
  if (art) {
    return <img src={art} style={{ width: size, height: size }} className="object-contain" draggable={false} alt="" />;
  }
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

/** The hover button that opens basic + LINE emoji reactions. */
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
  const [shelf, setShelf] = useState<Shelf>("basic");
  const [recent, setRecent] = useState(readRecentReactions);
  const setOpen = (o: boolean) => {
    setOpenState(o);
    onOpenChange?.(o);
    if (o) setRecent(readRecentReactions());
  };
  const pick = (r: ReactionName) => {
    setRecent(rememberReaction(r));
    onPick(r);
    setOpen(false);
  };
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="top"
      className="w-[268px] overflow-hidden p-0"
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
      <div className="flex h-10 items-center gap-1.5 border-b border-separator bg-[#f3f3f5] px-2 dark:bg-field/50">
        <ShelfTab title="ล่าสุด" active={shelf === "recent"} onClick={() => setShelf("recent")}>
          <ClockIcon size={15} strokeWidth={2} className="text-secondary" />
        </ShelfTab>
        <ShelfTab title="พื้นฐาน" active={shelf === "basic"} onClick={() => setShelf("basic")}>
          <img src={REACTIONS[0].art} alt="" className="h-6 w-6 object-contain" draggable={false} />
        </ShelfTab>
        <ShelfTab title="อิโมจิ LINE" active={shelf === "pack"} onClick={() => setShelf("pack")}>
          <img src={packTabUrl()} alt="" className="h-6 w-6 object-contain" draggable={false} />
        </ShelfTab>
      </div>
      <div className={shelfBody} aria-label="เลือกปฏิกิริยา">
        {shelf === "recent" &&
          (recent.length === 0 ? (
            <div className="grid h-full place-items-center px-4 text-center text-[12px] text-secondary">
              ยังไม่มีรีแอคชันล่าสุด
            </div>
          ) : (
            <div className={shelfGrid}>
              {recent.map((r) => (
                <PickButton key={r} active={mine === r} onClick={() => pick(r)}>
                  <Glyph reaction={r} size={22} />
                </PickButton>
              ))}
            </div>
          ))}
        {shelf === "basic" && (
          <div className={shelfGrid}>
            {REACTIONS.map((r) => (
              <PickButton key={r.name} span={2} active={mine === r.name} onClick={() => pick(r.name)}>
                <img src={r.art} alt="" className="h-8 w-8 object-contain" draggable={false} />
              </PickButton>
            ))}
          </div>
        )}
        {shelf === "pack" && (
          <div className={shelfGrid}>
            {reactionPackIds.map((id) => {
              const key = packReaction(id);
              return (
                <PickButton key={id} active={mine === key} onClick={() => pick(key)}>
                  <img
                    src={mediaUrl.sticon(basicEmoji.productId, id, 1)}
                    alt=""
                    loading="lazy"
                    className="h-7 w-7 object-contain"
                    draggable={false}
                  />
                </PickButton>
              );
            })}
          </div>
        )}
      </div>
    </Popover>
  );
}

function ShelfTab({
  title,
  active,
  onClick,
  children,
}: {
  title: string;
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={title}
      aria-label={title}
      aria-pressed={active}
      onClick={onClick}
      className={`grid h-8 w-8 place-items-center rounded-[9px] focus-visible:outline-2 focus-visible:outline-line-green ${
        active ? "bg-white shadow-[0_0_0_1px_rgba(0,0,0,0.08),0_1px_3px_rgba(0,0,0,0.12)]" : "hover:bg-white/70"
      }`}
    >
      {children}
    </button>
  );
}

function PickButton({
  active,
  onClick,
  children,
  span = 1,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  span?: 1 | 2;
}) {
  // pointerdown: popover can unmount on focus change before click fires
  return (
    <button
      type="button"
      onPointerDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onClick();
      }}
      className={`grid h-9 w-full place-items-center rounded-lg hover:bg-field ${
        span === 2 ? "col-span-2" : ""
      } ${active ? "bg-field ring-1 ring-separator" : ""}`}
    >
      {children}
    </button>
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
        <button title="ดูว่าใครกดรีแอคชัน" className="flex items-center gap-0.5">
          <span className="flex items-center">
            {types.map((t, i) => (
              <span key={t} className={`grid h-[22px] w-[22px] place-items-center leading-none ${i ? "-ml-0.5" : ""}`}>
                <Glyph reaction={t} size={22} />
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
