import type { Chat, ChatList, Workspace } from "./types";
import type { ChatFilter, ChatSort } from "./ui";

export const isGroupId = (id: string) => id.startsWith("c") || id.startsWith("r");
export const isGroup = (c: Pick<Chat, "id" | "kind">) => c.kind === "group" || isGroupId(c.id);

/** "เขา: " / "คุณ: " prefixes are dropped in the list, as LINE does. */
export const listPreview = (p: string) => p.replace(/^(เขา|คุณ|They|You): /, "");

export function filterChats(list: ChatList, filter: ChatFilter, sort: ChatSort, query: string): Chat[] {
  const pinned = list.pins.main ?? [];
  // Chats in the message request box stay out of every list but their
  // own, as in LINE.
  let chats = filter === "requests" ? list.chats.filter((c) => c.request) : list.chats.filter((c) => !c.request);
  if (filter === "friends") chats = chats.filter((c) => c.kind === "dm");
  else if (filter === "groups") chats = chats.filter((c) => c.kind === "group");
  else if (filter === "official") chats = chats.filter((c) => c.kind === "bot");
  else if (filter.startsWith("folder:")) {
    const ids = new Set(list.pins[filter.slice(7)] ?? []);
    chats = chats.filter((c) => ids.has(c.id));
  }
  const q = query.trim().toLocaleLowerCase("th");
  if (q) chats = chats.filter((c) => c.name.toLocaleLowerCase("th").includes(q) || c.preview.toLocaleLowerCase("th").includes(q));
  const byTime = (a: Chat, b: Chat) => b.lastActivity - a.lastActivity;
  const sorted = [...chats].sort(sort === "unread" ? (a, b) => Number(looksUnread(b)) - Number(looksUnread(a)) || byTime(a, b) : byTime);
  const rank = new Map(pinned.map((id, i) => [id, i]));
  return [
    ...sorted.filter((c) => rank.has(c.id)).sort((a, b) => rank.get(a.id)! - rank.get(b.id)!),
    ...sorted.filter((c) => !rank.has(c.id)),
  ];
}

/** The message request box entry: how many chats wait and whether any is
 *  unread (LINE lights a dot on the entry for a new request). */
export function requestBox(list: ChatList): { count: number; unread: boolean } {
  const waiting = list.chats.filter((c) => c.request);
  return { count: waiting.length, unread: waiting.some((c) => c.unread > 0) };
}

export type ListRow =
  | { kind: "chat"; chat: Chat; folder: string | null }
  | { kind: "folder"; folder: Workspace; chats: Chat[]; open: boolean };

/** The folder each chat is listed under: the first one by position when it
 *  is in several. */
export function folderOf(list: ChatList): Map<string, string> {
  const out = new Map<string, string>();
  for (const w of list.workspaces) {
    if (w.id === "main") continue;
    for (const id of list.pins[w.id] ?? []) if (!out.has(id)) out.set(id, w.id);
  }
  return out;
}

/** Chats in a folder grouped under it, as Discord groups servers. A folder
 *  sits where its first chat would; an open one lists its chats under it. */
export function groupRows(list: ChatList, chats: Chat[], open: ReadonlySet<string>): ListRow[] {
  const of = folderOf(list);
  const members = new Map<string, Chat[]>();
  for (const c of chats) {
    const f = of.get(c.id);
    if (f) members.set(f, [...(members.get(f) ?? []), c]);
  }
  const folders = new Map(list.workspaces.map((w) => [w.id, w]));
  const rows: ListRow[] = [];
  const shown = new Set<string>();
  for (const c of chats) {
    const f = of.get(c.id);
    if (!f) {
      rows.push({ kind: "chat", chat: c, folder: null });
      continue;
    }
    if (shown.has(f)) continue;
    shown.add(f);
    const inside = members.get(f)!;
    rows.push({ kind: "folder", folder: folders.get(f)!, chats: inside, open: open.has(f) });
    if (open.has(f)) for (const m of inside) rows.push({ kind: "chat", chat: m, folder: f });
  }
  return rows;
}

/** A folder without a name goes by its chats' names, as in Discord. */
export function folderName(folder: Workspace, list: ChatList | undefined): string {
  if (folder.name) return folder.name;
  const names = (list?.pins[folder.id] ?? []).map((id) => list?.chats.find((c) => c.id === id)?.name).filter(Boolean);
  return names.length ? names.slice(0, 3).join(", ") : "โฟลเดอร์ใหม่";
}

/** The chat above or below, stopping at the ends. */
export function adjacentChat(ids: string[], current: string | null, step: 1 | -1): string | null {
  if (!ids.length) return null;
  const i = current ? ids.indexOf(current) : -1;
  if (i < 0) return step === 1 ? ids[0] : ids[ids.length - 1];
  const j = i + step;
  return j < 0 || j >= ids.length ? null : ids[j];
}

/** Unread on LINE, or marked unread in LAAB (dot or Discord-style from a message). */
export const looksUnread = (c: Chat) => c.unread > 0 || !!c.markedUnread || !!c.unreadFrom;

export const unreadLabel = (n: number) => (n > 999 ? "999+" : String(n));

/** The chat with yourself is LINE's Keep Memo. */
export const isKeepMemo = (c: Pick<Chat, "id">, me: string | undefined) => !!me && c.id === me;
