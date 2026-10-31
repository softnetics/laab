import { Image } from "@tauri-apps/api/image";
import { readImage, writeImage } from "@tauri-apps/plugin-clipboard-manager";

const canvasFor = (width: number, height: number) => {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
};

const toPng = (canvas: HTMLCanvasElement) =>
  new Promise<Uint8Array>((resolve, reject) =>
    canvas.toBlob((blob) => (blob ? blob.arrayBuffer().then((b) => resolve(new Uint8Array(b))) : reject(new Error("แปลงรูปไม่สำเร็จ"))), "image/png"),
  );

export async function readClipboardPng(): Promise<Uint8Array | null> {
  let image: Image;
  try { image = await readImage(); } catch { return null; }
  const { width, height } = await image.size();
  if (!width || !height) return null;
  const rgba = await image.rgba();
  const canvas = canvasFor(width, height);
  canvas.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  return toPng(canvas);
}

export async function copyImage(bytes: ArrayBuffer) {
  const bitmap = await createImageBitmap(new Blob([bytes]));
  const canvas = canvasFor(bitmap.width, bitmap.height);
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(bitmap, 0, 0);
  const { data } = ctx.getImageData(0, 0, bitmap.width, bitmap.height);
  await writeImage(await Image.new(new Uint8Array(data.buffer), bitmap.width, bitmap.height));
}
