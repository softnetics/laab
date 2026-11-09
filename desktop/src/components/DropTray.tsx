import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { mediaUrl } from "../lib/api";
import type { DroppedFile } from "../lib/types";
import { CloseIcon, DocIcon, PaperclipIcon, PlayIcon } from "./Icons";

const EASE = [0.32, 0.72, 0, 1] as const;
const TINT = "bg-[color-mix(in_oklch,var(--color-line-green)_10%,var(--color-chat))]";
const GREEN_TEXT = "text-[#05a648] dark:text-line-green";

export function formatSize(bytes: number) {
  if (bytes < 1000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} KB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(bytes < 10_000_000 ? 1 : 0)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
}

const totalSize = (files: DroppedFile[]) => formatSize(files.reduce((n, f) => n + f.size, 0));

/** A long name keeps its start and its extension: "สรุปรายงาน…ครั้งที่ 4).pdf". */
function MiddleName({ name, className = "" }: { name: string; className?: string }) {
  const dot = name.lastIndexOf(".");
  const tail = dot > 0 ? name.slice(Math.max(dot - 6, 0)) : name.slice(-6);
  const head = name.slice(0, name.length - tail.length);
  return (
    <span className={`flex min-w-0 ${className}`} title={name}>
      <span className="truncate">{head}</span>
      <span className="shrink-0 whitespace-pre">{tail}</span>
    </span>
  );
}

/** The picture itself, or a tile naming the file's type. */
function FileThumb({ file, size, radius }: { file: DroppedFile; size: number; radius: number }) {
  const [broken, setBroken] = useState(false);
  if (file.kind === "image" && !broken)
    return (
      <img
        src={mediaUrl.dropped(file.path)}
        alt=""
        draggable={false}
        onError={() => setBroken(true)}
        className="shrink-0 bg-field object-cover -outline-offset-1 outline-1 outline-black/10 dark:outline-white/10"
        style={{ width: size, height: size, borderRadius: radius }}
      />
    );
  const Icon = file.kind === "video" ? PlayIcon : DocIcon;
  const tint = file.kind === "video" ? "bg-[#6c5ce7]/12 text-[#6c5ce7]" : "bg-line-green/12 text-[#05a648]";
  const dot = file.name.lastIndexOf(".");
  return (
    <div className={`relative grid shrink-0 place-items-center ${tint}`} style={{ width: size, height: size, borderRadius: radius }}>
      <Icon size={Math.round(size * 0.42)} />
      {size >= 56 && dot > 0 && (
        <span className="absolute bottom-1.5 text-[9px] font-semibold tracking-wide">{file.name.slice(dot + 1).toUpperCase()}</span>
      )}
    </div>
  );
}

/** While files are dragged over a chat: a panel joined to the top of the
 *  message box, showing what will be attached. `files` is null until their
 *  details arrive. Sits in a `relative` wrapper around the composer. */
