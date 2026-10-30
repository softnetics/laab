import { Menu } from "@base-ui/react/menu";
import { useHotkey } from "@tanstack/react-hotkeys";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { api, errorText, q, useChatList, usePeople, useSession } from "../lib/api";
import { isGroup } from "../lib/chatList";
import { fullStamp } from "../lib/time";
import type { ViewerItem, ViewerPayload } from "../lib/viewer";
import {
  BookmarkIcon,
  ChevronIcon,
  DoubleChevronIcon,
  DownloadIcon,
  ExternalIcon,
  FitIcon,
  GridIcon,
  MinusIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  RotateIcon,
  VolumeIcon,
} from "./Icons";
import { itemClass, popupClass, Spinner } from "./kit";
import { VirtualGrid } from "./VirtualGrid";

type View = { zoom: number; rotate: number; x: number; y: number };
const FIT: View = { zoom: 1, rotate: 0, x: 0, y: 0 };
const clampZoom = (z: number) => Math.min(8, Math.max(0.25, z));

/** LAAB's photo/video window, laid out like LINE for Mac's: tools over the
 *  photo, big ‹ › arrows, and a strip of the chat's media underneath. */
export function Viewer() {
  const [payload, setPayload] = useState<ViewerPayload | null>(null);
  const [index, setIndex] = useState(0);
  const [grid, setGrid] = useState(false);
  const [strip, setStrip] = useState(true);
  const [view, setView] = useState(FIT);
  const [toast, setToast] = useState<string | null>(null);
  useEffect(() => {
    const show = (p: ViewerPayload | null) => {
      if (!p?.items?.length) return;
      setPayload(p);
      setIndex(Math.min(Math.max(0, p.index), p.items.length - 1));
      setGrid(false);
    };
    api.viewerPayload().then(show);
    const off = listen<ViewerPayload>("viewer-show", (e) => show(e.payload));
    return () => {
      off.then((f) => f());
    };
  }, []);

  const items = payload?.items ?? [];
  const item = items[index] as ViewerItem | undefined;
  useEffect(() => setView(FIT), [item]);
  const say = (text: string) => {
    setToast(text);
    setTimeout(() => setToast((t) => (t === text ? null : t)), 3000);
  };

  const title = useChatList()?.chats.find((c) => c.id === item?.chatId)?.name;
  useEffect(() => {
    if (title)
      getCurrentWindow()
        .setTitle(title)
        .catch(() => {});
  }, [title]);

  const go = (i: number) => {
    setIndex(Math.min(Math.max(0, i), Math.max(0, items.length - 1)));
    setGrid(false);
  };
  const zoomBy = (f: number) => setView((v) => ({ ...v, zoom: clampZoom(v.zoom * f) }));
  useHotkey("ArrowLeft", () => go(index - 1));
  useHotkey("ArrowRight", () => go(index + 1));
  useHotkey("Escape", () => (grid ? setGrid(false) : getCurrentWindow().close()));
  useHotkey("Mod+=", () => zoomBy(1.25));
  useHotkey("Mod+-", () => zoomBy(0.8));
  useHotkey("Mod+0", () => setView(FIT));

  // Fetch the neighbours ahead so stepping does not wait.
  useEffect(() => {
    for (const n of [items[index - 1], items[index + 1]]) if (n && !n.video) new Image().src = n.src;
  }, [items, index]);

  const photo = item && !item.video && !grid;
  return (
    <div className="flex h-full flex-col bg-[#111] text-white select-none" style={{ colorScheme: "dark" }}>
      <div className="relative min-h-0 flex-1 overflow-hidden">
        {grid ? (
          <Grid items={items} index={index} onPick={go} />
        ) : (
          item && <Stage key={`${index}:${item.src}`} item={item} view={view} setView={setView} />
        )}
        <div className="pointer-events-none absolute inset-x-0 top-0 h-[96px] bg-linear-to-b from-black/55 to-transparent" />
        <div className="absolute top-3 left-4 flex items-center gap-1">
          <Tool title="ดูทั้งหมด" active={grid} onClick={() => setGrid((g) => !g)}>
            <GridIcon size={22} />
          </Tool>
          {photo && (
            <>
              <span className="mx-1.5 h-5 w-px bg-white/30" />
              <Tool title="ย่อ (⌘-)" onClick={() => zoomBy(0.8)}>
                <MinusIcon size={20} />
              </Tool>
              <Tool title="ขยาย (⌘=)" onClick={() => zoomBy(1.25)}>
                <PlusIcon size={20} />
              </Tool>
              <Tool title="พอดีหน้าต่าง (⌘0)" onClick={() => setView((v) => ({ ...FIT, rotate: v.rotate }))}>
                <FitIcon size={19} />
              </Tool>
              <Tool title="หมุน" onClick={() => setView((v) => ({ ...v, rotate: v.rotate - 90 }))}>
                <RotateIcon size={21} />
              </Tool>
            </>
          )}
        </div>
        {item && !grid && <Caption item={item} />}
        {item && <Actions item={item} say={say} />}
        {!grid && index > 0 && <Arrow dir="left" onClick={() => go(index - 1)} />}
        {!grid && index < items.length - 1 && <Arrow dir="right" onClick={() => go(index + 1)} />}
        {toast && (
          <div className="absolute top-16 left-1/2 max-w-[70%] -translate-x-1/2 truncate rounded-full bg-black/70 px-4 py-1.5 text-[12px]">
            {toast}
          </div>
        )}
      </div>
      {items.length > 1 && <Strip items={items} index={index} open={strip} onToggle={() => setStrip((s) => !s)} onPick={go} />}
    </div>
  );
}

