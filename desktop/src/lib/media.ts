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

/** A LINE emoji (sticon) drawn over part of a message's text. */
export interface Sticon {
  /** UTF-16 offsets of the text it replaces, end excluded. */
  start: number;
  end: number;
  productId: string;
  sticonId: string;
  version: number;
  animated: boolean;
}

/** The sticons of a text message, from its `REPLACE` metadata:
 *  `{"sticon":{"resources":[{"S","E","productId","sticonId","version","resourceType"}]}}`
 *  (LINE for Android `so8/c.java`). Overlapping or out-of-range ones are left out. */
export function sticons(metadata: Record<string, string>, textLength: number): Sticon[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(metadata.REPLACE ?? "");
  } catch {
    return [];
  }
  const resources = (parsed as { sticon?: { resources?: unknown } })?.sticon?.resources;
  if (!Array.isArray(resources)) return [];
  const out: Sticon[] = [];
  for (const r of resources as Record<string, unknown>[]) {
    const start = Number(r.S);
    const end = Number(r.E);
    const productId = String(r.productId ?? "");
    const sticonId = String(r.sticonId ?? "");
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end <= start || end > textLength) continue;
    if (!/^[A-Za-z0-9_-]+$/.test(productId) || !/^[A-Za-z0-9_-]+$/.test(sticonId)) continue;
    out.push({ start, end, productId, sticonId, version: Number(r.version) || 1, animated: r.resourceType === "ANIMATION" });
  }
  out.sort((a, b) => a.start - b.start);
  let end = 0;
  return out.filter((s) => s.start >= end && ((end = s.end), true));
}

/** A message of one to three LINE emoji and nothing else shows them big and
 *  with no bubble, as LINE does: 72pt for one or two, 52pt for three. Else
 *  null, and they sit in the text at its size. */
export function bigSticonSize(text: string, list: Sticon[]): number | null {
  if (!list.length || list.length > 3) return null;
  let at = 0;
  for (const s of list) {
    if (text.slice(at, s.start).trim()) return null;
    at = s.end;
  }
  if (text.slice(at).trim()) return null;
  return list.length === 3 ? 52 : 72;
}
