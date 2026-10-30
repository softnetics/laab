import { Menu } from "@base-ui/react/menu";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api, errorText, mediaUrl, queryClient, useChatList } from "../lib/api";
import { linkify } from "../lib/links";
import type { NotePost } from "../lib/types";
import { openUrl } from "../lib/open";
import { messageItem, openViewer } from "../lib/viewer";
import { fullStamp, listLabel, toDate } from "../lib/time";
import { notice, setUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { ConfirmDialog } from "./Dialogs";
import { CommentIcon, DocIcon, MoreVIcon, PhotoIcon, PlayIcon, SearchIcon, SmileIcon } from "./Icons";
import { itemClass, popupClass, Spinner } from "./kit";
import { VirtualGrid } from "./VirtualGrid";

// LINE for Mac opens albums/notes and a chat's photos, links and files in
// separate small windows. These are those windows.

function useChatTitle(chatId: string) {
  const chat = useChatList()?.chats.find((c) => c.id === chatId);
  const title = chat?.name ?? "";
  useEffect(() => {
    if (title)
      getCurrentWindow()
        .setTitle(title)
        .catch(() => {});
  }, [title]);
  return title;
}

function Tabs<T extends string>({
  tabs,
  value,
  onChange,
  light = false,
}: {
  tabs: [T, string][];
  value: T;
  onChange: (t: T) => void;
  light?: boolean;
}) {
  return (
    <div className={`flex border-b ${light ? "border-black/10" : "border-white/10"}`}>
      {tabs.map(([id, label]) => (
        // LINE underlines the label, not the whole half.
        <button
          key={id}
          onClick={() => onChange(id)}
          className={`flex h-9 flex-1 items-end justify-center text-[13px] ${value === id ? "font-semibold" : light ? "text-black/40" : "text-secondary"}`}
        >
          <span className="relative pb-2">
            {label}
            {value === id && <span className={`absolute inset-x-0 bottom-0 h-[2px] rounded-full ${light ? "bg-black/80" : "bg-primary"}`} />}
          </span>
        </button>
      ))}
    </div>
  );
}

/** Loading, centred in the space the view will fill. */
function Loading() {
  return (
    <div className="grid flex-1 place-items-center py-16 text-secondary">
      <Spinner size={18} />
    </div>
  );
}

function Empty({ title, body }: { title: string; body?: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-1 px-6 text-center">
      <div className="text-[13px] font-semibold">{title}</div>
      {body && <div className="text-[12px] text-secondary">{body}</div>}
    </div>
  );
}

const Fab = ({ onClick, title }: { onClick: () => void; title: string }) => (
  <button
    title={title}
    onClick={onClick}
    className="absolute right-5 bottom-5 grid h-11 w-11 place-items-center rounded-full bg-line-green text-[26px] leading-none text-white shadow-[0_3px_10px_rgba(0,0,0,0.3)]"
  >
    +
  </button>
);

/** "เมื่อสักครู่" for the last minute, as LINE's notes do. */
const ago = (t: number) => {
  const d = toDate(t);
  return d && Date.now() - d.getTime() < 60_000 ? "เมื่อสักครู่" : listLabel(t);
};

function NoteCompose({ onCancel, onPost }: { onCancel: () => void; onPost: (text: string, photos: string[]) => Promise<void> }) {
  const [text, setText] = useState("");
  const [photos, setPhotos] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const pick = async () => {
    const files = await openDialog({
      multiple: true,
      filters: [{ name: "รูป & วิดีโอ", extensions: ["jpg", "jpeg", "png", "gif", "heic", "webp", "mp4", "mov"] }],
    });
    if (files) setPhotos([...photos, ...(Array.isArray(files) ? files : [files])]);
  };
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <textarea
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="โพสต์เรื่องราวของคุณเลย!"
        className="selectable min-h-0 flex-1 resize-none bg-transparent px-4 py-3 text-[13.5px] outline-none placeholder:text-secondary"
      />
      {photos.length > 0 && (
        <div className="flex gap-1.5 overflow-x-auto px-4 pb-2">
          {photos.map((p) => (
            // File names only: LAAB serves no files from outside its own folder.
            <div key={p} className="flex shrink-0 items-center gap-1 rounded-full bg-field py-0.5 pr-1 pl-2.5 text-[11.5px]">
              <span className="max-w-32 truncate">{p.split("/").pop()}</span>
              <button
                aria-label="เอารูปออก"
                onClick={() => setPhotos(photos.filter((x) => x !== p))}
                className="grid h-4 w-4 place-items-center text-secondary hover:text-primary"
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}
      {/* LINE's composer has emoji, photo and screenshot buttons; LAAB has photos. */}
      <div className="flex items-center px-3 pb-2 text-secondary">
        <button title="รูป" onClick={pick} className="grid h-8 w-8 place-items-center hover:text-primary">
          <PhotoIcon size={20} />
        </button>
      </div>
      <div className="flex justify-center gap-2 pb-5">
        <button className="h-8 w-[76px] rounded-[4px] bg-[#555] text-[12.5px] font-semibold text-white" onClick={onCancel}>
          ยกเลิก
        </button>
        <button
          disabled={(!text.trim() && !photos.length) || busy}
          className="h-8 w-[76px] rounded-[4px] bg-line-green text-[12.5px] font-semibold text-white disabled:opacity-50"
          onClick={() => {
            setBusy(true);
            onPost(text, photos).finally(() => setBusy(false));
          }}
        >
          {busy ? "…" : "โพสต์"}
        </button>
      </div>
    </div>
  );
}

/** Photos open in the viewer when `chatId` is given; in the note list a click
 *  opens the note instead. */
function NoteMediaGrid({ media, chatId }: { media: NotePost["media"]; chatId?: string }) {
  if (!media.length) return null;
  const cols = media.length === 1 ? "grid-cols-1" : "grid-cols-2";
  return (
    <div className={`mt-2 grid gap-[2px] ${cols}`}>
      {media.map((m, i) => (
        <div
          key={m.objectId}
          className={`relative aspect-square overflow-hidden bg-field ${chatId ? "cursor-pointer" : ""}`}
          onClick={() => chatId && openViewer(media.map((x) => ({ src: mediaUrl.note("h", x.objectId), chatId })), i)}
        >
          <img src={mediaUrl.note("h", m.objectId)} className="h-full w-full object-cover" alt="" />
          {m.video && <PlayIcon size={18} className="absolute right-1.5 bottom-1.5 text-white drop-shadow" />}
        </div>
      ))}
    </div>
  );
}

/** A note's photos in the list, laid out as LINE does: one large photo with
 *  two beside it and a row of up to three below; "+N" on the last when the
 *  note has more. */
function NoteMosaic({ media, total }: { media: NotePost["media"]; total: number }) {
  const shown = media.slice(0, 6);
  const n = shown.length;
  if (!n) return null;
  const more = Math.max(total, media.length) - n;
  const tile = (i: number, className = "") => (
    <div key={shown[i].objectId} className={`relative overflow-hidden bg-field ${className}`}>
      <img src={mediaUrl.note("h", shown[i].objectId)} decoding="async" className="absolute inset-0 h-full w-full object-cover" alt="" />
      {shown[i].video && <PlayIcon size={18} className="absolute right-1.5 bottom-1.5 text-white drop-shadow" />}
      {i === n - 1 && more > 0 && (
        <div className="absolute inset-0 grid place-items-center bg-black/30 text-[22px] text-white">+{more}</div>
      )}
    </div>
  );
  // A large photo on the left with two stacked on its right. Built only for
  // 3+ photos: it reads tiles 1 and 2, which a 1- or 2-photo note lacks.
  const lead = () => (
    <div className="grid aspect-[3/2] grid-cols-3 grid-rows-2 gap-[2px]">
      {tile(0, "col-span-2 row-span-2")}
      {tile(1)}
      {tile(2)}
    </div>
  );
  const rest = shown.slice(3).map((_, j) => tile(j + 3));
  return (
    <div className="mt-2.5 flex flex-col gap-[2px] overflow-hidden rounded-[6px]">
      {n === 1 && <div className="grid aspect-square">{tile(0)}</div>}
      {n === 2 && <div className="grid aspect-[2/1] grid-cols-2 gap-[2px]">{[tile(0), tile(1)]}</div>}
      {n === 3 && lead()}
      {n === 4 && <div className="grid aspect-square grid-cols-2 grid-rows-2 gap-[2px]">{[0, 1, 2, 3].map((i) => tile(i))}</div>}
      {n >= 5 && (
        <>
          {lead()}
          <div className={`grid gap-[2px] ${rest.length === 3 ? "aspect-[3/1] grid-cols-3" : "aspect-[2/1] grid-cols-2"}`}>{rest}</div>
        </>
      )}
    </div>
  );
}

/** One note: its photos, comments and a comment box. */
function NoteView({ chatId, postId, onBack }: { chatId: string; postId: string; onBack: () => void }) {
  const key = ["note", chatId, postId];
  const { data, isLoading, error } = useQuery({ queryKey: key, queryFn: () => api.noteDetail(chatId, postId), staleTime: 0 });
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);
  const send = () => {
    if (!comment.trim() || busy) return;
    setBusy(true);
    api
      .addComment(chatId, postId, comment)
      .then(() => {
        setComment("");
        queryClient.invalidateQueries({ queryKey: key });
      })
      .catch((e) => notice(errorText(e)))
      .finally(() => setBusy(false));
  };
  const n = data?.note;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <button className="px-4 py-2 text-left text-[12.5px] text-secondary hover:text-primary" onClick={onBack}>
        ‹ โน้ตทั้งหมด
      </button>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isLoading && <Loading />}
        {error && <p className="p-4 text-[12.5px] text-badge-red">{errorText(error)}</p>}
        {n && (
          <div className="border-b border-white/10 px-4 pb-3">
            <div className="flex items-center gap-2">
              <Avatar id={n.authorMid} name={n.authorName} picture={n.authorPicture} size={30} />
              <div className="min-w-0">
                <div className="truncate text-[13px]">{n.authorName || "สมาชิก"}</div>
                <div className="text-[11px] text-secondary">{ago(n.createdTime)}</div>
              </div>
            </div>
            {n.text && <div className="selectable mt-2.5 text-[13.5px] whitespace-pre-wrap">{n.text}</div>}
            <NoteMediaGrid media={n.media} chatId={chatId} />
            <div className="mt-2 text-[11.5px] text-secondary">
              {n.likeCount > 0 && `ถูกใจ ${n.likeCount} · `}ความคิดเห็น {data.comments.length || n.commentCount}
            </div>
          </div>
        )}
        {data?.comments.map((c) => (
          <div key={c.id} className="flex gap-2.5 px-4 py-2.5">
            <Avatar id={c.authorMid} name={c.authorName} picture={c.authorPicture} size={28} />
            <div className="min-w-0 flex-1">
              <div className="flex items-baseline gap-2">
                <span className="truncate text-[12.5px] font-semibold">{c.authorName || "สมาชิก"}</span>
                <span className="shrink-0 text-[11px] text-secondary">{ago(c.createdTime)}</span>
              </div>
              {c.text && <div className="selectable text-[13px] whitespace-pre-wrap">{c.text}</div>}
              {c.images.map((id) => (
                <img key={id} src={mediaUrl.note("cmt", id)} className="mt-1.5 max-h-40 max-w-[200px] rounded-[4px] object-cover" alt="" />
              ))}
            </div>
          </div>
        ))}
        {data && data.comments.length === 0 && <p className="px-4 py-6 text-center text-[12px] text-secondary">ยังไม่มีความคิดเห็น</p>}
      </div>
      <div className="flex items-center gap-2 border-t border-white/10 px-3 py-2">
        <input
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && send()}
          placeholder="แสดงความคิดเห็น"
          className="selectable h-8 min-w-0 flex-1 rounded-full bg-field px-3 text-[13px] outline-none placeholder:text-secondary"
        />
        <button disabled={!comment.trim() || busy} onClick={send} className="text-[13px] font-semibold text-line-green disabled:opacity-40">
          ส่ง
        </button>
      </div>
    </div>
  );
}

