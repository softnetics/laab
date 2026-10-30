import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { api, errorText } from "../lib/api";
import {
  ALL_MEMBER,
  backspaceMention,
  composingQuery,
  insertMention,
  locate,
  matching,
  memberTitle,
  present,
  targets,
} from "../lib/mentions";
import type { ChatMember, DraftMention } from "../lib/types";
import { notice, pushDismiss, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { CloseIcon, PaperclipIcon, SmileIcon } from "./Icons";

const LINE_HEIGHT = 20;
const MAX_LINES = 6;

export interface ComposerHandle {
  focus: () => void;
  /** Close the mention picker; false if it was not open. */
  dismissPicker: () => boolean;
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
  }
>(function Composer({ chatId, group, disabled, members, loadMembers, stickerOpen, onToggleStickers, name, picture, onStep }, ref) {
  const area = useRef<HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const [text, setText] = useState("");
  const [mentions, setMentions] = useState<DraftMention[]>([]);
  const [caret, setCaret] = useState(0);
  const [pickIndex, setPickIndex] = useState(0);
  const [dismissedAt, setDismissedAt] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const pickerOpenedAt = useRef(0);
  const focusRequest = useUi((s) => s.composerFocus);
  // Replying to or editing a message (from its right-click menu).
  const modeAll = useUi((s) => s.composerMode);
  const mode = modeAll?.chatId === chatId ? modeAll : null;
  const savedDraft = useRef<{ text: string; mentions: DraftMention[] } | null>(null);
  useEffect(() => {
    if (mode?.kind === "edit") {
      savedDraft.current = { text, mentions };
      setText(mode.message.text);
      setMentions([]);
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

  // Per-chat drafts survive switching chats and relaunching.
  useEffect(() => {
    let live = true;
    setText("");
    setMentions([]);
    api.draft(chatId).then((d) => {
      if (!live || !d) return;
      setText(d.text);
      setMentions(d.mentions);
    });
    return () => {
      live = false;
    };
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

  const send = async () => {
    const body = text.replace(/\s+$/, "");
    if (!body.trim() || sending || disabled) return;
    const spans = targets(body, mentions);
    const current = mode;
    setSending(true);
    setText("");
    setMentions([]);
    try {
      if (current?.kind === "edit") {
        await api.editMessage(chatId, current.message.id, body);
        savedDraft.current = null;
      } else {
        await api.sendText(chatId, body, spans, current?.kind === "reply" ? current.message.id : null);
      }
      if (current) setUi({ composerMode: null });
    } catch (e) {
      setText(body);
      notice(current?.kind === "edit" ? errorText(e) : `ส่งข้อความไม่สำเร็จ: ${errorText(e)}`);
    } finally {
      setSending(false);
    }
  };

  const attach = async () => {
    const files = await openDialog({ multiple: true, directory: false });
    if (!files) return;
    const paths = Array.isArray(files) ? files : [files];
    api.sendFiles(chatId, paths).catch((e) => notice(errorText(e)));
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
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
    if (e.key === "Enter" && (plain || (e.metaKey && !e.shiftKey))) {
      e.preventDefault();
      send();
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
        <button
          title="สติกเกอร์"
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
