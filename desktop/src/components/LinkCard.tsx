import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { previewUrl } from "../lib/links";
import { openUrl } from "../lib/open";
import { WarningIcon } from "./Icons";

export { previewUrl };

/** A link's preview under the message, as LINE's app shows it. LINE's own
 *  service reads the page; the site never sees this Mac. */
export function LinkCard({ url, chatId, mine }: { url: string; chatId: string; mine: boolean }) {
  const { data } = useQuery({ queryKey: ["link-preview", url], queryFn: () => api.linkPreview(url), staleTime: Infinity });
  if (!data) return null;
  if (data.warning) {
    return (
      <div className="mt-1.5 flex items-start gap-1.5 text-[12px] text-badge-red">
        <WarningIcon size={14} />
        <span>{data.warning === "phishing" ? "LINE เตือนว่าลิงก์นี้อาจเป็นฟิชชิง" : "LINE ไม่แสดงตัวอย่างของลิงก์นี้"}</span>
      </div>
    );
  }
  return (
    <button
      onClick={(e) => (e.stopPropagation(), openUrl(data.url || url, chatId))}
      className={`mt-1.5 block w-[240px] max-w-full overflow-hidden rounded-lg text-left ${mine ? "bg-black/5" : "bg-field"}`}
    >
      {data.image && <img src={data.image} loading="lazy" className="h-[126px] w-full object-cover" alt="" draggable={false} />}
      <div className="px-2.5 py-2 leading-snug">
        {data.title && <div className="line-clamp-2 text-[13px] font-medium">{data.title}</div>}
        {data.summary && <div className="mt-0.5 line-clamp-2 text-[12px] opacity-70">{data.summary}</div>}
        {data.domain && <div className="mt-1 truncate text-[11px] opacity-55">{data.domain}</div>}
      </div>
    </button>
  );
}