function Notes({ chatId, post, at }: { chatId: string; post: string | null; at: string }) {
  const key = ["notes", chatId];
  const [viewing, setViewing] = useState<string | null>(post);
  useEffect(() => setViewing(post), [post, at]);
  const { data, isLoading, error } = useQuery({ queryKey: key, queryFn: () => api.listNotes(chatId), staleTime: 0 });
  const [query, setQuery] = useState("");
  const [composing, setComposing] = useState(false);
  if (viewing) return <NoteView chatId={chatId} postId={viewing} onBack={() => setViewing(null)} />;
  if (composing) {
    return (
      <NoteCompose
        onCancel={() => setComposing(false)}
        onPost={(text, photos) =>
          api
            .createNote(chatId, text, photos)
            .then(() => {
              setComposing(false);
              queryClient.invalidateQueries({ queryKey: key });
            })
            .catch((e) => notice(errorText(e)))
        }
      />
    );
  }
  const q = query.trim().toLocaleLowerCase("th");
  const notes = (data ?? []).filter((n) => !q || n.text.toLocaleLowerCase("th").includes(q));
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <label className="mx-3 mt-2.5 mb-1 flex h-8 items-center gap-2 rounded-[6px] bg-field px-2.5 text-secondary">
        <SearchIcon size={14} />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="ค้นหา"
          className="selectable min-w-0 flex-1 bg-transparent text-[13px] text-primary outline-none placeholder:text-secondary"
        />
      </label>
      {isLoading && <Loading />}
      {error && <p className="p-4 text-[12.5px] text-badge-red">{errorText(error)}</p>}
      {data && notes.length === 0 && <Empty title="ยังไม่มีโน้ต" body="บันทึกข้อมูลสำคัญและแชร์กับเพื่อนๆ ได้ที่โน้ต" />}
      {notes.length > 0 && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {notes.map((n) => (
            <div key={n.id} className="px-4 pt-3 pb-4">
              <div className="flex items-center gap-2.5">
                <Avatar id={n.authorMid} name={n.authorName} picture={n.authorPicture} size={30} />
                <span className="flex-1 truncate text-[13.5px]">{n.authorName || "สมาชิก"}</span>
                <Menu.Root>
                  <Menu.Trigger aria-label="เมนูโน้ต" className="grid h-6 w-6 place-items-center text-secondary hover:text-primary">
                    <MoreVIcon size={15} />
                  </Menu.Trigger>
                  <Menu.Portal>
                    <Menu.Positioner className="z-50" align="end" sideOffset={2}>
                      <Menu.Popup className={`${popupClass} min-w-24!`}>
                        <Menu.Item
                          className={itemClass}
                          onClick={() =>
                            setUi({
                              confirm: {
                                title: "ต้องการลบโน้ตนี้หรือไม่",
                                action: "ลบ",
                                run: () => api.deleteNote(chatId, n.id).then(() => queryClient.invalidateQueries({ queryKey: key })),
                              },
                            })
                          }
                        >
                          ลบ
                        </Menu.Item>
                      </Menu.Popup>
                    </Menu.Positioner>
                  </Menu.Portal>
                </Menu.Root>
              </div>
              <button className="block w-full text-left" onClick={() => setViewing(n.id)}>
                {n.text && <div className="mt-2 line-clamp-6 text-[13.5px] whitespace-pre-wrap">{n.text}</div>}
                <NoteMosaic media={n.media} total={n.mediaCount} />
              </button>
              {/* LINE: react, comment and share icons, then the full date.
                  LAAB cannot react or share a note yet. */}
              <div className="mt-2.5 flex items-center gap-3 text-secondary">
                <span className="flex items-center gap-1 text-[12px]" title="ถูกใจ">
                  <SmileIcon size={20} strokeWidth={1.6} />
                  {n.likeCount > 0 && n.likeCount}
                </span>
                <button className="flex items-center gap-1 text-[12px] hover:text-primary" title="ความคิดเห็น" onClick={() => setViewing(n.id)}>
                  <CommentIcon size={21} />
                  {n.commentCount > 0 && n.commentCount}
                </button>
              </div>
              <div className="mt-1.5 text-[12px] text-secondary">{fullStamp(n.createdTime)}</div>
            </div>
          ))}
        </div>
      )}
      <Fab title="เขียนโน้ต" onClick={() => setComposing(true)} />
    </div>
  );
}

