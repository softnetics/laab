import { useState } from "react";
import { mediaUrl } from "../lib/api";
import { GroupIcon } from "./Icons";

const PALETTE = ["#5B8DEF", "#47B881", "#E0A23B", "#D9667B", "#8E6FD8", "#3FA7B8", "#7F8C8D"];

function colorFor(id: string) {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return PALETTE[Math.abs(h) % PALETTE.length];
}

export function Avatar({
  id,
  name,
  size,
  picture,
  local,
  all,
  className = "",
}: {
  id: string;
  name: string;
  size: number;
  picture?: string | null;
  local?: string | null;
  all?: boolean;
  className?: string;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const src = mediaUrl.avatar(id, picture, local);
  const style = { width: size, height: size, minWidth: size };
  if (all) {
    return (
      <div className={`rounded-full bg-line-green text-white grid place-items-center ${className}`} style={style}>
        <GroupIcon size={size * 0.55} />
      </div>
    );
  }
  if (failed === src) {
    return (
      <div
        className={`rounded-full grid place-items-center text-white font-semibold ${className}`}
        style={{ ...style, background: colorFor(id), fontSize: size * 0.4 }}
      >
        {(name.trim()[0] ?? "?").toUpperCase()}
      </div>
    );
  }
  return (
    <img
      src={src}
      alt=""
      draggable={false}
      onError={() => setFailed(src)}
      className={`rounded-full object-cover bg-field ${className}`}
      style={style}
    />
  );
}
