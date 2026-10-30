import { useState } from "react";
import { openUrl } from "../lib/open";

// LINE image-map ("RICH") messages: an image from DOWNLOAD_URL/<width> with
// tap areas from MARKUP_JSON, the way LINE shows official-account ads.

type Markup = {
  canvas?: { width?: number; height?: number };
  actions?: Record<string, { type?: string; params?: { linkUri?: string; text?: string } }>;
  scenes?: Record<string, { listeners?: { type?: string; action?: string; params?: number[] }[] }>;
};

export function RichImage({ meta, chatId, width = 300 }: { meta: Record<string, string>; chatId: string; width?: number }) {
  const [failed, setFailed] = useState(false);
  const base = meta.DOWNLOAD_URL;
  let markup: Markup = {};
  try {
    markup = JSON.parse(meta.MARKUP_JSON ?? "{}");
  } catch {
    /* plain alt text below */
  }
  if (!base || failed) return null;
  const cw = markup.canvas?.width ?? 1040;
  const ch = markup.canvas?.height ?? 1040;
  const scale = width / cw;
  const areas = Object.values(markup.scenes ?? {}).flatMap((s) => s.listeners ?? []);
  return (
    <div className="relative overflow-hidden rounded-2xl" style={{ width, height: ch * scale }}>
      <img
        src={`${base}/700`}
        alt={meta.ALT_TEXT ?? ""}
        onError={() => setFailed(true)}
        draggable={false}
        className="block h-full w-full object-cover"
      />
      {areas.map((a, i) => {
        const action = a.action ? markup.actions?.[a.action] : undefined;
        const uri = action?.params?.linkUri;
        const [x, y, w, h] = a.params ?? [];
        if (!uri || w == null) return null;
        return (
          <button
            key={i}
            title={uri}
            onClick={() => openUrl(uri, chatId)}
            className="absolute cursor-pointer"
            style={{ left: x * scale, top: y * scale, width: w * scale, height: h * scale }}
          />
        );
      })}
    </div>
  );
}
