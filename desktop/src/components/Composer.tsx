import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState, type ReactNode } from "react";
import { api, errorText } from "../lib/api";
import { readClipboardPng } from "../lib/clipboardImage";
import {
  ALL_MEMBER,
  backspaceMention,
  composingQuery,
  fromSpans,
  insertMention,
  locate,
  matching,
  memberTitle,
  present,
  targets,
} from "../lib/mentions";
import type { ChatMember, DraftMention } from "../lib/types";
import { pushDismiss, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { CloseIcon, PaperclipIcon, SmileIcon, WarningIcon } from "./Icons";

const LINE_HEIGHT = 20;
const MAX_LINES = 6;
const kbd = "rounded border border-separator px-1 py-px font-sans text-[10.5px] text-secondary";

type PendingImage = { id: number; bytes: Uint8Array; name: string; preview: string };

let pendingImageSeq = 0;
const revokePending = (images: PendingImage[]) => {
  for (const image of images) URL.revokeObjectURL(image.preview);
};
const stagePending = (bytes: Uint8Array, name: string, type: string): PendingImage => ({
  id: ++pendingImageSeq,
  bytes,
  name,
  preview: URL.createObjectURL(new Blob([new Uint8Array(bytes).buffer as ArrayBuffer], { type })),
});

export interface ComposerHandle {
  focus: () => void;
  /** Close the mention picker; false if it was not open. */
  dismissPicker: () => boolean;
  /** Send files (dropped on the chat), with Retry for those that fail. */
  sendFiles: (paths: string[]) => Promise<void>;
}

/** The message field. A transparent textarea sits over a mirror that colours
 *  mention tags, since a textarea cannot colour part of its text. */
export const Composer = forwardRef<
  ComposerHandle,
  {
    chatId: string;
    group: boolean;
    disabled: boolean;
    members: ChatMember[] | undefined;
    loadMembers: () => void;
    stickerOpen: boolean;
    onToggleStickers: () => void;
    name: (mid: string) => string;
    picture?: (mid: string) => string | null;
    /** ↑/↓ in an empty field move to the chat above or below, as in LINE. */
    onStep?: (step: 1 | -1) => void;
    /** Left of the sticker button — OA rich-menu chip, etc. */
    toolbarExtra?: ReactNode;
  }
>(function Composer({ chatId, group, disabled, members, loadMembers, stickerOpen, onToggleStickers, name, picture, onStep, toolbarExtra }, ref) {
  const area = useRef<HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const [text, setText] = useState("");
  const [mentions, setMentions] = useState<DraftMention[]>([]);
  const [caret, setCaret] = useState(0);
  const [pickIndex, setPickIndex] = useState(0);
  const [dismissedAt, setDismissedAt] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  // Pasted images stage here until send; each paste adds, it does not replace.
  const [pendingImages, setPendingImages] = useState<PendingImage[]>([]);
  const pendingImagesRef = useRef(pendingImages);
  pendingImagesRef.current = pendingImages;
  useEffect(
    () => () => {
      revokePending(pendingImagesRef.current);
    },
    [],
  );
  const clearPendingImages = () => {
    revokePending(pendingImagesRef.current);
    setPendingImages([]);
  };
  const removePendingImage = (id: number) => {
    setPendingImages((images) => {
      const gone = images.find((image) => image.id === id);
      if (gone) URL.revokeObjectURL(gone.preview);
      return images.filter((image) => image.id !== id);
    });
  };
  // A send that failed stays above the box until it is retried, closed, or
  // something else goes out: a passing status line is easy to miss.
  const [failure, setFailure] = useState<{ title: string; detail: string; retry: (() => void) | null } | null>(null);
  const pickerOpenedAt = useRef(0);
  const focusRequest = useUi((s) => s.composerFocus);
  const mentionRequest = useUi((s) => s.mentionRequest);
  // Replying to or editing a message (from its right-click menu).
  const modeAll = useUi((s) => s.composerMode);
  const mode = modeAll?.chatId === chatId ? modeAll : null;
  const savedDraft = useRef<{ text: string; mentions: DraftMention[] } | null>(null);
  useEffect(() => {
    if (mode?.kind === "edit") {
      savedDraft.current = { text, mentions };
      setText(mode.message.text);
      setMentions(fromSpans(mode.message.text, mode.message.mentions ?? []));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode?.kind, mode?.message.id]);
  const endMode = () => {
    if (mode?.kind === "edit" && savedDraft.current) {
      setText(savedDraft.current.text);
      setMentions(savedDraft.current.mentions);
      savedDraft.current = null;
    }
    setUi({ composerMode: null });
  };
  useEffect(() => (mode ? pushDismiss(endMode) : undefined));

  // Per-chat drafts survive switching chats and relaunching. An AI app can
  // write one through MCP (`draft-changed`), shown even while the chat is open.
  useEffect(() => {
    let live = true;
    setFailure(null);
    setText("");
    setMentions([]);
    clearPendingImages();
    api.draft(chatId).then((d) => {
      if (!live || !d) return;
      setText(d.text);
      setMentions(d.mentions);
    });
    const off = listen<{ chatId: string }>("draft-changed", (e) => {
      if (e.payload.chatId !== chatId) return;
      api.draft(chatId).then((d) => {
        if (!live || !d) return;
        setText(d.text);
        setMentions(d.mentions);
      });
    });
    return () => {
      live = false;
      off.then((f) => f());
    };
    // clearPendingImages reads the latest staged images via a ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatId]);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => api.setDraft(chatId, { text, mentions }).catch(() => {}), 300);
    return () => clearTimeout(saveTimer.current);
  }, [chatId, text, mentions]);

  useEffect(() => {
    if (focusRequest) area.current?.focus();
  }, [focusRequest]);

  useEffect(() => {
    if (!mentionRequest || mentionRequest.chatId !== chatId) return;
    if (!group || disabled) {
      setUi({ mentionRequest: null });
      return;
    }
    const member: ChatMember = { mid: mentionRequest.mid, name: mentionRequest.name };
    const at = area.current?.selectionStart ?? caret;
    const r = insertMention(text, at, member, mentions);
    if (r) {
      update(r.text, r.caret, r.mentions);
    } else {
      // A sender label is not necessarily being typed after an @ query, so
      // insert a complete mention at the current caret when needed.
      const label = `@${mentionRequest.name}`;
      const needSpace = at > 0 && !/\s/.test(text[at - 1]!);
      const inserted = `${needSpace ? " " : ""}${label} `;
      update(text.slice(0, at) + inserted + text.slice(at), at + inserted.length, [
        ...mentions,
        { mid: member.mid, all: false, label },
      ]);
    }
    area.current?.focus();
    setUi({ mentionRequest: null });
    // The request is a one-shot event; draft values are read at click time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mentionRequest]);

  const query = group && !disabled ? composingQuery(text, caret, mentions) : null;
  const pickerOpen = query != null && dismissedAt !== `${caret}:${query}`;
  useEffect(() => {
    if (pickerOpen) {
      loadMembers();
      pickerOpenedAt.current = Date.now();
      setPickIndex(0);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pickerOpen]);
  const candidates = useMemo(() => (query == null ? [] : matching([ALL_MEMBER, ...(members ?? [])], query)), [members, query]);

  useImperativeHandle(ref, () => ({
    focus: () => area.current?.focus(),
    dismissPicker: () => {
      if (!pickerOpen) return false;
      setDismissedAt(`${caret}:${query}`);
      return true;
    },
    sendFiles: (paths) => sendFiles(paths),
  }));
  useEffect(() => (pickerOpen ? pushDismiss(() => setDismissedAt(`${caret}:${query}`)) : undefined), [pickerOpen, caret, query]);

  const update = (next: string, nextCaret: number, nextMentions = mentions) => {
    setText(next);
    setMentions(present(next, nextMentions));
    setCaret(nextCaret);
    requestAnimationFrame(() => {
      const el = area.current;
      if (el && document.activeElement === el) el.setSelectionRange(nextCaret, nextCaret);
    });
  };

  const pick = (m: ChatMember) => {
    const r = insertMention(text, caret, m, mentions);
    if (r) update(r.text, r.caret, r.mentions);
    area.current?.focus();
  };

  // The reqSeq of the last message that failed to send. Sending the same
  // message again reuses it, so if the first try reached LINE but its answer
  // was lost (a timeout), LINE doesn't post it twice.
  const unsent = useRef<{ key: string; reqSeq: number } | null>(null);

  // `silent` (⌥Return): without a notification on the others' devices. It is
  // a way of sending one message, not a mode, so it can't be mistaken for
  // muting the chat.
  const send = async (silent = false) => {
    if (pendingImages.length && !sending && !disabled) {
      const images = pendingImages;
      const caption = text.replace(/\s+$/, "");
      const captionMentions = targets(caption, mentions);
      setSending(true);
      setText("");
      setMentions([]);
      try {
        const left: PendingImage[] = [];
        let error: unknown = null;
        for (const image of images) {
          if (error) {
            left.push(image);
            continue;
          }
          try {
            await api.sendImageBytes(chatId, image.bytes, image.name);
            URL.revokeObjectURL(image.preview);
          } catch (e) {
            error = e;
            left.push(image);
          }
        }
        setPendingImages(left);
        if (error) throw error;
        if (caption.trim()) {
          if (mode?.kind === "edit") await api.editMessage(chatId, mode.message.id, caption);
          else await api.sendText(chatId, caption, captionMentions, mode?.kind === "reply" ? mode.message.id : null, null, silent);
        }
        if (mode) setUi({ composerMode: null });
        setFailure(null);
      } catch (e) {
        setText(caption);
        setMentions(mentions);
        setFailure({ title: "ส่งรูปหรือข้อความไม่สำเร็จ", detail: errorText(e), retry: () => sendLatest.current(silent) });
      } finally {
        setSending(false);
      }
      return;
    }
    const body = text.replace(/\s+$/, "");
    if (!body.trim() || sending || disabled) return;
    const spans = targets(body, mentions);
    const current = mode;
    setSending(true);
    setText("");
    setMentions([]);
    try {
      if (current?.kind === "edit") {
        await api.editMessage(chatId, current.message.id, body, spans);
        savedDraft.current = null;
      } else {
        const replyTo = current?.kind === "reply" ? current.message.id : null;
        const key = JSON.stringify([chatId, body, spans, replyTo, silent]);
        if (unsent.current?.key !== key) unsent.current = { key, reqSeq: await api.newReqSeq() };
        await api.sendText(chatId, body, spans, replyTo, unsent.current.reqSeq, silent);
        unsent.current = null;
      }
      if (current) setUi({ composerMode: null });
      setFailure(null);
    } catch (e) {
      setText(body);
      setFailure({ title: current?.kind === "edit" ? "แก้ไขข้อความไม่สำเร็จ" : "ยังไม่ได้ส่งข้อความนี้", detail: errorText(e), retry: () => sendLatest.current(silent) });
    } finally {
      setSending(false);
    }
  };

  // Retry sends what is in the box now, which may have been changed since.
  const sendLatest = useRef(send);
  sendLatest.current = send;

  const attach = async () => {
    const files = await openDialog({ multiple: true, directory: false });
    if (!files) return;
    const paths = Array.isArray(files) ? files : [files];
    sendFiles(paths);
  };
  // Files go one by one and a failure doesn't stop the rest; Retry sends
  // only the ones that failed.
  const sendFiles = (paths: string[]) =>
    api.sendFiles(chatId, paths).then(
      (failed) => {
        if (!failed.length) return setFailure(null);
        const again = failed.map((f) => f.path);
        setFailure({
          title: paths.length > 1 ? `ส่งแล้ว ${paths.length - failed.length} จาก ${paths.length} ไฟล์` : "ยังไม่ได้ส่งไฟล์นี้",
          detail: failed.map((f) => f.error).join("\n"),
          retry: () => sendFiles(again),
        });
      },
      (e) => setFailure({ title: paths.length > 1 ? "ยังไม่ได้ส่งไฟล์เหล่านี้" : "ยังไม่ได้ส่งไฟล์นี้", detail: errorText(e), retry: () => sendFiles(paths) }),
    );
  useEffect(() => (failure ? pushDismiss(() => setFailure(null)) : undefined), [failure]);

  const onPaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    if (disabled || mode?.kind === "edit") return;
    const data = e.clipboardData;
    const files = Array.from(data.files).filter((f) => f.type.startsWith("image/"));
    const hasImage =
      files.length > 0 ||
      Array.from(data.items).some((item) => item.kind === "file" && item.type.startsWith("image/")) ||
      Array.from(data.types).some((type) => type.startsWith("image/"));
    // Office apps and web pages put a picture of copied text beside the text
    // itself: paste the text, and stage an image only when there is none.
    if (!hasImage || (!files.length && data.getData("text/plain"))) return;
    e.preventDefault();
    const stage = async () => {
      const next: PendingImage[] = [];
      if (files.length) {
        for (const file of files) {
          next.push(stagePending(new Uint8Array(await file.arrayBuffer()), file.name || "paste.png", file.type || "image/png"));
        }
      } else {
        const bytes = await readClipboardPng();
        if (!bytes) return;
        next.push(stagePending(bytes, "paste.png", "image/png"));
      }
      setPendingImages((old) => [...old, ...next]);
    };
    stage().catch((err) => setFailure({ title: "วางรูปไม่สำเร็จ", detail: errorText(err), retry: null }));
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    const silent = e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey;
    if (pendingImages.length && !(pickerOpen && candidates.length) && e.key === "Enter" && (plain || silent || (e.metaKey && !e.shiftKey))) {
      e.preventDefault();
      void send(silent);
      return;
    }
    if (pickerOpen && candidates.length && plain) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        pickerOpenedAt.current = Date.now();
        setPickIndex((i) => (i + (e.key === "ArrowDown" ? 1 : -1) + candidates.length) % candidates.length);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        pick(candidates[pickIndex] ?? candidates[0]);
        return;
      }
    }
    if (pickerOpen && e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setDismissedAt(`${caret}:${query}`);
      return;
    }
    if ((e.key === "ArrowDown" || e.key === "ArrowUp") && plain && onStep && !text && !mode) {
      e.preventDefault();
      onStep(e.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (e.key === "Enter" && (plain || silent || (e.metaKey && !e.shiftKey))) {
      e.preventDefault();
      void send(silent);
      return;
    }
    if (e.key === "Backspace" && plain) {
      const el = e.currentTarget;
      if (el.selectionStart === el.selectionEnd) {
        const r = backspaceMention(text, el.selectionStart, mentions);
        if (r) {
          e.preventDefault();
          update(r.text, r.caret, r.mentions);
        }
      }
    }
  };

  // Mirror: same text, mentions coloured, everything else invisible.
  const spans = locate(text, mentions);
  const mirrored: React.ReactNode[] = [];
  let at = 0;
  for (const s of spans) {
    if (s.start > at) mirrored.push(text.slice(at, s.start));
    mirrored.push(
      <span key={s.start} className="text-mention">
        {text.slice(s.start, s.end)}
      </span>,
    );
    at = s.end;
  }
  mirrored.push(text.slice(at) + "​");

  const lines = Math.min(MAX_LINES, Math.max(1, text.split("\n").length));
  const [height, setHeight] = useState(LINE_HEIGHT);
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "0px";
    const h = Math.min(LINE_HEIGHT * MAX_LINES, Math.max(LINE_HEIGHT, el.scrollHeight));
    el.style.height = `${h}px`;
    setHeight(h);
  }, [text, lines]);

  return (
    <div className="relative border-t border-separator bg-chat px-[14px] pt-3 pb-1.5" onClick={() => area.current?.focus()}>
      {pickerOpen && (
        // Opens while typing "@": no animation.
        <div className="absolute right-4 bottom-full left-4 mb-1 overflow-hidden rounded-xl bg-popover shadow-[0_8px_28px_rgba(0,0,0,0.22)] ring-1 ring-black/10">
          {members === undefined ? (
            <div className="px-4 py-3 text-[13px] text-secondary">กำลังโหลดสมาชิก…</div>
          ) : candidates.length === 0 ? (
            <div className="px-4 py-3 text-[13px] text-secondary">ไม่พบสมาชิก</div>
          ) : (
            <PickerList candidates={candidates} index={pickIndex} setIndex={setPickIndex} openedAt={pickerOpenedAt} onPick={pick} />
          )}
        </div>
      )}
      {failure && (
        <div
          role="alert"
          className="-mx-[14px] -mt-3 mb-3 flex items-start gap-2.5 border-b border-badge-red/25 bg-badge-red/10 px-[14px] py-2.5"
          onClick={(e) => e.stopPropagation()}
        >
          <span className="mt-px shrink-0 text-badge-red">
            <WarningIcon size={16} />
          </span>
          <div className="min-w-0 flex-1 leading-snug">
            <div className="text-[13.5px] font-medium text-badge-red">{failure.title}</div>
            <div className="selectable text-[12.5px] text-pretty whitespace-pre-line text-primary/80">{failure.detail}</div>
          </div>
          {failure.retry && (
            <button
              onClick={() => failure.retry?.()}
              disabled={sending}
              className="h-7 shrink-0 rounded-md bg-badge-red px-3 text-[12.5px] font-medium text-white hover:bg-badge-red/90 disabled:opacity-50"
            >
              ลองอีกครั้ง
            </button>
          )}
          <button title="ปิด" onClick={() => setFailure(null)} className="grid h-7 w-6 shrink-0 place-items-center text-secondary hover:text-primary">
            <CloseIcon size={15} />
          </button>
        </div>
      )}
      {mode && (
        // LINE for Mac: a full-width strip with the quoted sender and text.
        <div className="-mx-[14px] -mt-3 mb-3 flex items-center gap-2.5 border-b border-separator px-[14px] py-2">
          {mode.kind === "reply" && (
            <Avatar
              id={mode.message.mine ? (mode.message.senderId ?? "me") : (mode.message.senderId ?? "")}
              name={name(mode.message.senderId ?? "")}
              picture={picture?.(mode.message.senderId ?? "")}
              size={24}
            />
          )}
          <div className="min-w-0 flex-1 leading-tight">
            <div className="truncate text-[13.5px]">{mode.kind === "edit" ? "แก้ไขข้อความ" : name(mode.message.senderId ?? "")}</div>
            <div className="truncate text-[12.5px] text-secondary">
              {mode.message.text ||
                (
                  { IMAGE: "รูปภาพ", VIDEO: "วิดีโอ", STICKER: "สติกเกอร์", FILE: mode.message.fileName ?? "ไฟล์" } as Record<
                    string,
                    string
                  >
                )[mode.message.contentType] ||
                "ข้อความ"}
            </div>
          </div>
          <button
            title="ยกเลิก (Esc)"
            onClick={(e) => (e.stopPropagation(), endMode())}
            className="grid h-6 w-6 shrink-0 place-items-center self-start text-secondary hover:text-primary"
          >
            <CloseIcon size={15} />
          </button>
        </div>
      )}
      {pendingImages.length > 0 && (
        <div className="mb-2 flex items-center gap-2 rounded-lg bg-field p-2" onClick={(e) => e.stopPropagation()}>
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto">
            {pendingImages.map((image) => (
              <div key={image.id} className="relative shrink-0">
                <img src={image.preview} alt={image.name} title={image.name} className="max-h-20 max-w-28 rounded object-contain" />
                <button
                  type="button"
                  title="เอารูปออก"
                  onClick={() => removePendingImage(image.id)}
                  className="absolute top-0.5 right-0.5 grid h-5 w-5 place-items-center rounded-full bg-black/55 text-white hover:bg-black/75"
                >
                  <CloseIcon size={12} />
                </button>
              </div>
            ))}
          </div>
          <button type="button" title="เอารูปออกทั้งหมด" onClick={clearPendingImages} className="shrink-0 px-2 text-secondary hover:text-primary">
            ลบ
          </button>
          <button type="button" onClick={(e) => send(e.altKey)} disabled={sending} className="shrink-0 rounded bg-line-green px-3 py-1 text-[12px] text-white disabled:opacity-50">
            {pendingImages.length > 1 ? `ส่ง ${pendingImages.length} รูป` : "ส่งรูป"}
          </button>
        </div>
      )}
      <div className="relative" style={{ height }}>
        <div
          ref={mirror}
          aria-hidden
          className="pointer-events-none absolute inset-0 overflow-hidden text-[14px] break-words whitespace-pre-wrap text-primary"
          style={{ lineHeight: `${LINE_HEIGHT}px` }}
        >
          {mirrored}
        </div>
        <textarea
          ref={area}
          rows={1}
          disabled={disabled}
          value={text}
          placeholder="พิมพ์ข้อความ"
          spellCheck={false}
          onChange={(e) => update(e.target.value, e.target.selectionStart)}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onScroll={(e) => {
            if (mirror.current) mirror.current.scrollTop = e.currentTarget.scrollTop;
          }}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
          className="selectable absolute inset-0 w-full resize-none bg-transparent text-[14px] text-transparent caret-primary outline-none placeholder:text-secondary/70 disabled:opacity-50"
          style={{ lineHeight: `${LINE_HEIGHT}px` }}
        />
      </div>
      {/* LINE for Mac: 📎 on the left, 🙂 (stickers) on the right, no send
          button; Return sends. */}
      <div className="mt-7 flex items-center gap-1 text-secondary dark:text-[#c0c0c0]">
        <button
          title="แนบไฟล์"
          disabled={disabled}
          onClick={attach}
          className="grid h-8 w-8 place-items-center hover:text-primary disabled:opacity-40"
        >
          <PaperclipIcon size={21} strokeWidth={1.6} />
        </button>
        <div className="flex-1" />
        {/* The keys to send, while there is something to send. ⌥Return has
            no button, so this line is how people find it. */}
        <span
          aria-hidden
          className={`mr-2 flex min-w-0 items-center gap-1.5 overflow-hidden text-[11px] whitespace-nowrap text-secondary transition-opacity duration-150 ${
            (text.trim() || pendingImages.length) && mode?.kind !== "edit" ? "opacity-100" : "opacity-0"
          }`}
        >
          <kbd className={kbd}>⏎</kbd> ส่ง
          <span className="mx-1 text-separator">|</span>
          <kbd className={kbd}>⌥⏎</kbd> ส่งแบบไม่มีการแจ้งเตือน
        </span>
        {toolbarExtra}
        <button
          title="สติกเกอร์และอิโมจิ"
          aria-label="สติกเกอร์และอิโมจิ"
          aria-expanded={stickerOpen}
          onClick={(e) => (e.stopPropagation(), onToggleStickers())}
          className={`grid h-8 w-8 place-items-center hover:text-primary ${stickerOpen ? "text-line-green" : ""}`}
        >
          <SmileIcon size={23} strokeWidth={1.5} />
        </button>
      </div>
    </div>
  );
});

function PickerList({
  candidates,
  index,
  setIndex,
  openedAt,
  onPick,
}: {
  candidates: ChatMember[];
  index: number;
  setIndex: (i: number) => void;
  openedAt: React.RefObject<number>;
  onPick: (m: ChatMember) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector(`[data-i="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);
  return (
    <div ref={list} className="overflow-y-auto py-1" style={{ maxHeight: 5 * 40 + 8 }}>
      {candidates.map((m, i) => (
        <div
          key={m.mid}
          data-i={i}
          // A resting mouse must not steal the keyboard selection.
          onMouseMove={() => Date.now() - (openedAt.current ?? 0) > 400 && setIndex(i)}
          onMouseDown={(e) => (e.preventDefault(), onPick(m))}
          className={`flex h-10 items-center gap-2.5 px-3.5 ${i === index ? "bg-field" : ""}`}
        >
          <Avatar id={m.mid} name={memberTitle(m)} picture={m.picture} size={28} all={m.all} />
          <span className="truncate text-[13px]">{memberTitle(m)}</span>
        </div>
      ))}
    </div>
  );
}
