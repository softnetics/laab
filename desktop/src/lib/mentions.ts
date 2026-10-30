import type { ChatMember, DraftMention, MentionSpan } from "./types";

// Composer mentions. Offsets are UTF-16 code units, which is what JS strings
// index by and what LINE's MENTION metadata uses.

const isTokenStart = (text: string, i: number) => i === 0 || /[ \t\n\r]/.test(text[i - 1]);

export const memberLabel = (m: ChatMember) => `@${m.all ? "All" : m.name.replace(/\n/g, " ")}`;
export const memberTitle = (m: ChatMember) => (m.all ? "ทั้งหมด" : m.name);

export const ALL_MEMBER: ChatMember = { mid: "all", name: "All", all: true };

interface Located {
  start: number;
  end: number;
  mid: string;
  all: boolean;
}

/** Where each draft mention sits: the first occurrence of its label that
 *  starts a token and does not overlap an earlier one. */
export function locate(text: string, mentions: DraftMention[]): Located[] {
  const used: [number, number][] = [];
  const found: Located[] = [];
  for (const m of mentions) {
    if (!m.label) continue;
    let from = 0;
    while (from < text.length) {
      const at = text.indexOf(m.label, from);
      if (at < 0) break;
      const end = at + m.label.length;
      const overlaps = used.some(([a, b]) => at < b && end > a);
      if (!overlaps && isTokenStart(text, at)) {
        used.push([at, end]);
        found.push({ start: at, end, mid: m.mid, all: m.all });
        break;
      }
      from = at + 1;
    }
  }
  return found.sort((a, b) => a.start - b.start);
}

/** Text after the `@` being typed at the caret. A finished mention is not a query. */
export function composingQuery(text: string, caret: number, mentions: DraftMention[]): string | null {
  const spans = locate(text, mentions);
  for (let i = caret - 1; i >= 0; i--) {
    const c = text[i];
    if (c === "\n" || c === "\r") return null;
    if (c === "@" && isTokenStart(text, i)) {
      if (spans.some((s) => s.start <= i && i < s.end)) return null;
      return text.slice(i + 1, caret);
    }
  }
  return null;
}

/** Replace the `@query` before the caret with the member's label. */
export function insertMention(
  text: string,
  caret: number,
  member: ChatMember,
  mentions: DraftMention[],
): { text: string; caret: number; mentions: DraftMention[] } | null {
  const query = composingQuery(text, caret, mentions);
  if (query == null) return null;
  const start = caret - query.length - 1;
  const label = memberLabel(member);
  if (label.length <= 1 || label.includes("\n")) return null;
  const before = text.slice(0, start) + label + " ";
  return {
    text: before + text.slice(caret),
    caret: before.length,
    mentions: [...mentions, { mid: member.mid, all: !!member.all, label }],
  };
}

/** Drop draft mentions whose label was edited away. Duplicates of
 *  the same label are kept only as many times as they still appear. */
export function present(text: string, mentions: DraftMention[]): DraftMention[] {
  const kept: DraftMention[] = [];
  for (const m of mentions) {
    if (locate(text, [...kept, m]).length > locate(text, kept).length) kept.push(m);
  }
  return kept;
}

export function targets(text: string, mentions: DraftMention[]): MentionSpan[] {
  return locate(text, mentions).map((l) =>
    l.all ? { start: l.start, end: l.end, all: true } : { start: l.start, end: l.end, mid: l.mid },
  );
}

/** Backspace right after a mention deletes the whole tag. */
export function backspaceMention(
  text: string,
  caret: number,
  mentions: DraftMention[],
): { text: string; caret: number; mentions: DraftMention[] } | null {
  const hit = locate(text, mentions).find((s) => s.end === caret);
  if (!hit) return null;
  const next = text.slice(0, hit.start) + text.slice(hit.end);
  return { text: next, caret: hit.start, mentions: present(next, mentions) };
}

export function matching(members: ChatMember[], query: string): ChatMember[] {
  const q = query.trim().toLocaleLowerCase("th");
  return members
    .filter((m) => {
      if (!q) return true;
      if (m.all) return "all".includes(q) || "ทั้งหมด".includes(q);
      return m.name.toLocaleLowerCase("th").includes(q);
    })
    .slice(0, 8);
}
