import type { Message } from "./types";

/** Whether a photo is animated (a GIF), from its `MEDIA_CONTENT_INFO`
 *  (`{"animated":true,"extension":"gif",…}`). LINE plays these in the chat;
 *  their thumbnail is a still. */
export function isAnimated(m: Message): boolean {
  if (m.contentType !== "IMAGE") return false;
  const raw = m.metadata.MEDIA_CONTENT_INFO;
  if (!raw) return false;
  try {
    const info = JSON.parse(raw) as { animated?: unknown; extension?: unknown };
    return info.animated === true || info.animated === "true" || String(info.extension ?? "").toLowerCase() === "gif";
  } catch {
    return false;
  }
}