function Tool({ title, onClick, active, children }: { title: string; onClick: () => void; active?: boolean; children: React.ReactNode }) {
  return (
    <button
      title={title}
      onClick={onClick}
      className={`grid h-9 w-9 place-items-center rounded-md drop-shadow ${active ? "text-line-green" : "text-white/90 hover:text-white"}`}
    >
      {children}
    </button>
  );
}

function Arrow({ dir, onClick }: { dir: "left" | "right"; onClick: () => void }) {
  return (
    <button
      title={dir === "left" ? "ก่อนหน้า (←)" : "ถัดไป (→)"}
      onClick={onClick}
      className={`absolute top-1/2 ${dir === "left" ? "left-2" : "right-2"} grid h-16 w-10 -translate-y-1/2 place-items-center text-white/80 drop-shadow hover:text-white`}
    >
      <ChevronIcon dir={dir} size={34} strokeWidth={1.4} />
    </button>
  );
}

/** Sender and time over the photo; › shows the message in the chat. */
function Caption({ item }: { item: ViewerItem }) {
  const name = useSenderName(item);
  if (!name && !item.time) return null;
  const jump = item.chatId && item.messageId;
  return (
    <div className="absolute top-3 left-1/2 flex -translate-x-1/2 flex-col items-center text-[13px] leading-snug drop-shadow">
      {name && <span className="font-semibold">{name}</span>}
      {item.time && (
        <button
          disabled={!jump}
          title={jump ? "ดูในแชท" : undefined}
          onClick={() => jump && api.showMessage(item.chatId!, item.messageId!)}
          className="flex items-center gap-1 text-white/90 enabled:hover:text-white"
        >
          {fullStamp(item.time)}
          {jump && <ChevronIcon dir="right" size={11} />}
        </button>
      )}
    </div>
  );
}

function useSenderName(item: ViewerItem) {
  const session = useSession();
  const list = useChatList();
  const people = usePeople();
  const chat = list?.chats.find((c) => c.id === item.chatId);
  const group = !!item.chatId && isGroup({ id: item.chatId, kind: chat?.kind ?? "" });
  const { data: members } = useQuery({ ...q.members(item.chatId ?? ""), enabled: group });
  if (item.mine) return session?.profile?.displayName ?? "";
  const mid = item.senderId;
  if (!mid) return "";
  return list?.chats.find((c) => c.id === mid && c.name !== c.id)?.name ?? members?.find((m) => m.mid === mid)?.name ?? people[mid]?.name ?? "";
}

/** Download (save as) and Keep Memo for chat attachments, and a way out to
 *  another app for everything. */
