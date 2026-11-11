// URL detection for text bubbles, and LIFF link recognition.

const URL_RE = /\b(?:https?:\/\/|www\.)[^\s<>"'`]+|\bline:\/\/[^\s<>"'`]+/gi;

export type Segment = { text: string; url?: string };

export function linkify(text: string): Segment[] {
  const out: Segment[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_RE)) {
    let url = m[0].replace(/[.,!?;:)\]}>]+$/, "");
    const at = m.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at) });
    out.push({ text: url, url: url.startsWith("www.") ? `https://${url}` : url });
    last = at + url.length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

/** The first web link in a message's text, the one LINE previews. */
export function previewUrl(text: string): string | null {
  return linkify(text).find((s) => s.url && /^https?:\/\//i.test(s.url))?.url ?? null;
}

const LIFF_ID = /^\d{6,}-[A-Za-z0-9]{6,}$/;

export function liffId(url: string): string | null {
  try {
    const u = new URL(url);
    const parts = u.pathname.split("/").filter(Boolean);
    const host = u.hostname.toLowerCase();
    let id: string | undefined;
    if ((u.protocol === "https:" || u.protocol === "http:") && (host === "liff.line.me" || host === "miniapp.line.me")) id = parts[0];
    else if ((u.protocol === "https:" || u.protocol === "http:") && host === "line.me" && parts[0] === "R" && parts[1] === "app")
      id = parts[2];
    else if (u.protocol === "line:" && (host === "app" || u.pathname.startsWith("//app/"))) id = host === "app" ? parts[0] : parts[1];
    return id && LIFF_ID.test(id) ? id : null;
  } catch {
    return null;
  }
}

/** Where a group post card ("โพสต์ในกลุ่ม") leads: a note, or an album
 *  (line://group/home/albums/album?albumId=…). An album link may name the
 *  album by `albumId`, `albumIdV2` or both, so `ids` holds every id it gives. */
export function postTarget(url: string): { kind: "note" | "album"; ids: string[] } | null {
  try {
    const u = new URL(url.replace(/^line:\/\//i, "https://x/"));
    if (/\/albums?(\/|$)/.test(u.pathname)) {
      const ids = ["albumId", "albumIdV2"].map((k) => u.searchParams.get(k)).filter((v): v is string => !!v && v !== "0");
      return { kind: "album", ids };
    }
    const postId = u.searchParams.get("postId");
    return postId ? { kind: "note", ids: [postId] } : null;
  } catch {
    return null;
  }
}

/** What an album card in the chat shows, from its message metadata: the
 *  header, the first photo and how many photos the post added. Null when
 *  the card is not about an album. */
export function albumCard(meta: Record<string, string>): {
  title: string;
  albumId: string | null;
  oid: string | null;
  video: boolean;
  count: number;
  aspect: number | null;
} | null {
  const target = meta.postEndUrl ? postTarget(meta.postEndUrl) : null;
  if (target?.kind !== "album" && meta.albumName === undefined) return null;
  const name = meta.albumName ?? "";
  const quoted = name ? ` "${name}"` : "";
  // BA is a new album; other album cards add photos to one.
  const title = meta.locKey === "BA" ? `สร้างอัลบั้ม${quoted}` : `เพิ่มรูปในอัลบั้ม${quoted}`;
  // mediaCount counts the photos beyond the one the card shows.
  const extra = Number(meta.mediaCount);
  const oid = meta.mediaOid || null;
  const w = Number(meta.width);
  const h = Number(meta.height);
  return {
    title,
    albumId: target?.ids[0] ?? (meta.cafeId && meta.cafeId !== "0" ? meta.cafeId : null),
    oid,
    video: meta.mediaType === "V",
    count: oid ? 1 + (Number.isFinite(extra) && extra > 0 ? extra : 0) : 0,
    aspect: w > 0 && h > 0 ? w / h : null,
  };
}
