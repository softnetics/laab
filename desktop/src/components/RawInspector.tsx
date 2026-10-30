import { Dialog } from "@base-ui/react/dialog";
import { useQuery } from "@tanstack/react-query";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, errorText, q } from "../lib/api";
import { readCall, readChatEvent } from "../lib/notices";
import { setUi, useUi } from "../lib/ui";
import { buttonClass, Sheet } from "./Sheet";

const sortKeys = (v: unknown): unknown =>
  Array.isArray(v)
    ? v.map(sortKeys)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.entries(v)
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([k, x]) => [k, sortKeys(x)]),
        )
      : v;

/** Developer tool: the message as LAAB holds it next to what LINE sent. */
export function RawInspector({ chatId }: { chatId: string }) {
  const id = useUi((s) => s.inspectMessageId);
  const { data: messages } = useQuery(q.messages(chatId));
  const message = messages?.find((m) => m.id === id);
  const raw = useQuery({
    queryKey: ["raw", chatId, id],
    queryFn: () => api.rawMessage(chatId, id!),
    enabled: !!id,
    staleTime: 0,
  });
  const local = message
    ? JSON.stringify(sortKeys({ ...message, call: readCall(message), chatEvent: readChatEvent(message) }), null, 2)
    : "";
  const remote = raw.isLoading ? "กำลังโหลด…" : raw.error ? errorText(raw.error) : (raw.data ?? "");
  const close = () => setUi({ inspectMessageId: null });
  return (
    <Sheet open={!!id} onClose={close} className="h-[min(640px,calc(100vh-48px))] w-[min(880px,calc(100vw-48px))] min-w-[600px]">
      <div className="flex items-center gap-3 border-b border-separator px-5 py-3">
        <Dialog.Title className="text-[15px] font-semibold">ข้อมูลดิบของข้อความ</Dialog.Title>
        <code className="selectable text-[12px] text-secondary">{id}</code>
        <div className="flex-1" />
        <button className={buttonClass()} onClick={close}>
          ปิด
        </button>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-separator">
        {[
          ["ใน LAAB", local],
          ["จาก LINE", remote],
        ].map(([title, body]) => (
          <div key={title} className="flex min-h-0 flex-col">
            <div className="flex items-center px-4 py-2">
              <span className="text-[13px] font-semibold">{title}</span>
              <div className="flex-1" />
              <button className={buttonClass(true)} onClick={() => writeText(body)}>
                คัดลอก
              </button>
            </div>
            <pre className="selectable min-h-0 flex-1 overflow-auto px-4 pb-4 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap">
              {body}
            </pre>
          </div>
        ))}
      </div>
    </Sheet>
  );
}