function Actions({ item, say }: { item: ViewerItem; say: (text: string) => void }) {
  const me = useSession()?.profile?.mid;
  const { chatId, messageId } = item;
  const attachment = !!(chatId && messageId);
  const run = (p: Promise<unknown>, done: string) => p.then(() => done && say(done)).catch((e) => say(errorText(e)));
  const local = () => (attachment ? api.download(chatId!, messageId!, item.fileName ?? null) : Promise.resolve(item.path!));
  const saveAs = async () => {
    const dest = await save({ defaultPath: item.fileName ?? `LINE_${messageId}.${item.video ? "mp4" : "jpg"}` });
    if (dest) run(api.saveAttachment(chatId!, messageId!, dest), "บันทึกแล้ว");
  };
  if (!attachment && !item.path) return null;
  return (
    <div className="absolute top-3 right-4 flex items-center gap-1">
      <Tool title="เปิดด้วยแอปอื่น" onClick={() => run(local().then(api.openPath), "")}>
        <ExternalIcon size={21} />
      </Tool>
      {attachment && (
        <>
          <Tool title="บันทึกเป็น…" onClick={saveAs}>
            <DownloadIcon size={22} />
          </Tool>
          {chatId !== me && (
            <Tool title="ส่งไปยัง Keep Memo" onClick={() => run(api.saveToKeep(chatId!, messageId!), "ส่งไปยัง Keep Memo แล้ว")}>
              <BookmarkIcon size={21} strokeWidth={1.8} />
            </Tool>
          )}
        </>
      )}
    </div>
  );
}

function Stage({ item, view, setView }: { item: ViewerItem; view: View; setView: (f: (v: View) => View) => void }) {
  if (item.video) return <VideoStage item={item} />;
  return <PhotoStage item={item} view={view} setView={setView} />;
}

/** A photo fitted to the window. Zoom with the toolbar, ⌘= / ⌘-, a pinch or
 *  a double-click; drag to pan once zoomed. */
function PhotoStage({ item, view, setView }: { item: ViewerItem; view: View; setView: (f: (v: View) => View) => void }) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const drag = useRef<{ x: number; y: number } | null>(null);
  // The 100ms transition is for toolbar zoom and rotate. While a pan or a
  // pinch is moving the image, it would only make the image trail the hand.
  const [moving, setMoving] = useState(false);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(settle.current), []);
  const transform = `translate(${view.x}px, ${view.y}px) scale(${view.zoom}) rotate(${view.rotate}deg)`;
  const fit = "absolute inset-0 m-auto max-h-full max-w-full object-contain";
  return (
    <div
      className={`absolute inset-0 ${view.zoom > 1 ? "cursor-grab active:cursor-grabbing" : ""}`}
      onWheel={(e) => {
        setMoving(true);
        clearTimeout(settle.current);
        settle.current = setTimeout(() => setMoving(false), 150);
        // A trackpad pinch arrives as a wheel event with ctrlKey.
        if (e.ctrlKey) setView((v) => ({ ...v, zoom: clampZoom(v.zoom * Math.exp(-e.deltaY / 100)) }));
        else if (view.zoom > 1) setView((v) => ({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY }));
      }}
      onDoubleClick={() => setView((v) => (v.zoom > 1 ? { ...FIT, rotate: v.rotate } : { ...v, zoom: 2 }))}
      onPointerDown={(e) => {
        if (view.zoom <= 1) return;
        drag.current = { x: e.clientX, y: e.clientY };
        setMoving(true);
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        drag.current = { x: e.clientX, y: e.clientY };
        setView((v) => ({ ...v, x: v.x + e.clientX - d.x, y: v.y + e.clientY - d.y }));
      }}
      onPointerUp={() => {
        drag.current = null;
        setMoving(false);
      }}
    >
      {!loaded && item.thumb && <img src={item.thumb} className={`${fit} h-full w-full`} style={{ transform }} draggable={false} alt="" />}
      <img
        src={item.src}
        onLoad={() => setLoaded(true)}
        onError={() => setFailed(true)}
        draggable={false}
        className={`${fit} ${moving ? "" : "transition-transform duration-100 ease-out"} ${loaded ? "" : "opacity-0"}`}
        style={{ transform }}
        alt=""
      />
      {!loaded && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 text-[12px] text-white/70">
          {failed ? "โหลดรูปเต็มไม่สำเร็จ" : <Spinner size={16} />}
        </div>
      )}
    </div>
  );
}

const time = (s: number) => {
  if (!Number.isFinite(s)) return "00:00";
  const t = Math.floor(s);
  const mm = String(Math.floor(t / 60) % 60).padStart(2, "0");
  const ss = String(t % 60).padStart(2, "0");
  return t >= 3600 ? `${Math.floor(t / 3600)}:${mm}:${ss}` : `${mm}:${ss}`;
};
const SPEEDS = [0.5, 1, 1.25, 1.5, 2];

