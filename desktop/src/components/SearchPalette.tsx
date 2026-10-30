import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorText, useChatList } from "../lib/api";
import { openChat } from "../lib/nav";
import type { SearchKind } from "../lib/types";
import { setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { DocIcon, SearchIcon } from "./Icons";
import { Spinner } from "./kit";
import { Sheet } from "./Sheet";

const KINDS: { id: SearchKind; label: string }[] = [
  { id: "chat", label: "แชท" },
  { id: "message", label: "ข้อความ" },
  { id: "file", label: "ไฟล์" },
];

/** ⌘K: search chats, synced messages and file names. */
export function SearchPalette() {
  const open = useUi((s) => s.searchOpen);
  const initial = useUi((s) => s.searchInitial);
  const [query, setQuery] = useState("");
  const [kinds, setKinds] = useState<SearchKind[]>(["chat", "message", "file"]);
  const [index, setIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const chats = useChatList()?.chats;
  const byId = useMemo(() => new Map((chats ?? []).map((c) => [c.id, c])), [chats]);
  useEffect(() => {
    if (open) {
      setQuery(initial);
      setIndex(0);
    }
  }, [open, initial]);
  const { data: hits = [], isFetching, error } = useQuery({
    queryKey: ["search", query, kinds],
    queryFn: () => api.search(query, kinds),
    enabled: open,
    placeholderData: (prev) => prev,
    staleTime: 0,
  });
  useEffect(() => setIndex(0), [query, kinds]);
  // Only ↑/↓ scroll the list, and only as far as the selected row needs.
  // Selecting by hover must not scroll: the list would move under the
  // pointer, which selects another row, which scrolls again.
  const keyed = useRef(false);
  useEffect(() => {
    if (!keyed.current) return;
    keyed.current = false;
    listRef.current?.querySelector(`[data-i="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);
  // WebKit sends a mousemove when the list scrolls under a still pointer;
  // only a pointer that really moved selects a row.
  const pointer = useRef({ x: -1, y: -1 });
  const hover = (e: React.MouseEvent, i: number) => {
    if (e.clientX === pointer.current.x && e.clientY === pointer.current.y) return;
    pointer.current = { x: e.clientX, y: e.clientY };
    setIndex(i);
  };
  const close = () => setUi({ searchOpen: false });
  const pick = (i: number) => {
    const h = hits[i];
    if (h) openChat(h.chatId, { messageId: h.messageId });
  };
  return (
    // ⌘K is used many times a day: no animation, like Spotlight.
    <Sheet open={open} onClose={close} instant className="h-[520px] w-[640px]">
      <div className="flex items-center gap-3 border-b border-separator px-5 py-4">
        <SearchIcon size={18} className="text-secondary" />
        <input
          autoFocus
          className="selectable flex-1 bg-transparent text-[17px] outline-none placeholder:text-secondary"
          placeholder="ค้นหาแชท ข้อความ หรือชื่อไฟล์"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing || e.metaKey || e.altKey || e.ctrlKey) return;
            if (e.key === "ArrowDown") {
              e.preventDefault();
              keyed.current = true;
              setIndex((i) => Math.min(hits.length - 1, i + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              keyed.current = true;
              setIndex((i) => Math.max(0, i - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              pick(index);
            }
          }}
        />
      </div>
      <div className="flex items-center gap-2 px-5 py-2.5">
        {KINDS.map((k) => {
          const on = kinds.includes(k.id);
          return (
            <button
              key={k.id}
              onClick={() => setKinds(on ? kinds.filter((x) => x !== k.id) : [...kinds, k.id])}
              className={`h-[26px] rounded-full px-3 text-[12px] ${on ? "bg-line-green text-white" : "bg-field text-primary"}`}
            >
              {k.label}
            </button>
          );
        })}
        <span className="ml-auto text-[12px] text-secondary">{query.trim() ? `${hits.length} ผลลัพธ์` : "แชทล่าสุด"}</span>
      </div>
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-2">
        {hits.map((h, i) => {
          const chat = byId.get(h.chatId);
          return (
            <div
              key={`${h.kind}:${h.chatId}:${h.messageId ?? ""}`}
              data-i={i}
              onMouseMove={(e) => hover(e, i)}
              onClick={() => pick(i)}
              className={`mx-2 flex items-center gap-3 rounded-lg px-3 py-2 ${i === index ? "bg-row-selected" : ""}`}
            >
              {h.kind === "file" ? (
                <div className="grid h-10 w-10 place-items-center rounded-lg bg-field text-secondary">
                  <DocIcon size={20} />
                </div>
              ) : (
                <Avatar id={h.chatId} name={chat?.name ?? h.title} size={40} picture={chat?.picture} local={chat?.avatar} />
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate text-[14px] font-medium">{h.title}</div>
                <div className="line-clamp-2 text-[12px] text-secondary">{h.detail}</div>
              </div>
              <span className="text-[11px] text-secondary">{KINDS.find((k) => k.id === h.kind)?.label}</span>
            </div>
          );
        })}
        {query.trim() && hits.length === 0 && (
          <div className="grid place-items-center pt-16 text-center text-[13px] text-secondary">
            {error ? `ค้นหาไม่สำเร็จ: ${errorText(error)}` : isFetching ? <Spinner size={18} /> : "ไม่พบผลลัพธ์"}
          </div>
        )}
      </div>
    </Sheet>
  );
}
