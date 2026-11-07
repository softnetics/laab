import type { Reaction, ReactionName } from "./types";

// System emoji stand in for LINE's reaction art.
export const REACTIONS: { name: Exclude<ReactionName, "PAID" | `PAID:${string}`>; emoji: string }[] = [
  { name: "NICE", emoji: "👍" },
  { name: "LOVE", emoji: "❤️" },
  { name: "FUN", emoji: "😆" },
  { name: "AMAZING", emoji: "😮" },
  { name: "SAD", emoji: "😢" },
  { name: "OMG", emoji: "😱" },
];

export const emojiFor = (r: ReactionName) => REACTIONS.find((x) => x.name === r)?.emoji ?? "✨";

/** The LINE emoji of a paid reaction, or null. */
export function paidEmoji(r: ReactionName): { productId: string; emojiId: string; version: number; animated: boolean } | null {
  const [kind, productId, emojiId, version, animated] = r.split(":");
  if (kind !== "PAID" || !productId || !emojiId) return null;
  return { productId, emojiId, version: Number(version) || 1, animated: animated === "1" };
}

/** Most-used types first, up to three, for the badge. */
export function topTypes(list: Reaction[]): ReactionName[] {
  const count = new Map<ReactionName, number>();
  for (const r of list) count.set(r.reaction, (count.get(r.reaction) ?? 0) + 1);
  return [...count.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([k]) => k);
}

/** Picking the same reaction again removes it. */
export const nextReaction = (current: ReactionName | undefined, picked: ReactionName) => (current === picked ? null : picked);