/** An album's first four photos in a square, as LINE's album list shows. */
function AlbumCover({ chatId, albumId }: { chatId: string; albumId: string }) {
  const { data } = useQuery({
    queryKey: ["album-cover", chatId, albumId],
    queryFn: () => api.albumPhotos(chatId, albumId, 4),
    staleTime: 5 * 60_000,
  });
  return (
    <div className="grid aspect-square grid-cols-2 grid-rows-2 gap-[2px] overflow-hidden rounded-[6px]">
      {[0, 1, 2, 3].map((i) => {
        const p = data?.[i];
        return (
          <div key={i} className="relative overflow-hidden bg-field">
            {p && <img src={mediaUrl.album(chatId, albumId, p.oid)} decoding="async" className="absolute inset-0 h-full w-full object-cover" alt="" />}
            {p?.video && <PlayIcon size={16} className="absolute right-1 bottom-1 text-white drop-shadow" />}
          </div>
        );
      })}
    </div>
  );
}

function Albums({ chatId }: { chatId: string }) {
  const key = ["albums", chatId];
  const { data, isLoading, error } = useQuery({ queryKey: key, queryFn: () => api.listAlbums(chatId), staleTime: 0 });
  const [open, setOpen] = useState<{ id: string; title: string; count: number } | null>(null);
  const [naming, setNaming] = useState<{ id: string | null; title: string } | null>(null);
  const photos = useQuery({ queryKey: ["album", chatId, open?.id], queryFn: () => api.albumPhotos(chatId, open!.id), enabled: !!open });
  const refresh = () => queryClient.invalidateQueries({ queryKey: key });
  const act = (p: Promise<unknown>) => p.then(refresh).catch((e) => notice(errorText(e)));
  if (naming) {
    const submit = () => {
      if (!naming.title.trim()) return;
      act(naming.id ? api.renameAlbum(chatId, naming.id, naming.title) : api.createAlbum(chatId, naming.title));
      setNaming(null);
    };
    return (
      <div className="flex flex-1 flex-col gap-3 p-4">
        <input
          autoFocus
          value={naming.title}
          onChange={(e) => setNaming({ ...naming, title: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && submit()}
          placeholder="ชื่ออัลบั้ม"
          className="selectable h-9 rounded-[6px] bg-field px-3 text-[13.5px] outline-none placeholder:text-secondary"
        />
        <div className="flex justify-center gap-2">
          <button className="h-8 w-[76px] rounded-[4px] bg-[#555] text-[12.5px] font-semibold text-white" onClick={() => setNaming(null)}>
            ยกเลิก
          </button>
          <button
            disabled={!naming.title.trim()}
            className="h-8 w-[76px] rounded-[4px] bg-line-green text-[12.5px] font-semibold text-white disabled:opacity-50"
            onClick={submit}
          >
            {naming.id ? "บันทึก" : "สร้าง"}
          </button>
        </div>
      </div>
    );
  }
  if (open) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <button className="px-4 py-2 text-left text-[12.5px] text-secondary hover:text-primary" onClick={() => setOpen(null)}>
          ‹ {open.title}
        </button>
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-1 pb-4">
          {photos.error && <p className="p-3 text-[12.5px] text-badge-red">{errorText(photos.error)}</p>}
          {photos.data?.length === 0 && <Empty title="ยังไม่มีรูปในอัลบั้มนี้" />}
          <div className="grid grid-cols-3 gap-[2px]">
            {/* While the list loads, empty tiles hold the grid's shape. */}
            {photos.isLoading &&
              Array.from({ length: Math.min(open.count || 9, 30) }, (_, i) => <div key={i} className="aspect-square bg-field" />)}
            {photos.data?.map((p, i) => (
              <button
                key={p.id}
                className="relative aspect-square overflow-hidden bg-field"
                onClick={() =>
                  openViewer(
                    photos.data!.map((x) => ({ src: mediaUrl.album(chatId, open.id, x.oid), path: x.path, chatId })),
                    i,
                  )
                }
              >
                <img
                  src={mediaUrl.album(chatId, open.id, p.oid)}
                  loading="lazy"
                  decoding="async"
                  className="h-full w-full object-cover"
                  draggable={false}
                  alt=""
                />
                {p.video && <PlayIcon size={16} className="absolute right-1 bottom-1 text-white drop-shadow" />}
              </button>
            ))}
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {isLoading && (
        <div className="px-4 pt-3" aria-label="กำลังโหลดอัลบั้ม">
          {[0, 1].map((i) => (
            <div key={i} className="mb-6">
              <div className="aspect-square rounded-[6px] bg-field" />
              <div className="mt-2 h-3.5 w-2/5 rounded bg-field" />
              <div className="mt-1.5 h-3 w-10 rounded bg-field" />
            </div>
          ))}
        </div>
      )}
      {error && <p className="p-4 text-[12.5px] text-badge-red">{errorText(error)}</p>}
      {data?.length === 0 && <Empty title="ยังไม่มีอัลบั้ม" body="แบ่งปันช่วงเวลาพิเศษของคุณที่อัลบั้ม" />}
      {!!data?.length && (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-20">
          {data.map((a) => (
            <div key={a.id} className="mb-6">
              <button className="block w-full" onClick={() => setOpen({ id: a.id, title: a.title, count: a.photoCount })}>
                <AlbumCover chatId={chatId} albumId={a.id} />
              </button>
              <div className="mt-2 flex items-center gap-2">
                <button className="min-w-0 flex-1 text-left" onClick={() => setOpen({ id: a.id, title: a.title, count: a.photoCount })}>
                  <div className="truncate text-[13.5px] font-semibold">{a.title || "อัลบั้ม"}</div>
                  <div className="text-[13px] text-secondary">{a.photoCount}</div>
                </button>
                <Menu.Root>
                  <Menu.Trigger aria-label="เมนูอัลบั้ม" className="grid h-6 w-6 place-items-center text-secondary hover:text-primary">
                    <MoreVIcon size={15} />
                  </Menu.Trigger>
                  <Menu.Portal>
                    <Menu.Positioner className="z-50" align="end" sideOffset={2}>
                      <Menu.Popup className={`${popupClass} min-w-28!`}>
                        <Menu.Item className={itemClass} onClick={() => setNaming({ id: a.id, title: a.title })}>
                          แก้ไขชื่อ
                        </Menu.Item>
                        <Menu.Item
                          className={itemClass}
                          onClick={() =>
                            setUi({
                              confirm: {
                                title: "ต้องการลบอัลบั้มนี้หรือไม่",
                                action: "ลบ",
                                run: () => api.deleteAlbum(chatId, a.id).then(refresh),
                              },
                            })
                          }
                        >
                          ลบ
                        </Menu.Item>
                      </Menu.Popup>
                    </Menu.Positioner>
                  </Menu.Portal>
                </Menu.Root>
              </div>
            </div>
          ))}
        </div>
      )}
      <Fab title="สร้างอัลบั้ม" onClick={() => setNaming({ id: null, title: "" })} />
    </div>
  );
}

/** อัลบั้ม / โน้ต, one window with two tabs, as in LINE. */
/** `at` changes on every request to open the window, so a repeated
 *  "ดูโน้ต" reopens the note even when the URL is otherwise the same. */
export function AlbumsNotesWindow({ chatId, tab, post, at }: { chatId: string; tab: "albums" | "notes"; post: string | null; at: string }) {
  const title = useChatTitle(chatId);
  const [current, setCurrent] = useState(tab);
  useEffect(() => setCurrent(tab), [tab, at]);
  return (
    <div className="flex h-full flex-col bg-chat">
      {/* LINE puts the title on its own row under the traffic lights. */}
      <div className="drag h-[26px] shrink-0" data-tauri-drag-region />
      <div className="drag flex h-[30px] shrink-0 items-center justify-center px-10 text-[14px] font-semibold" data-tauri-drag-region>
        <span className="truncate">{title}</span>
      </div>
      <Tabs
        tabs={[
          ["albums", "อัลบั้ม"],
          ["notes", "โน้ต"],
        ]}
        value={current}
        onChange={setCurrent}
      />
      {current === "albums" ? <Albums chatId={chatId} /> : <Notes chatId={chatId} post={post} at={at} />}
      <ConfirmDialog />
    </div>
  );
}

/** รูป & วิดีโอ / ลิงก์ / ไฟล์ of a chat, from what LAAB has synced. LINE
 *  draws this window white even in dark mode. */
export function MediaWindow({ chatId, initial }: { chatId: string; initial: "media" | "links" | "files" }) {
  const title = useChatTitle(chatId);
  const [tab, setTab] = useState(initial);
  useEffect(() => setTab(initial), [initial]);
  const { data, isLoading, error } = useQuery({ queryKey: ["chat-media", chatId, tab], queryFn: () => api.chatMedia(chatId, tab), staleTime: 0 });
  const empty = { media: "ไม่มีรูปที่แชร์ในแชทนี้", links: "ไม่มีลิงก์ที่แชร์ในแชทนี้", files: "ไม่มีไฟล์ที่แชร์ในแชทนี้" }[tab];
  const openFile = (id: string, name: string | null) =>
    api
      .download(chatId, id, name)
      .then(api.openPath)
      .catch((e) => notice(errorText(e)));
  return (
    <div className="flex h-full flex-col bg-white text-[#111111]" style={{ colorScheme: "light" }}>
      <div className="drag flex h-[30px] shrink-0 items-center justify-center px-[76px] text-[13px] font-semibold" data-tauri-drag-region>
        <span className="truncate">{title}</span>
      </div>
      <Tabs
        light
        tabs={[
          ["media", "รูป & วิดีโอ"],
          ["links", "ลิงก์"],
          ["files", "ไฟล์"],
        ]}
        value={tab}
        onChange={setTab}
      />
      {isLoading && <Loading />}
      {error && <div className="flex flex-1 items-center justify-center px-6 text-center text-[12px] text-[#555]">{errorText(error)}</div>}
      {data?.length === 0 && <div className="flex flex-1 items-center justify-center text-[12px] text-[#555]">{empty}</div>}
      {!!data?.length && tab === "media" && (
        <VirtualGrid
          count={data.length}
          columns={4}
          className="flex-1"
          render={(i) => {
            const m = data[i];
            return (
              <button
                className="h-full w-full overflow-hidden bg-[#eee]"
                onClick={() => openViewer(data.map((x) => messageItem(chatId, x)).reverse(), data.length - 1 - i)}
              >
                <img src={messageItem(chatId, m).thumb} decoding="async" className="h-full w-full object-cover" draggable={false} alt="" />
                {m.contentType === "VIDEO" && <PlayIcon size={16} className="absolute right-1 bottom-1 text-white drop-shadow" />}
              </button>
            );
          }}
        />
      )}
      {!!data?.length && tab !== "media" && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {tab === "files" &&
            data.map((m) => (
              <button
                key={m.id}
                onClick={() => openFile(m.id, m.fileName)}
                className="flex w-full items-center gap-3 border-b border-black/5 px-4 py-2.5 text-left hover:bg-black/5"
              >
                <DocIcon size={26} className="shrink-0 text-[#9a9a9a]" />
                <div className="min-w-0">
                  <div className="truncate text-[13px]">{m.fileName ?? "ไฟล์"}</div>
                  <div className="text-[11px] text-[#888]">{listLabel(m.createdTime)}</div>
                </div>
              </button>
            ))}
          {tab === "links" &&
            data.flatMap((m) =>
              linkify(m.text)
                .filter((s) => s.url)
                .map((s, i) => (
                  <button
                    key={`${m.id}-${i}`}
                    onClick={() => openUrl(s.url!, chatId)}
                    className="block w-full border-b border-black/5 px-4 py-2.5 text-left hover:bg-black/5"
                  >
                    <div className="truncate text-[13px] text-[#2b65c9]">{s.text}</div>
                    <div className="text-[11px] text-[#888]">{listLabel(m.createdTime)}</div>
                  </button>
                )),
            )}
        </div>
      )}
    </div>
  );
}