/** LINE's video controls: a green progress line, play, sound, time, speed. */
function VideoStage({ item }: { item: ViewerItem }) {
  const ref = useRef<HTMLVideoElement>(null);
  const bar = useRef<HTMLDivElement>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(false);
  // Remembered across videos and launches.
  const [volume, setVolume] = useState(() => {
    const v = Number(localStorage.getItem("videoVolume") ?? 1);
    return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 1;
  });
  useEffect(() => {
    if (ref.current) ref.current.volume = volume;
    localStorage.setItem("videoVolume", String(volume));
  }, [volume]);
  const silent = muted || volume === 0;
  const toggleMute = () => {
    if (silent && volume === 0) setVolume(1);
    setMuted(!silent);
  };
  const [now, setNow] = useState(0);
  const [length, setLength] = useState(0);
  const [speed, setSpeed] = useState(1);
  const [waiting, setWaiting] = useState(false);
  const toggle = () => {
    const v = ref.current;
    if (v) (v.paused ? v.play() : v.pause())?.catch?.(() => {});
  };
  useHotkey("Space", toggle);
  // A clip plays as soon as it opens. If WebKit refuses autoplay with
  // sound, play muted; the speaker shows it, one click turns sound on.
  useEffect(() => {
    const v = ref.current;
    v?.play().catch((e: unknown) => {
      if (!(e instanceof DOMException) || e.name !== "NotAllowedError") return;
      v.muted = true;
      setMuted(true);
      v.play().catch(() => {});
    });
  }, []);
  // While dragging, the bar and the time follow the pointer (`scrub`); the
  // video's own time only catches up once the seek lands, which from the
  // encrypted cache takes a moment.
  const [scrub, setScrub] = useState<number | null>(null);
  const dragging = useRef(false);
  // Holding the bar pauses the video; letting go resumes it if it was playing.
  const resume = useRef(false);
  const at = (clientX: number) => {
    const r = bar.current?.getBoundingClientRect();
    return r && length ? Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * length : null;
  };
  const scrubTo = (clientX: number, exact: boolean) => {
    const v = ref.current;
    const t = at(clientX);
    if (!v || t == null) return;
    setScrub(t);
    // fastSeek jumps to a keyframe, quick enough to follow a drag.
    if (!exact && typeof v.fastSeek === "function") v.fastSeek(t);
    else v.currentTime = t;
  };
  const shown = scrub ?? now;
  return (
    <div className="absolute inset-0">
      <video
        ref={ref}
        src={item.src}
        poster={item.thumb}
        muted={muted}
        onClick={toggle}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onTimeUpdate={(e) => {
          const t = e.currentTarget.currentTime;
          setNow(t);
          // In case `seeked` never comes: let go once playback reaches the target.
          if (!dragging.current && scrub != null && Math.abs(t - scrub) < 1) setScrub(null);
        }}
        onSeeked={(e) => {
          if (dragging.current) return;
          setNow(e.currentTarget.currentTime);
          setScrub(null);
        }}
        onLoadedMetadata={(e) => {
          setLength(e.currentTarget.duration);
          e.currentTarget.volume = volume;
        }}
        onWaiting={() => setWaiting(true)}
        onPlaying={() => setWaiting(false)}
        className="absolute inset-0 m-auto max-h-full max-w-full"
      />
      {waiting && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center">
          <Spinner size={22} />
        </div>
      )}
      <div className="absolute inset-x-0 bottom-0 bg-linear-to-t from-black/60 to-transparent px-5 pt-10 pb-3">
        <div
          ref={bar}
          className="group relative h-3 cursor-pointer"
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            dragging.current = true;
            const v = ref.current;
            resume.current = !!v && !v.paused;
            v?.pause();
            scrubTo(e.clientX, false);
          }}
          onPointerMove={(e) => dragging.current && scrubTo(e.clientX, false)}
          onPointerUp={(e) => {
            if (!dragging.current) return;
            dragging.current = false;
            scrubTo(e.clientX, true);
            if (resume.current) ref.current?.play().catch(() => {});
          }}
          onLostPointerCapture={() => {
            if (!dragging.current) return;
            dragging.current = false;
            if (resume.current) ref.current?.play().catch(() => {});
          }}
        >
          <div className="absolute inset-x-0 top-1/2 h-[3px] -translate-y-1/2 rounded bg-white/35 group-hover:h-[5px]">
            <div className="relative h-full rounded bg-line-green" style={{ width: `${length ? (shown / length) * 100 : 0}%` }}>
              {/* The knob shows on hover, so the bar reads as draggable. */}
              <span className="absolute top-1/2 right-0 h-3 w-3 translate-x-1/2 -translate-y-1/2 rounded-full bg-white opacity-0 shadow group-hover:opacity-100" />
            </div>
          </div>
        </div>
        <div className="mt-2 flex items-center gap-4 text-[14px] tabular-nums">
          <button title={playing ? "หยุด" : "เล่น"} onClick={toggle} className="text-white/90 hover:text-white">
            {playing || (scrub != null && resume.current) ? <PauseIcon size={20} /> : <PlayIcon size={20} />}
          </button>
          <div className="flex items-center gap-2">
            <button title={silent ? "เปิดเสียง" : "ปิดเสียง"} onClick={toggleMute} className="text-white/90 hover:text-white">
              <VolumeIcon size={22} muted={silent} />
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={silent ? 0 : volume}
              onChange={(e) => {
                const v = Number(e.target.value);
                setVolume(v);
                setMuted(v === 0);
              }}
              aria-label="ระดับเสียง"
              className="h-1 w-20 cursor-pointer accent-line-green"
            />
          </div>
          <span>
            <span className="text-line-green">{time(shown)}</span>
            <span className="mx-2 text-white/40">|</span>
            <span>{time(length)}</span>
          </span>
          <span className="flex-1" />
          <Menu.Root>
            <Menu.Trigger className="flex items-center gap-1 rounded bg-black/40 px-2.5 py-1 text-[13px] hover:bg-black/60">
              {Number.isInteger(speed) ? speed.toFixed(1) : speed}x
              <ChevronIcon dir="up" size={10} />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner className="z-50" side="top" align="end" sideOffset={6}>
                <Menu.Popup className={`${popupClass} w-24`}>
                  {SPEEDS.map((s) => (
                    <Menu.Item
                      key={s}
                      className={itemClass}
                      onClick={() => {
                        setSpeed(s);
                        if (ref.current) ref.current.playbackRate = s;
                      }}
                    >
                      <span className="w-4 text-center">{s === speed ? "✓" : ""}</span>
                      {s}x
                    </Menu.Item>
                  ))}
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
        </div>
      </div>
    </div>
  );
}

