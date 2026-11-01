// The three screens on the peek hint's iPhone Duo, drawn the way LAAB draws
// them, in points of the 466 × 678 pt cover screen (each inner half is drawn
// at the same size): the chat list on the cover, and the peek view, the
// picked chat's name on the left half and its messages on the right.

export interface DuoRow {
  id: string;
  name: string;
  preview: string;
  time: string;
  unread: number;
  muted: boolean;
  avatar: string;
}

export interface DuoMessage {
  mine: boolean;
  text: string;
}

export interface DuoData {
  chatId: string;
  name: string;
  avatar: string;
  rows: DuoRow[];
  messages: DuoMessage[];
  dark: boolean;
}

export const PT_W = 466;
export const PT_H = 678;

const FONT = '-apple-system, "SF Pro Text", "Thonburi", sans-serif';
const GREEN = "#06c755";
// Avatar.tsx's colours for a chat without a picture.
const PALETTE = ["#5B8DEF", "#47B881", "#E0A23B", "#D9667B", "#8E6FD8", "#3FA7B8", "#7F8C8D"];

/** index.css's light and dark values. */
function theme(dark: boolean) {
  return dark
    ? { bg: "#2d2f2f", chat: "#2c2e2f", text: "#e6e6e6", muted: "#8e9093", field: "#474747", incoming: "#555555", glass: "#3a3c3d", chip: "#262729" }
    : { bg: "#ffffff", chat: "#ffffff", text: "#111111", muted: "#8a8c90", field: "#f0f1f3", incoming: "#edeef0", glass: "#f4f4f6", chip: "#f0f1f3" };
}

function colorFor(id: string) {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return PALETTE[Math.abs(h) % PALETTE.length];
}

// Pictures load once and redraw the screens when they arrive. The media
// protocol allows any origin, so they leave the canvas usable as a texture.
const pictures = new Map<string, HTMLImageElement | "failed">();
export function loadPictures(urls: string[], loaded: () => void) {
  for (const url of urls) {
    if (pictures.has(url)) continue;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => loaded();
    img.onerror = () => pictures.set(url, "failed");
    img.src = url;
    pictures.set(url, img);
  }
}

function avatar(g: CanvasRenderingContext2D, id: string, name: string, url: string, x: number, y: number, size: number) {
  const r = size / 2;
  const img = pictures.get(url);
  g.save();
  g.beginPath();
  g.arc(x + r, y + r, r, 0, Math.PI * 2);
  g.clip();
  if (img && img !== "failed" && img.complete && img.naturalWidth) {
    const s = Math.min(img.naturalWidth, img.naturalHeight);
    g.drawImage(img, (img.naturalWidth - s) / 2, (img.naturalHeight - s) / 2, s, s, x, y, size, size);
  } else {
    g.fillStyle = colorFor(id);
    g.fillRect(x, y, size, size);
    g.fillStyle = "#fff";
    g.font = `600 ${size * 0.4}px ${FONT}`;
    g.textAlign = "center";
    g.textBaseline = "middle";
    g.fillText((name.trim()[0] ?? "?").toUpperCase(), x + r, y + r + size * 0.03);
  }
  g.restore();
}

function text(g: CanvasRenderingContext2D, s: string, x: number, y: number, size: number, color: string, weight = 400, align: CanvasTextAlign = "left") {
  g.font = `${weight} ${size}px ${FONT}`;
  g.fillStyle = color;
  g.textAlign = align;
  g.textBaseline = "alphabetic";
  g.fillText(s, x, y);
}

function fit(g: CanvasRenderingContext2D, s: string, max: number) {
  if (g.measureText(s).width <= max) return s;
  let t = s;
  while (t.length > 1 && g.measureText(t + "…").width > max) t = t.slice(0, -1);
  return t + "…";
}

