import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { api, useSession } from "../lib/api";
import { openUrl } from "../lib/open";
import type { RichMenu as Menu } from "../lib/types";
import { notice, pushDismiss } from "../lib/ui";

// An official account's rich menu, opened on demand from a "เมนู" chip in the
// composer toolbar as a popover above it. Picked over a menu always shown
// under the composer (LINE mobile style, with or without a blurred fill for
// the side gutters) and an inset card: those cost a third of the window's
// height in every OA chat, and the menu is used now and then.
// Enter 200ms scale 0.96 + 6px rise from the bottom right; exit 140ms ease-in.

const ease = [0.32, 0.72, 0, 1] as const;

/** Renders right under the composer, in a zero-height anchor. */
export function RichMenu({ chatId }: { chatId: string }) {
  const state = useSession()?.state;
  const { data: menu } = useQuery({
    queryKey: ["richMenu", chatId],
    queryFn: () => api.richMenu(chatId),
    staleTime: 10 * 60_000,
    enabled: state === "signed_in" || state === "demo",
  });
  const [open, setOpen] = useState(false);
  const reduce = useReducedMotion();
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const undo = pushDismiss(() => setOpen(false));
    const down = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", down);
    return () => {
      undo();
      document.removeEventListener("pointerdown", down);
    };
  }, [open]);
  if (!menu) return null;
  return (
    <div ref={root} className="relative z-20 h-0">
      {/* In the toolbar row, left of the sticker button. */}
      <button
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className={`absolute right-[52px] bottom-[6px] flex h-8 items-center gap-1.5 rounded-full pr-2.5 pl-3 text-[12.5px] transition-[color,background-color,scale] duration-150 ease-out active:scale-[0.96] ${
          open ? "bg-line-green/15 text-line-green" : "text-secondary hover:bg-field hover:text-primary"
        }`}
      >
        <GridIcon />
        {menu.label}
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={reduce ? false : { opacity: 0, scale: 0.96, y: 6 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={reduce ? { opacity: 0, transition: { duration: 0 } } : { opacity: 0, scale: 0.98, y: 4, transition: { duration: 0.14, ease: "easeIn" } }}
            transition={{ duration: 0.2, ease }}
            // No taller than 40% of the window.
            style={{ transformOrigin: "bottom right", width: `min(calc(100% - 24px), calc(40vh * ${menu.width / menu.height}))` }}
            className="absolute right-3 bottom-[46px] overflow-hidden rounded-[14px] bg-popover p-1.5 shadow-[0_0_0_1px_rgba(255,255,255,0.08),0_12px_32px_rgba(0,0,0,0.35),0_2px_8px_rgba(0,0,0,0.2)]"
          >
            <div className="overflow-hidden rounded-[8px]">
              <Canvas menu={menu} chatId={chatId} onTap={() => setOpen(false)} />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

const pct = (v: number, of: number) => `${(v / of) * 100}%`;

function Canvas({ menu, chatId, onTap }: { menu: Menu; chatId: string; onTap: () => void }) {
  return (
    <div className="relative isolate w-full" style={{ aspectRatio: `${menu.width} / ${menu.height}` }}>
      {menu.images.map((img, i) => (
        <img
          key={i}
          src={img.url}
          alt=""
          draggable={false}
          className="absolute select-none"
          style={{ left: pct(img.x, menu.width), top: pct(img.y, menu.height), width: pct(img.w, menu.width), height: pct(img.h, menu.height) }}
        />
      ))}
      {menu.areas.map((a, i) => (
        <button
          key={i}
          aria-label={a.action.label ?? `ปุ่มเมนู ${i + 1}`}
          onClick={() => {
            if (a.action.uri) openUrl(a.action.uri, chatId);
            else notice(`LAAB ยังไม่รองรับปุ่มเมนูแบบ ${a.action.type || "?"}`);
            onTap();
          }}
          className="absolute cursor-pointer touch-manipulation transition-colors duration-150 ease-out hover:bg-white/[0.07] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-white/70 active:bg-black/15"
          style={{ left: pct(a.x, menu.width), top: pct(a.y, menu.height), width: pct(a.w, menu.width), height: pct(a.h, menu.height) }}
        />
      ))}
    </div>
  );
}

const GridIcon = () => (
  <svg width="13" height="13" viewBox="0 0 13 13" fill="currentColor" aria-hidden>
    <rect x="0" y="0" width="5.5" height="5.5" rx="1.4" />
    <rect x="7.5" y="0" width="5.5" height="5.5" rx="1.4" />
    <rect x="0" y="7.5" width="5.5" height="5.5" rx="1.4" />
    <rect x="7.5" y="7.5" width="5.5" height="5.5" rx="1.4" />
  </svg>
);