function Thumb({ item, selected, onClick, className }: { item: ViewerItem; selected: boolean; onClick: () => void; className: string }) {
  return (
    <button
      onClick={onClick}
      className={`relative shrink-0 overflow-hidden bg-white/5 ${className} ${selected ? "outline-2 -outline-offset-2 outline-line-green" : ""}`}
    >
      <img src={item.thumb ?? item.src} decoding="async" draggable={false} className="h-full w-full object-cover" alt="" />
      {item.video && <PlayIcon size={18} className="absolute bottom-1.5 left-1.5 text-white drop-shadow" />}
    </button>
  );
}

/** The strip of the chat's media under the photo; ⌄⌄ folds it away. Only
 *  the thumbnails in view are mounted. */
function Strip({ items, index, open, onToggle, onPick }: { items: ViewerItem[]; index: number; open: boolean; onToggle: () => void; onPick: (i: number) => void }) {
  const row = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    horizontal: true,
    count: items.length,
    getScrollElement: () => row.current,
    estimateSize: () => 62,
    overscan: 6,
    paddingStart: 4,
    paddingEnd: 4,
  });
  useEffect(() => {
    if (open) virtual.scrollToIndex(index, { align: "center" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [index, open]);
  return (
    <div className="shrink-0 border-t border-white/10 bg-[#1a1a1a]">
      <button
        title={open ? "ซ่อนแถบรูป" : "แสดงแถบรูป"}
        onClick={onToggle}
        className="mx-auto flex h-4 w-10 items-center justify-center text-white/50 hover:text-white"
      >
        <DoubleChevronIcon size={13} up={!open} />
      </button>
      {open && (
        <div ref={row} className="overflow-x-auto pb-1.5 [scrollbar-width:none]">
          <div className="relative h-[58px]" style={{ width: virtual.getTotalSize() }}>
            {virtual.getVirtualItems().map((v) => (
              <div key={v.key} className="absolute top-0" style={{ left: v.start }}>
                <Thumb item={items[v.index]} selected={v.index === index} onClick={() => onPick(v.index)} className="h-[58px] w-[58px]" />
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** Every photo and video at once, from the toolbar's grid button. */
function Grid({ items, index, onPick }: { items: ViewerItem[]; index: number; onPick: (i: number) => void }) {
  return (
    <VirtualGrid
      count={items.length}
      minCell={120}
      gap={4}
      padding={16}
      scrollTo={index}
      className="absolute inset-0 pt-12"
      render={(i) => <Thumb item={items[i]} selected={i === index} onClick={() => onPick(i)} className="h-full w-full" />}
    />
  );
}