// Intl.Segmenter (Safari 14.1+) is newer than the project's TypeScript lib.
const Segmenter = (
  Intl as unknown as {
    Segmenter?: new (locale: string, o: { granularity: "word" }) => { segment(s: string): Iterable<{ segment: string }> };
  }
).Segmenter;

/** Thai has no spaces between words; break at word boundaries anyway. */
function wrap(g: CanvasRenderingContext2D, s: string, max: number, lines: number) {
  const seg = Segmenter ? new Segmenter("th", { granularity: "word" }) : null;
  const parts = seg ? [...seg.segment(s.replace(/\s+/g, " "))].map((p) => p.segment) : [...s];
  const out: string[] = [];
  let line = "";
  for (const p of parts) {
    if (g.measureText(line + p).width > max && line) {
      out.push(line.trimEnd());
      line = p.trimStart();
      if (out.length === lines) break;
    } else line += p;
  }
  if (out.length < lines && line) out.push(line);
  if (out.length === lines && parts.join("").length > out.join("").length + 1) out[lines - 1] = fit(g, out[lines - 1] + "…", max);
  return out;
}

function pill(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, color: string) {
  g.fillStyle = color;
  g.beginPath();
  g.roundRect(x, y, w, h, h / 2);
  g.fill();
}

function stroke(g: CanvasRenderingContext2D, color: string, width: number) {
  g.strokeStyle = color;
  g.lineWidth = width;
  g.lineCap = "round";
  g.lineJoin = "round";
}

function eye(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  stroke(g, color, s * 0.13);
  g.beginPath();
  g.ellipse(x, y, s / 2, s * 0.3, 0, 0, Math.PI * 2);
  g.stroke();
  g.fillStyle = color;
  g.beginPath();
  g.arc(x, y, s * 0.13, 0, Math.PI * 2);
  g.fill();
}

function person(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string, plus = false) {
  stroke(g, color, s * 0.08);
  g.beginPath();
  g.arc(x, y - s * 0.18, s * 0.2, 0, Math.PI * 2);
  g.stroke();
  g.beginPath();
  g.moveTo(x - s * 0.36, y + s * 0.38);
  g.quadraticCurveTo(x - s * 0.36, y + s * 0.06, x, y + s * 0.06);
  g.quadraticCurveTo(x + s * 0.36, y + s * 0.06, x + s * 0.36, y + s * 0.38);
  g.closePath();
  g.stroke();
  if (plus) {
    g.fillStyle = color;
    g.beginPath();
    g.arc(x + s * 0.34, y + s * 0.3, s * 0.18, 0, Math.PI * 2);
    g.fill();
  }
}

function bubbleIcon(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  g.fillStyle = color;
  g.beginPath();
  g.roundRect(x - s * 0.42, y - s * 0.32, s * 0.84, s * 0.56, s * 0.12);
  g.moveTo(x - s * 0.22, y + s * 0.2);
  g.lineTo(x - s * 0.3, y + s * 0.42);
  g.lineTo(x, y + s * 0.2);
  g.fill();
}

function gear(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  stroke(g, color, s * 0.08);
  g.beginPath();
  g.arc(x, y, s * 0.24, 0, Math.PI * 2);
  g.stroke();
  for (let i = 0; i < 8; i++) {
    const a = (i * Math.PI) / 4;
    g.beginPath();
    g.moveTo(x + Math.cos(a) * s * 0.3, y + Math.sin(a) * s * 0.3);
    g.lineTo(x + Math.cos(a) * s * 0.4, y + Math.sin(a) * s * 0.4);
    g.stroke();
  }
  g.beginPath();
  g.arc(x, y, s * 0.09, 0, Math.PI * 2);
  g.stroke();
}

function magnifier(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  stroke(g, color, s * 0.1);
  g.beginPath();
  g.arc(x - s * 0.08, y - s * 0.08, s * 0.26, 0, Math.PI * 2);
  g.stroke();
  g.beginPath();
  g.moveTo(x + s * 0.12, y + s * 0.12);
  g.lineTo(x + s * 0.34, y + s * 0.34);
  g.stroke();
}