export function DropDock({ count, files }: { count: number | null; files: DroppedFile[] | null }) {
  const reduce = useReducedMotion();
  // Folders are left out once the files are known.
  const n = files ? files.length : (count ?? 0);
  return (
    <AnimatePresence>
      {count !== null && (
        <motion.div
          key="dock"
          initial={reduce ? { opacity: 0 } : { opacity: 0, y: 14 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduce ? 0 : 8, transition: { duration: 0.12, ease: "easeIn" } }}
          transition={{ duration: 0.22, ease: EASE }}
          className={`pointer-events-none absolute right-2 bottom-[calc(100%-2px)] left-2 z-30 flex h-[60px] items-center gap-3 rounded-t-2xl px-4 ${TINT} ring-2 ring-line-green ring-inset`}
        >
          {files && files.length > 0 ? (
            // More than five: the row fades out to the right; the count is in the label.
            <span className={`flex gap-1.5 ${files.length > 5 ? "[mask-image:linear-gradient(to_right,black_70%,transparent)]" : ""}`}>
              {files.slice(0, 6).map((f) => (
                <FileThumb key={f.path} file={f} size={36} radius={8} />
              ))}
            </span>
          ) : (
            <PaperclipIcon size={20} className={GREEN_TEXT} />
          )}
          <span className="flex-1" />
          <span className={`text-[13px] font-semibold ${GREEN_TEXT}`}>
            {n > 0 ? `ปล่อยเพื่อแนบ ${n} ไฟล์` : "ปล่อยเพื่อแนบไฟล์"}
            {files && files.length > 0 && <span className="font-normal opacity-70 tabular-nums"> · {totalSize(files)}</span>}
          </span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** Dropped files waiting above the message box until they are sent or let
 *  go, so a file dropped on the wrong chat is not sent. Return sends; the
 *  chat's Escape handling lets them go. */
export function DropTray({
  files,
  onSend,
  onCancel,
  onRemove,
}: {
  files: DroppedFile[];
  onSend: () => void;
  onCancel: () => void;
  onRemove: (path: string) => void;
}) {
  const reduce = useReducedMotion();
  const send = useRef<HTMLButtonElement>(null);
  const open = files.length > 0;
  useEffect(() => {
    if (open) send.current?.focus();
  }, [open]);
  return (
    <AnimatePresence>
      {open && (
        <motion.section
          key="tray"
          aria-label="ไฟล์ที่จะส่ง"
          initial={reduce ? false : { opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduce ? 0 : 6, transition: { duration: 0.14, ease: "easeIn" } }}
          transition={{ duration: 0.22, ease: EASE }}
          className="absolute right-3 bottom-[calc(100%+8px)] left-3 z-30 flex flex-col gap-2 rounded-2xl bg-popover p-2.5 shadow-[0_0_0_1px_rgb(0_0_0/0.06),0_2px_6px_rgb(0_0_0/0.06),0_12px_28px_-8px_rgb(0_0_0/0.18)] dark:shadow-[0_0_0_1px_rgb(255_255_255/0.09)]"
        >
          <div className="flex h-7 items-center gap-2 px-1">
            <span className="text-[12.5px] font-semibold text-primary">
              {files.length} ไฟล์ <span className="font-normal text-secondary tabular-nums">· {totalSize(files)}</span>
            </span>
            <span className="flex-1" />
            <button type="button" onClick={onCancel} className="h-7 rounded-md px-2.5 text-[12.5px] text-secondary hover:bg-row-hover hover:text-primary">
              ยกเลิก
            </button>
            <button
              ref={send}
              type="button"
              onClick={onSend}
              className="h-7 rounded-md bg-line-green px-3.5 text-[12.5px] font-semibold text-white outline-offset-2 outline-secondary transition-[scale] duration-150 ease-out active:scale-[0.96]"
            >
              ส่ง {files.length} ไฟล์
            </button>
          </div>
          <ul className="flex gap-2 overflow-x-auto pb-0.5">
            {files.map((f) => (
              <li key={f.path} className="relative flex w-[104px] shrink-0 flex-col gap-1.5 rounded-[10px] bg-field/60 p-2">
                <FileThumb file={f} size={88} radius={6} />
                <MiddleName name={f.name} className="text-[11.5px] leading-tight text-primary" />
                <span className="text-[10.5px] text-secondary tabular-nums">{formatSize(f.size)}</span>
                <button
                  type="button"
                  aria-label={`เอา ${f.name} ออก`}
                  onClick={() => onRemove(f.path)}
                  className="absolute top-1 right-1 grid h-6 w-6 place-items-center rounded-full bg-black/55 text-white transition-colors duration-150 before:absolute before:-inset-2 hover:bg-black/75"
                >
                  <CloseIcon size={12} />
                </button>
              </li>
            ))}
          </ul>
        </motion.section>
      )}
    </AnimatePresence>
  );
}
