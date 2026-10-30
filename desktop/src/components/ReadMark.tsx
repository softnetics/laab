import { useState } from "react";
import type { ChatMember } from "../lib/types";
import { Avatar } from "./Avatar";
import { Popover } from "./Popover";

export interface Reader {
  mid: string;
  name: string;
  picture?: string | null;
}

/** "อ่านแล้ว" in a 1:1; in a group, reader avatars that open the full list. */
export function ReadMark({ direct, readers, onOpen }: { direct: boolean; readers: Reader[]; onOpen?: () => void }) {
  const [open, setOpen] = useState(false);
  if (!readers.length) return null;
  if (direct) return <span className="text-[11px] text-secondary">อ่านแล้ว</span>;
  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (o) onOpen?.();
      }}
      side="top"
      className="w-[220px] overflow-hidden"
      trigger={
        <button
          className="text-[11px] text-secondary hover:text-primary"
          title={`อ่านแล้วโดย ${readers.map((r) => r.name).join(", ")}`}
          aria-label={`อ่านแล้ว ${readers.length} คน`}
        >
          อ่านแล้ว {readers.length}
        </button>
      }
    >
      <div className="border-b border-separator px-3.5 py-2.5 text-[13px] font-semibold">อ่านแล้ว {readers.length} คน</div>
      <div className="overflow-y-auto py-1" style={{ maxHeight: Math.min(readers.length * 38 + 8, 280) }}>
        {readers.map((r) => (
          <div key={r.mid} className="flex items-center gap-2.5 px-3.5 py-[5px]">
            <Avatar id={r.mid} name={r.name} picture={r.picture} size={28} />
            <span className="truncate text-[13px]">{r.name}</span>
          </div>
        ))}
      </div>
    </Popover>
  );
}

export function readerList(
  ids: string[],
  name: (mid: string) => string,
  people: Record<string, { picture: string | null }>,
  members?: ChatMember[],
): Reader[] {
  const pic = (mid: string) => members?.find((m) => m.mid === mid)?.picture ?? people[mid]?.picture ?? null;
  return ids.map((mid) => ({ mid, name: name(mid), picture: pic(mid) })).sort((a, b) => a.name.localeCompare(b.name, "th"));
}
