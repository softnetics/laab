// LINE sends these as human-readable names in parentheses. The spelling and
// casing varies between clients/locales, so normalize before looking up a
// name and accept the common aliases used by older clients.
const EMOJI: Record<string, string> = {
  excited: "😆", "big smile": "😆", happy: "😊", smile: "😊",
  nice: "👍", love: "❤️", fun: "😆", amazing: "😮", omg: "😱",
  "in love": "😍", shy: "☺️", blush: "☺️",
  sad: "😢", crying: "😭", cry: "😭", angry: "😠",
  surprised: "😮", "big eyes": "😮", sleepy: "😴", confused: "😕",
  wink: "😉", sweat: "😓", worried: "😟", frown: "☹️",
  scream: "😱", scared: "😨", dizzy: "😵", speechless: "😶",
  smirk: "😏", sneaky: "😏", hmm: "🤔", sigh: "😮‍💨",
};
export function lineEmoji(name: string): string | null {
  return EMOJI[name.trim().toLocaleLowerCase()] ?? null;
}
export function splitLineEmoji(text: string): Array<{ text: string; emoji?: string }> {
  const out: Array<{ text: string; emoji?: string }> = [];
  const re = /\(([^()\n]{1,32})\)/g;
  let at = 0;
  for (const m of text.matchAll(re)) {
    const emoji = lineEmoji(m[1]);
    if (!emoji || m.index === undefined) continue;
    if (m.index > at) out.push({ text: text.slice(at, m.index) });
    out.push({ text: m[0], emoji }); at = m.index + m[0].length;
  }
  if (at < text.length) out.push({ text: text.slice(at) });
  return out.length ? out : [{ text }];
}
