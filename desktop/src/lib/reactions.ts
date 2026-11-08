import amazingArt from "../assets/reactions/AMAZING.png";
import funArt from "../assets/reactions/FUN.png";
import loveArt from "../assets/reactions/LOVE.png";
import niceArt from "../assets/reactions/NICE.png";
import omgArt from "../assets/reactions/OMG.png";
import sadArt from "../assets/reactions/SAD.png";
import basicEmoji from "./basicEmoji.json";
import type { Reaction, ReactionName } from "./types";

export type PredefinedReaction = "NICE" | "LOVE" | "FUN" | "AMAZING" | "SAD" | "OMG";
export const PREDEFINED: PredefinedReaction[] = ["NICE", "LOVE", "FUN", "AMAZING", "SAD", "OMG"];

/** LINE's Moon reaction art (same faces the official client shows). */
export const REACTIONS: { name: PredefinedReaction; emoji: string; art: string }[] = [
  { name: "NICE", emoji: "👍", art: niceArt },
  { name: "LOVE", emoji: "❤️", art: loveArt },
  { name: "FUN", emoji: "😆", art: funArt },
  { name: "AMAZING", emoji: "😮", art: amazingArt },
  { name: "SAD", emoji: "😢", art: sadArt },
  { name: "OMG", emoji: "😱", art: omgArt },
];

/** Bundled LINE art for a predefined reaction, or null. */
export const predefinedArt = (r: ReactionName) => REACTIONS.find((x) => x.name === r)?.art ?? null;

export const REACTION_PACK_ID = basicEmoji.productId;
export const REACTION_PACK_COUNT = basicEmoji.count;
export const reactionPackIds = Array.from({ length: REACTION_PACK_COUNT }, (_, i) =>
  String(i + 1).padStart(3, "0"),
);

export const packTabUrl = () =>
  `https://stickershop.line-scdn.net/sticonshop/v1/sticon/${REACTION_PACK_ID}/android/tab_on.png`;

export const emojiFor = (r: ReactionName) => REACTIONS.find((x) => x.name === r)?.emoji ?? "✨";

/** `PAID:<productId>:<emojiId>:<version>:<1|0 animated>` */
export function paidReaction(
  productId: string,
  emojiId: string,
  version = 1,
  animated = false,
): ReactionName {
  return `PAID:${productId}:${emojiId}:${version}:${animated ? "1" : "0"}`;
}

export const packReaction = (emojiId: string) => paidReaction(REACTION_PACK_ID, emojiId);

/** The LINE emoji of a paid reaction, or null. */
export function paidEmoji(
  r: ReactionName,
): { productId: string; emojiId: string; version: number; animated: boolean } | null {
  const [kind, productId, emojiId, version, animated] = r.split(":");
  if (kind !== "PAID" || !productId || !emojiId) return null;
  return { productId, emojiId, version: Number(version) || 1, animated: animated === "1" };
}

export function isPredefined(reaction: string): reaction is PredefinedReaction {
  return PREDEFINED.includes(reaction as PredefinedReaction);
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
export const nextReaction = (current: ReactionName | undefined, picked: ReactionName) =>
  (current === picked ? null : picked);

const RECENT_KEY = "laab.reactionRecent";
const RECENT_MAX = 24;

export function readRecentReactions(): ReactionName[] {
  try {
    const raw = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    if (!Array.isArray(raw)) return [];
    return raw
      .filter((x): x is string => typeof x === "string")
      .filter((x) => isPredefined(x) || !!paidEmoji(x as ReactionName))
      .slice(0, RECENT_MAX) as ReactionName[];
  } catch {
    return [];
  }
}

export function rememberReaction(reaction: ReactionName): ReactionName[] {
  const next = [reaction, ...readRecentReactions().filter((x) => x !== reaction)].slice(0, RECENT_MAX);
  localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  return next;
}
