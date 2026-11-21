import { invoke } from "@tauri-apps/api/core";

/** Elements with this attribute are left out of a picture (hover controls). */
export const SKIP = "data-capture-skip";

const SVG_NS = "http://www.w3.org/2000/svg";
// Inside an SVG, only these carry over; copying every property onto a path
// can move or hide it.
const SVG_PROPS = ["fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "opacity", "color", "transform", "visibility", "display"];

function copyStyle(from: Element, to: Element, root: boolean) {
  const cs = getComputedStyle(from);
  let css = "";
  if (from.namespaceURI === SVG_NS && from.tagName.toLowerCase() !== "svg") {
    for (const p of SVG_PROPS) css += `${p}:${cs.getPropertyValue(p)};`;
  } else {
    for (let i = 0; i < cs.length; i++) {
      const p = cs[i];
      css += `${p}:${cs.getPropertyValue(p)};`;
    }
  }
  // The picture is one frame: an entrance animation would leave it at its
  // first, often transparent, frame.
  css += "animation:none;transition:none;";
  if (root) css += "margin:0;";
  to.setAttribute("style", css);
}

const asDataUrl = (blob: Blob) =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as string);
    r.onerror = () => reject(r.error);
    r.readAsDataURL(blob);
  });

/** An image as a data URL: an SVG drawn as an image can't load anything. */
async function inlineImage(src: string): Promise<string | null> {
  if (!src || src.startsWith("data:")) return src || null;
  try {
    if (src.startsWith("https:")) {
      const bytes = await invoke<ArrayBuffer>("capture_image", { url: src });
      return await asDataUrl(new Blob([bytes]));
    }
    const res = await fetch(src);
    if (!res.ok) return null;
    return await asDataUrl(await res.blob());
  } catch {
    return null;
  }
}

async function cloneTree(from: Element, root = false): Promise<Node | null> {
  if (from.hasAttribute(SKIP)) return null;
  const to = from.cloneNode(false) as Element;
  copyStyle(from, to, root);
  for (const a of ["class", "id", "data-message", "aria-label", "title"]) to.removeAttribute(a);
  if (from instanceof HTMLImageElement) {
    const data = await inlineImage(from.currentSrc || from.src);
    to.removeAttribute("srcset");
    if (data) to.setAttribute("src", data);
    else to.removeAttribute("src");
  } else if (from instanceof HTMLVideoElement || from instanceof HTMLAudioElement || from instanceof HTMLCanvasElement) {
    // Keep the box, not the player.
    const box = document.createElement("div");
    box.setAttribute("style", to.getAttribute("style") ?? "");
    return box;
  }
  const kids = await Promise.all(
    [...from.childNodes].map((n) =>
      n.nodeType === Node.ELEMENT_NODE ? cloneTree(n as Element) : Promise.resolve(n.nodeType === Node.TEXT_NODE ? n.cloneNode() : null),
    ),
  );
  for (const k of kids) if (k) to.appendChild(k);
  return to;
}

const loadImage = (url: string) =>
  new Promise<HTMLImageElement>((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("วาดรูปข้อความไม่สำเร็จ"));
    img.src = url;
  });

const nextFrame = () => new Promise((r) => setTimeout(r, 30));

/** A PNG of `node` as it is drawn now, at `scale` times its size. */
export async function snapshot(node: HTMLElement, scale = 2): Promise<Uint8Array> {
  const rect = node.getBoundingClientRect();
  const width = Math.ceil(rect.width);
  const height = Math.ceil(rect.height);
  const clone = (await cloneTree(node, true)) as Element;
  const html = new XMLSerializer().serializeToString(clone);
  const svg =
    `<svg xmlns="${SVG_NS}" width="${width}" height="${height}">` +
    `<foreignObject x="0" y="0" width="100%" height="100%">${html}</foreignObject></svg>`;
  const img = await loadImage(`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`);
  const canvas = document.createElement("canvas");
  canvas.width = width * scale;
  canvas.height = height * scale;
  const ctx = canvas.getContext("2d")!;
  ctx.scale(scale, scale);
  // WebKit can paint an SVG's embedded photos only from the second draw.
  ctx.drawImage(img, 0, 0, width, height);
  await nextFrame();
  ctx.clearRect(0, 0, width, height);
  ctx.drawImage(img, 0, 0, width, height);
  const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/png"));
  if (!blob) throw new Error("วาดรูปข้อความไม่สำเร็จ");
  return new Uint8Array(await blob.arrayBuffer());
}

/** Resolves once every image under `node` has loaded or failed, or after `ms`. */
export function imagesLoaded(node: HTMLElement, ms = 8000): Promise<void> {
  const pending = [...node.querySelectorAll("img")].filter((i) => !i.complete);
  const all = Promise.all(
    pending.map((i) => new Promise<void>((r) => { i.addEventListener("load", () => r(), { once: true }); i.addEventListener("error", () => r(), { once: true }); })),
  ).then(() => undefined);
  return Promise.race([all, new Promise<void>((r) => setTimeout(r, ms))]);
}