function wifi(g: CanvasRenderingContext2D, x: number, y: number, s: number, color: string) {
  stroke(g, color, s * 0.11);
  for (const r of [0.45, 0.3, 0.15]) {
    g.beginPath();
    g.arc(x, y + s * 0.2, s * r, Math.PI * 1.25, Math.PI * 1.75);
    g.stroke();
  }
  g.fillStyle = color;
  g.beginPath();
  g.arc(x, y + s * 0.2, s * 0.06, 0, Math.PI * 2);
  g.fill();
}

/** Points to pixels, stretched to the canvas. */
function begin(c: HTMLCanvasElement, fill: string) {
  const g = c.getContext("2d")!;
  g.setTransform(c.width / PT_W, 0, 0, c.height / PT_H, 0, 0);
  g.fillStyle = fill;
  g.fillRect(0, 0, PT_W, PT_H);
  return g;
}

/** The cover screen: LAAB's chat list with the picked chat marked, and the
 *  vertical bar at the right edge. */
export function drawCover(c: HTMLCanvasElement, d: DuoData) {
  const t = theme(d.dark);
  const g = begin(c, t.bg);
  const right = 400;
  text(g, "แชท", 16, 44, 26, t.text, 700);
  let x = 16;
  ["ทั้งหมด", "เพื่อน", "กลุ่ม", "บัญชีทางการ"].forEach((label, i) => {
    text(g, label, x, 82, 15, i === 0 ? t.text : t.muted, i === 0 ? 700 : 400);
    const w = g.measureText(label).width;
    if (i === 0) pill(g, x, 88, w, 2.5, t.text);
    x += w + 20;
  });
  g.fillStyle = t.field;
  g.beginPath();
  g.roundRect(16, 102, right - 60, 36, 10);
  g.fill();
  magnifier(g, 34, 120, 14, t.muted);
  text(g, "ค้นหาข้อความและห้องแชท", 48, 125, 13.5, t.muted);
  let y = 150;
  for (const r of d.rows) {
    if (y + 72 > PT_H) break;
    const picked = r.id === d.chatId;
    if (picked) {
      g.fillStyle = "rgba(6,199,85,0.12)";
      g.fillRect(0, y, right, 72);
    }
    avatar(g, r.id, r.name, r.avatar, 16, y + 10, 52);
    let nx = 80;
    if (picked) {
      eye(g, nx + 9, y + 25, 17, GREEN);
      nx += 23;
    }
    g.font = `500 16px ${FONT}`;
    text(g, fit(g, r.name, right - nx - 56), nx, y + 31, 16, t.text, 500);
    g.font = `400 13.5px ${FONT}`;
    text(g, fit(g, r.preview, right - 80 - 40), 80, y + 53, 13.5, t.muted);
    text(g, r.time, right - 12, y + 29, 11, t.muted, 400, "right");
    if (r.unread > 0) {
      const label = r.unread > 999 ? "999+" : String(r.unread);
      g.font = `700 11px ${FONT}`;
      const w = Math.max(20, g.measureText(label).width + 12);
      pill(g, right - 12 - w, y + 40, w, 20, r.muted ? t.muted : GREEN);
      text(g, label, right - 12 - w / 2, y + 54, 11, "#fff", 700, "center");
    }
    y += 72;
  }
  // The vertical bar: camera, clock, Wi‑Fi, search, and the tab capsule.
  const bx = 433;
  g.fillStyle = "#000";
  g.beginPath();
  g.arc(bx, 46, 18, 0, Math.PI * 2);
  g.fill();
  text(g, "9:41", bx, 95, 17, t.text, 600, "center");
  wifi(g, bx, 118, 26, t.text);
  g.fillStyle = t.glass;
  g.beginPath();
  g.arc(bx, 202, 24, 0, Math.PI * 2);
  g.fill();
  magnifier(g, bx, 202, 24, t.text);
  g.fillStyle = t.glass;
  g.beginPath();
  g.roundRect(bx - 23, 444, 46, 190, 23);
  g.fill();
  g.fillStyle = d.dark ? "#4a4c4d" : "#e4e4e8";
  g.beginPath();
  g.arc(bx, 513, 21, 0, Math.PI * 2);
  g.fill();
  person(g, bx, 469, 24, t.text);
  bubbleIcon(g, bx, 513, 24, t.text);
  person(g, bx, 557, 24, t.text, true);
  gear(g, bx, 601, 24, t.text);
  const unread = d.rows.reduce((n, r) => n + r.unread, 0);
  if (unread > 0) {
    const label = unread > 99 ? "99+" : String(unread);
    g.font = `700 10px ${FONT}`;
    const w = Math.max(16, g.measureText(label).width + 8);
    pill(g, bx + 4, 495, w, 16, "#ff334b");
    text(g, label, bx + 4 + w / 2, 507, 10, "#fff", 700, "center");
  }
}

