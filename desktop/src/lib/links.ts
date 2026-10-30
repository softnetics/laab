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
