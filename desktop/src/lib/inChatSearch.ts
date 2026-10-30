import type { Message } from "./types";

// Search the messages already loaded in the open chat.

const SKIP = new Set(["IMAGE", "VIDEO", "STICKER", "FILE", "AUDIO"]);

/** Case- and width-insensitive form used for matching. */
export const fold = (s: string) => s.normalize("NFKC").toLocaleLowerCase("th");

/** Ids of matching messages, newest first. */
export function findHits(messages: Message[], query: string): string[] {
  const q = fold(query.trim());
  if (!q) return [];
  const hits: string[] = [];
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (SKIP.has(m.contentType) || m.unsent) continue;
    if (fold(m.text).includes(q)) hits.push(m.id);
  }
  return hits;
}

/** Ranges of `query` inside `text`, for highlighting. NFKC can change
 *  lengths, so only the plain lowercase form is used for positions. */
export function ranges(text: string, query: string): [number, number][] {
  const q = query.trim().toLocaleLowerCase("th");
  if (!q) return [];
  const t = text.toLocaleLowerCase("th");
  const out: [number, number][] = [];
  let from = 0;
  while (true) {
    const at = t.indexOf(q, from);
    if (at < 0) break;
    out.push([at, at + q.length]);
    from = at + q.length;
  }
  return out;
}
