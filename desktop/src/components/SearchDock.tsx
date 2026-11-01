import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useChatList } from "../lib/api";
import { openChat } from "../lib/nav";
import { setUi, useUi } from "../lib/ui";
import { CloseIcon } from "./Icons";
import { Spinner } from "./kit";
import { PhoneHeader } from "./PhoneHeader";
import { HitRow, searchQuery } from "./SearchPalette";

/** ⌘K's results in the list column of the unfolded iPhone Duo. Each tap
 *  opens that hit in the chat beside the list, so the results can be read
 *  one after another without going back. Tapping the query reopens ⌘K. */
export function SearchDock() {
  const dock = useUi((s) => s.searchDock)!;
  const { data: hits = [], isFetching } = useQuery(searchQuery(dock.query, dock.kinds));
  const chats = useChatList()?.chats;
  const byId = useMemo(() => new Map((chats ?? []).map((c) => [c.id, c])), [chats]);
  const show = (i: number) => {
    const h = hits[i];
    if (!h) return;
    setUi({ searchDock: { ...dock, index: i } });
    openChat(h.chatId, { messageId: h.messageId, focusComposer: false });
  };
  return (
    <div className="flex h-full flex-col bg-list">
      <PhoneHeader
        title={
          <button
            className="pointer-events-auto max-w-full truncate"
            onClick={() => setUi({ searchOpen: true, searchInitial: dock.query })}
          >
            “{dock.query}”
          </button>
        }
        right={
          <button title="ปิดผลการค้นหา" onClick={() => setUi({ searchDock: null })} className="grid h-11 w-11 place-items-center">
            <CloseIcon size={20} />
          </button>
        }
      />
      <div className="px-4 pb-1.5 text-[12px] text-secondary">
        {isFetching && !hits.length ? <Spinner size={14} /> : `${hits.length} ผลลัพธ์`}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto" style={{ paddingBottom: "env(safe-area-inset-bottom)" }}>
        {hits.map((h, i) => (
          <HitRow
            key={`${h.kind}:${h.chatId}:${h.messageId ?? ""}`}
            hit={h}
            chat={byId.get(h.chatId)}
            index={i}
            selected={i === dock.index}
            onClick={() => show(i)}
          />
        ))}
      </div>
    </div>
  );
}
