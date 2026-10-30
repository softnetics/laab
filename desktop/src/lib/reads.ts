import type { Message } from "./types";

// Where each other person has read up to. A receipt means "read through this
// message", so older messages do not each grow their own mark.

export const isNumericId = (value: string) => value.length > 0 && /^\d+$/.test(value);

/** Exact compare for LINE's 64-bit message ids, including ones past 2^53. */
export function compareIds(a: string, b: string): -1 | 0 | 1 | null {
  if (!isNumericId(a) || !isNumericId(b)) return null;
  const x = a.replace(/^0+(?=\d)/, "");
  const y = b.replace(/^0+(?=\d)/, "");
  if (x.length !== y.length) return x.length < y.length ? -1 : 1;
  if (x === y) return 0;
  return x < y ? -1 : 1;
}

/** A newer receipt wins. An unusable id never pulls a cursor backwards. */
export function later(current: string | undefined, incoming: string): string {
  if (!incoming) return current ?? "";
  if (!current) return incoming;
  const order = compareIds(incoming, current);
  if (order === 1) return incoming;
  if (order === null) return isNumericId(incoming) && !isNumericId(current) ? incoming : current;
  return current;
}

/** Message id → reader mids whose cursor lands on it. In a 1:1 the mark sits
 *  on your own message; in a group on the message each person reached. */
export function placement(messages: Message[], cursors: Record<string, string>, direct: boolean): Record<string, string[]> {
  const pool = direct ? messages.filter((m) => m.mine) : messages;
  const placed: Record<string, string[]> = {};
  for (const user of Object.keys(cursors).sort()) {
    const cursor = cursors[user];
    if (!cursor) continue;
    const hit = watermark(pool, cursor);
    if (hit) (placed[hit.id] ??= []).push(user);
  }
  return placed;
}

function watermark(messages: Message[], cursor: string): Message | undefined {
  if (isNumericId(cursor)) {
    let best: Message | undefined;
    for (const m of messages) {
      const order = compareIds(m.id, cursor);
      if (order === null || order === 1) continue;
      if (!best || compareIds(best.id, m.id) === -1) best = m;
    }
    return best;
  }
  return [...messages].reverse().find((m) => m.id === cursor);
}

/** LINE's way: each of your own messages lists everyone who has read at
 *  least that far. Computed once per render: readers are sorted by how far
 *  they read, then each message takes a suffix by binary search. */
export function readersOf(messages: Message[], cursors: Record<string, string>): Record<string, string[]> {
  const reached: { user: string; pos: number }[] = [];
  for (const user of Object.keys(cursors)) {
    const cursor = cursors[user];
    if (!cursor) continue;
    const hit = watermark(messages, cursor);
    if (hit) reached.push({ user, pos: messages.indexOf(hit) });
  }
  reached.sort((a, b) => a.pos - b.pos || a.user.localeCompare(b.user));
  const out: Record<string, string[]> = {};
  messages.forEach((m, i) => {
    if (!m.mine) return;
    let lo = 0;
    let hi = reached.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (reached[mid].pos < i) lo = mid + 1;
      else hi = mid;
    }
    if (lo < reached.length) out[m.id] = reached.slice(lo).map((r) => r.user);
  });
  return out;
}