/** The inner screen's left half: the peek view's panel. */
export function drawInnerLeft(c: HTMLCanvasElement, d: DuoData) {
  const t = theme(d.dark);
  const g = begin(c, t.chat);
  const cx = PT_W / 2;
  const cy = PT_H / 2 - 30;
  avatar(g, d.chatId, d.name, d.avatar, cx - 48, cy - 110, 96);
  g.font = `600 27px ${FONT}`;
  text(g, fit(g, d.name, PT_W - 60), cx, cy + 30, 27, t.text, 600, "center");
  g.font = `400 18px ${FONT}`;
  const line = "ยังไม่ขึ้นว่าอ่านแล้ว";
  const w = g.measureText(line).width + 26;
  eye(g, cx - w / 2 + 9, cy + 66, 18, t.muted);
  text(g, line, cx - w / 2 + 26, cy + 72, 18, t.muted);
  text(g, "กางจนสุดเมื่อไหร่ จะขึ้นว่าอ่านแล้ว", cx, cy + 102, 16, t.muted, 400, "center");
}

/** The inner screen's right half: the chat's latest messages, as the chat
 *  shows them, pinned to the newest. */
export function drawInnerRight(c: HTMLCanvasElement, d: DuoData) {
  const t = theme(d.dark);
  const g = begin(c, t.chat);
  const max = PT_W * 0.66;
  g.font = `400 16px ${FONT}`;
  const laid = d.messages.map((m) => {
    const lines = wrap(g, m.text, max - 28, 3);
    const w = Math.min(max, Math.max(...lines.map((l) => g.measureText(l).width)) + 28);
    return { m, lines, w, h: lines.length * 21 + 18 };
  });
  const gap = 10;
  const top = 64;
  let total = laid.reduce((n, b) => n + b.h + gap, 0);
  while (laid.length > 1 && top + total > PT_H - 20) total -= laid.shift()!.h + gap;
  // The date chip, as Timeline shows it over the day's first message.
  g.font = `400 13px ${FONT}`;
  const chipW = g.measureText("วันนี้").width + 26;
  pill(g, PT_W / 2 - chipW / 2, 18, chipW, 28, t.chip);
  text(g, "วันนี้", PT_W / 2, 37, 13, t.muted, 400, "center");
  let y = top;
  for (const b of laid) {
    const x = b.m.mine ? PT_W - 16 - b.w : 60;
    if (!b.m.mine) avatar(g, d.chatId, d.name, d.avatar, 14, y, 36);
    g.fillStyle = b.m.mine ? "#67dd71" : t.incoming;
    g.beginPath();
    g.roundRect(x, y, b.w, b.h, 18);
    g.fill();
    b.lines.forEach((l, i) => text(g, l, x + 14, y + 26 + i * 21, 16, b.m.mine ? "#111a11" : t.text));
    y += b.h + gap;
  }
}
