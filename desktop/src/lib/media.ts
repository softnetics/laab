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
