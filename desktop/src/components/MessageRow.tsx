import { Menu } from "@base-ui/react/menu";
import { useQuery } from "@tanstack/react-query";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import { memo, useState } from "react";
import { api, errorText, mediaUrl, queryClient } from "../lib/api";
import { performFlexAction } from "../lib/flexActions";
import { callBubbleLabel, isBlank, noticeFor, readCall } from "../lib/notices";
import { nextReaction } from "../lib/reactions";
import { clock } from "../lib/time";
import type { MentionSpan, Message, ReactionName } from "../lib/types";
import { notice, setUi, ui, useUi } from "../lib/ui";
import { openUrl } from "../lib/open";
import { viewMessage } from "../lib/viewer";
import { Avatar } from "./Avatar";
import { FlexCard, parseFlex } from "./FlexCard";
import { DocIcon, PhoneIcon, PhotoIcon, PlayIcon, VideoIcon } from "./Icons";
import { Ctx, itemClass, pressable, Separator, Spinner } from "./kit";
import { Popover } from "./Popover";
import { ReactButton, ReactionBadge } from "./Reactions";
import { ReadMark, type Reader } from "./ReadMark";
import { RichImage } from "./RichImage";
import { RichText } from "./RichText";

export interface RowContext {
  chatId: string;
  group: boolean;
  me: string;
  signedIn: boolean;
  name: (mid: string) => string;
  picture: (mid: string) => string | null;
  loadMembers: () => void;
  /** A loaded message by id, for reply quotes. */
  find: (id: string) => Message | undefined;
}

const TAIL = 8;

/** LINE's "horn" tail at the bubble's top outer corner. */
function Tail({ mine, color }: { mine: boolean; color: string }) {
  return (
    // Matched pixel by pixel to LINE for Mac: a horn leaving the rounded
    // corner about 3pt down, curling up at its tip 8pt out, its underside
    // meeting the edge about 14pt down. x = 8 is the bubble's edge.
    <svg
      width={TAIL + 10}
      height={14}
      viewBox="0 0 18 14"
      className="pointer-events-none absolute top-0"
      style={mine ? { right: -TAIL, transform: "scaleX(-1)" } : { left: -TAIL }}
    >
      <path d="M0 2.6 Q5 5.4 16.5 4.8 L16.5 14 L8 14 Q1.5 11.5 0 2.6 Z" fill={color} />
    </svg>
  );
}

function Bubble({ mine, tail, children, className = "" }: { mine: boolean; tail: boolean; children: React.ReactNode; className?: string }) {
  const color = mine ? "var(--color-outgoing)" : "var(--color-incoming)";
  return (
    <div
      className={`selectable relative max-w-[420px] rounded-[16.5px] px-[13px] py-[7px] text-[13.5px] leading-[1.4] break-words whitespace-pre-wrap ${className}`}
      style={{ background: color, color: mine ? "var(--color-outgoing-text)" : "var(--color-primary)" }}
    >
      {tail && <Tail mine={mine} color={color} />}
      {children}
    </div>
  );
}

function MentionCard({ span, ctx, anchor, onClose }: { span: MentionSpan; ctx: RowContext; anchor: HTMLElement; onClose: () => void }) {
  const name = span.all ? "ทั้งหมด" : span.mid ? ctx.name(span.mid) : "สมาชิก";
  return (
    <Popover
      open
      onOpenChange={(o) => !o && onClose()}
      side="top"
      className="flex w-[196px] flex-col items-center gap-2 p-4"
      anchor={anchor}
    >
      <Avatar id={span.mid ?? "all"} name={name} picture={span.mid ? ctx.picture(span.mid) : null} size={84} all={span.all} />
      <div className="text-center text-[14px] font-semibold">{name}</div>
    </Popover>
  );
}

/** Download (once) and open or reveal an attachment. */
function useOpener(m: Message, ctx: RowContext) {
  const [state, setState] = useState<"idle" | "loading" | string>("idle");
  const open = async (reveal = false) => {
    setState("loading");
    try {
      const path = await api.download(ctx.chatId, m.id, m.fileName);
      setState("idle");
      await (reveal ? api.revealPath(path) : api.openPath(path));
    } catch (e) {
      setState(errorText(e));
    }
  };
  return { state, open };
}

function Attachment({ m, ctx }: { m: Message; ctx: RowContext }) {
  return m.contentType === "FILE" ? <FileBubble m={m} ctx={ctx} /> : <MediaBubble m={m} ctx={ctx} />;
}

function FileBubble({ m, ctx }: { m: Message; ctx: RowContext }) {
  const { state, open } = useOpener(m, ctx);
  const caption = state === "loading" ? "กำลังดาวน์โหลด…" : state !== "idle" ? state : m.filePath ? "คลิกเพื่อเปิด" : "คลิกเพื่อดาวน์โหลด";
  return (
    <div className="flex w-[260px] flex-col gap-2 rounded-2xl bg-incoming p-3" {...pressable(() => open())}>
      <div className="flex items-center gap-3">
        <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-chat text-secondary">
          {state === "loading" ? <Spinner size={16} /> : <DocIcon size={22} />}
        </div>
        <div className="min-w-0">
          <div className="line-clamp-2 text-[14px] font-medium break-all">{m.fileName ?? "ไฟล์"}</div>
          <div className="text-[11.5px] text-secondary">{caption}</div>
        </div>
      </div>
      {state === "loading" && (
        <div className="h-1 w-40 overflow-hidden rounded bg-field">
          <div className="h-full w-1/2 animate-pulse bg-line-green" />
        </div>
      )}
      {m.filePath && (
        <button className="self-start rounded-full bg-chat px-3 py-1 text-[12px]" onClick={(e) => (e.stopPropagation(), open(true))}>
          แสดงใน Finder
        </button>
      )}
    </div>
  );
}

function MediaBubble({ m, ctx }: { m: Message; ctx: RowContext }) {
  const [failed, setFailed] = useState(false);
  const video = m.contentType === "VIDEO";
  // A downloaded video's imagePath is the video itself, not a picture.
  const src = m.imagePath && !video ? mediaUrl.local(m.imagePath) : mediaUrl.message(ctx.chatId, m.id, true);
  const run = (p: Promise<unknown>, done: (r: unknown) => string) => p.then((r) => notice(done(r))).catch((e) => notice(errorText(e)));
  const saveAs = async () => {
    const dest = await save({ defaultPath: m.fileName ?? (video ? `LINE_${m.id}.mp4` : `LINE_${m.id}.jpg`) });
    if (dest) run(api.saveAttachment(ctx.chatId, m.id, dest), () => "บันทึกแล้ว");
  };
  const link = "hover:text-primary";
  return (
    <div className={`flex flex-col ${m.mine ? "items-end" : "items-start"}`}>
      {/* LINE for Mac: square corners, and links under every photo/video. */}
      <div
        className="relative cursor-pointer overflow-hidden rounded-[2px]"
        aria-label={video ? "เปิดวิดีโอ" : "เปิดรูป"}
        {...pressable(() => viewMessage(ctx.chatId, m))}
      >
        {failed ? (
          <div className="flex h-[150px] w-[200px] flex-col items-center justify-center gap-1 bg-incoming text-secondary">
            {video ? <VideoIcon size={28} /> : <PhotoIcon size={28} />}
            <span className="text-[12px]">{video ? "วิดีโอ" : "รูปภาพ"}</span>
          </div>
        ) : (
          <img
            src={src}
            onError={() => setFailed(true)}
            draggable={false}
            className="block max-h-[280px] max-w-[260px] object-cover"
            alt=""
          />
        )}
        {video && (
          <div className="absolute inset-0 grid place-items-center">
            <span className="grid h-11 w-11 place-items-center rounded-full bg-black/45 text-white">
              <PlayIcon size={20} />
            </span>
          </div>
        )}
      </div>
      {
        <div className="mt-1 flex gap-1.5 text-[11.5px] text-secondary">
          <button className={link} onClick={() => run(api.saveAttachment(ctx.chatId, m.id), (p) => `บันทึกไว้ที่ ${p}`)}>
            บันทึก
          </button>
          <span>|</span>
          <button className={link} onClick={saveAs}>
            บันทึกเป็น…
          </button>
          {ctx.chatId !== ctx.me && (
            <>
              <span>|</span>
              <button className={link} onClick={() => run(api.saveToKeep(ctx.chatId, m.id), () => "ส่งไปยัง Keep Memo แล้ว")}>
                ส่งไปยัง Keep Memo
              </button>
            </>
          )}
        </div>
      }
    </div>
  );
}

/** The quoted original above a reply, as LINE shows it. */
function ReplyQuote({ id, ctx, mine }: { id: string; ctx: RowContext; mine: boolean }) {
  const o = ctx.find(id);
  const line = mine ? "border-black/15" : "border-secondary/30";
  if (!o || o.unsent) {
    return <div className={`mb-1.5 border-b pb-1.5 text-[12.5px] opacity-70 ${line}`}>(ไม่สามารถโหลดข้อความต้นฉบับได้)</div>;
  }
  const who = o.mine ? ctx.name(ctx.me) : o.senderId ? ctx.name(o.senderId) : "สมาชิก";
  const thumb =
    o.contentType === "IMAGE" || o.contentType === "VIDEO"
      ? o.imagePath && o.contentType === "IMAGE"
        ? mediaUrl.local(o.imagePath)
        : mediaUrl.message(ctx.chatId, o.id, true)
      : o.contentType === "STICKER" && o.stickerId
        ? mediaUrl.sticker(o.stickerId, false)
        : null;
  const text =
    o.contentType === "IMAGE"
      ? "รูปภาพ"
      : o.contentType === "VIDEO"
        ? "วิดีโอ"
        : o.contentType === "STICKER"
          ? "สติกเกอร์"
          : o.contentType === "FILE"
            ? (o.fileName ?? "ไฟล์")
            : o.contentType === "CONTACT"
              ? `ผู้ติดต่อ: ${o.metadata.displayName ?? ""}`
              : o.text;
  return (
    <button
      onClick={() => setUi({ focusMessageId: o.id })}
      className={`mb-1.5 flex w-full items-center gap-2 border-b pb-1.5 text-left ${line}`}
    >
      <Avatar id={o.mine ? ctx.me : (o.senderId ?? "?")} name={who} picture={ctx.picture(o.mine ? ctx.me : (o.senderId ?? ""))} size={20} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12.5px] font-semibold">{who}</div>
        {!thumb && <div className="truncate text-[12.5px] opacity-80">{text}</div>}
      </div>
      {thumb && <img src={thumb} alt="" className="h-9 w-9 rounded-md object-cover" draggable={false} />}
    </button>
  );
}

/** A group note/post notice ("ดูโน้ต"). */
function PostNotice({ m, tail, chatId }: { m: Message; tail: boolean; chatId: string }) {
  const url = m.metadata.postEndUrl;
  // "ดูโน้ต" opens the note in LAAB's notes window, as LINE opens its own.
  const postId = url ? new URL(url.replace(/^line:\/\//, "https://x/")).searchParams.get("postId") : null;
  const view = () =>
    postId ? api.openAuxWindow("albums", chatId, "", "notes", postId).catch((e) => notice(errorText(e))) : url && openUrl(url);
  return (
    <Bubble mine={m.mine} tail={tail} className="min-w-[220px] px-0! py-0!">
      <div className="px-[13px] pt-[6px] pb-[5px]">{m.metadata.text || "โพสต์ในกลุ่ม"}</div>
      {url && (
        <button
          onClick={view}
          className={`flex w-full items-center justify-between border-t px-[13px] py-[5px] text-left ${m.mine ? "border-black/15" : "border-secondary/30"}`}
        >
          ดูโน้ต <span className="opacity-60">›</span>
        </button>
      )}
    </Bubble>
  );
}

/** A shared contact: LINE's card with picture, name and a chevron. Opens
 *  the profile sheet. */
function ContactCard({ m }: { m: Message }) {
  const mid = m.metadata.mid ?? "";
  const { data } = useQuery({
    queryKey: ["profile", mid],
    queryFn: () => api.contactProfile(mid),
    enabled: !!mid,
    staleTime: 60_000,
  });
  const name = data?.name || m.metadata.displayName || "ผู้ติดต่อ";
  const picture = data?.picture ?? null;
  return (
    <div
      className="flex max-w-[300px] items-center gap-2.5 rounded-[16.5px] bg-incoming py-2.5 pr-3.5 pl-2.5 text-primary"
      {...pressable(() => mid && setUi({ profile: { mid, name, picture } }))}
    >
      <Avatar id={mid || m.id} name={name} picture={picture} size={40} />
      <span className="min-w-0 truncate text-[14px]">{name}</span>
      <span className="ml-2 text-[16px] text-secondary">›</span>
    </div>
  );
}

function Sticker({ m }: { m: Message }) {
  const [stage, setStage] = useState<"anim" | "still" | "none">("anim");
  if (!m.stickerId || stage === "none") return <div className="px-2 py-6 text-[13px] text-secondary">สติกเกอร์</div>;
  return (
    <img
      src={stage === "anim" ? mediaUrl.sticker(m.stickerId, true) : mediaUrl.sticker(m.stickerId, false)}
      onError={() => setStage(stage === "anim" ? "still" : "none")}
      draggable={false}
      className="max-h-[140px] max-w-[140px] object-contain"
      alt="สติกเกอร์"
    />
  );
}

function Content({
  m,
  ctx,
  tail,
  highlight,
  current,
}: {
  m: Message;
  ctx: RowContext;
  tail: boolean;
  highlight?: [number, number][];
  current?: boolean;
}) {
  const [mention, setMention] = useState<{ span: MentionSpan; el: HTMLElement } | null>(null);
  switch (m.contentType) {
    case "STICKER":
      return <Sticker m={m} />;
    case "IMAGE":
    case "VIDEO":
    case "FILE":
      return <Attachment m={m} ctx={ctx} />;
    case "CALL": {
      const call = readCall(m)!;
      return (
        <Bubble mine={m.mine} tail={tail} className="flex flex-col items-center gap-1 px-[18px]! py-1.5!">
          {call.kind === "video" ? <VideoIcon size={20} /> : <PhoneIcon size={20} />}
          <span className="text-[14px]">{callBubbleLabel(call, m.mine)}</span>
        </Bubble>
      );
    }
    case "FLEX": {
      const root = parseFlex(m.metadata.FLEX_JSON);
      if (root) return <FlexCard root={root} onAction={(a) => performFlexAction(a, ctx.chatId, m.id)} />;
      return (
        <Bubble mine={m.mine} tail={tail}>
          {m.metadata.ALT_TEXT || m.text || "ข้อความ Flex"}
        </Bubble>
      );
    }
    case "RICH":
      // Image-map messages from official accounts; alt text if there is no image.
      if (m.metadata.DOWNLOAD_URL) return <RichImage meta={m.metadata} chatId={ctx.chatId} />;
      return (
        <Bubble mine={m.mine} tail={tail}>
          {m.metadata.ALT_TEXT || m.text || "ข้อความ"}
        </Bubble>
      );
    case "CONTACT":
      return <ContactCard m={m} />;
    case "POSTNOTIFICATION":
      return <PostNotice m={m} tail={tail} chatId={ctx.chatId} />;
    case "AUDIO":
      return (
        <Bubble mine={m.mine} tail={tail}>
          <audio controls preload="none" src={mediaUrl.message(ctx.chatId, m.id, false)} className="h-8 w-56" />
        </Bubble>
      );
  }
  if (isBlank(m)) {
    return (
      <Bubble mine={m.mine} tail={tail}>
        <span className="text-[13px] italic opacity-75" title="ดูข้อความนี้ใน LINE บนโทรศัพท์">
          ไม่สามารถแสดงข้อความนี้ใน LAAB
        </span>
      </Bubble>
    );
  }
  return (
    <Bubble mine={m.mine} tail={tail}>
      {m.replyTo && <ReplyQuote id={m.replyTo} ctx={ctx} mine={m.mine} />}
      <RichText
        text={m.text || (m.contentType !== "NONE" ? m.contentType.toLowerCase() : "")}
        mentions={m.mentions}
        mine={m.mine}
        highlight={highlight}
        current={current}
        chatId={ctx.chatId}
        onMention={(span, el) =>
          span.mid && !span.all
            ? setUi({ profile: { mid: span.mid, name: ctx.name(span.mid), picture: ctx.picture(span.mid) } })
            : setMention({ span, el })
        }
      />
      {mention && <MentionCard span={mention.span} ctx={ctx} anchor={mention.el} onClose={() => setMention(null)} />}
    </Bubble>
  );
}

/** LINE for Mac's message menu, in its order. Items LAAB cannot do
 *  (แชร์, รายงานปัญหา) are left out. */
function rowMenu(m: Message, dev: boolean, ctx?: RowContext) {
  const flexAlt = m.contentType === "FLEX" ? m.metadata.ALT_TEXT : null;
  const copyText =
    flexAlt ||
    (m.contentType === "CONTACT" ? (m.metadata.displayName ?? "") : ["IMAGE", "VIDEO", "STICKER", "FILE"].includes(m.contentType) ? "" : m.text);
  const text = m.contentType === "NONE" && !!m.text.trim();
  const sent = !m.id.startsWith("local-");
  const run = (p: Promise<unknown>, done?: string) => p.then(() => done && notice(done)).catch((e) => notice(errorText(e)));
  const item = (label: React.ReactNode, onClick: () => void, disabled = false) => (
    <Menu.Item className={itemClass} onClick={onClick} disabled={disabled}>
      {label}
    </Menu.Item>
  );
  if (!ctx) {
    return dev ? item("ดูข้อมูลดิบ", () => setUi({ inspectMessageId: m.id })) : null;
  }
  const live = sent && m.unsent === 0;
  return (
    <>
      {live && item("ตอบกลับ", () => setUi({ composerMode: { chatId: ctx.chatId, kind: "reply", message: m }, composerFocus: Date.now() }))}
      {item("คัดลอก", () => writeText(copyText), !copyText)}
      {m.mine &&
        live &&
        text &&
        item(
          <>
            แก้ไข <span className="h-1.5 w-1.5 rounded-full bg-line-green" />
          </>,
          () => setUi({ composerMode: { chatId: ctx.chatId, kind: "edit", message: m }, composerFocus: Date.now() }),
        )}
      {m.mine &&
        live &&
        item("ยกเลิกข้อความ", () =>
          setUi({
            confirm: {
              title: "ต้องการยกเลิกข้อความนี้หรือไม่",
              body: "คู่สนทนาจะได้รับแจ้งเตือนหากคุณยกเลิกข้อความที่คู่สนทนาอ่านแล้ว\n\nข้อความอาจไม่หายไปหากคู่สนทนาใช้งาน LINE บางเวอร์ชัน",
              action: "ตกลง",
              run: () => api.unsend(m.id),
            },
          }),
        )}
      {item("ลบ", () =>
        setUi({
          confirm: {
            title: "ต้องการลบข้อความนี้หรือไม่",
            body: "ข้อความจะถูกลบจากเครื่องนี้เท่านั้น คู่สนทนายังเห็นอยู่",
            action: "ลบ",
            run: () => api.deleteMessage(m.id),
          },
        }),
      )}
      {live && (
        <>
          <Separator />
          {text &&
            item("แปล", () =>
              api
                .translate(m.text)
                .then((t) => setUi({ translations: { ...ui.state.translations, [m.id]: t } }))
                .catch((e) => notice(errorText(e))),
            )}
          {item("ส่งไปยัง Keep Memo", () => run(api.saveToKeep(ctx.chatId, m.id), "ส่งไปยัง Keep Memo แล้ว"))}
          {ctx.group && text && item("บันทึกที่โน้ต", () => run(api.createNote(ctx.chatId, m.text), "บันทึกที่โน้ตแล้ว"))}
          {text && (
            <>
              <Separator />
              {item("ประกาศ", () =>
                run(
                  api
                    .announce(ctx.chatId, m.id, m.text)
                    .then(() => queryClient.invalidateQueries({ queryKey: ["announcements", ctx.chatId] })),
                  "ประกาศแล้ว",
                ),
              )}
            </>
          )}
        </>
      )}
      {dev && (
        <>
          <Separator />
          {item("ดูข้อมูลดิบ", () => setUi({ inspectMessageId: m.id }))}
        </>
      )}
    </>
  );
}

function NoticeLink({ link }: { link: NonNullable<ReturnType<typeof noticeFor>>["link"] & {} }) {
  if (link.action === "announcements") {
    return (
      <button className="underline hover:text-primary" onClick={() => setUi({ announcementsOpen: true })}>
        {link.text}
      </button>
    );
  }
  // LINE links this to a LINE Premium page whose address it fetches at
  // runtime, so LAAB shows the line without a destination.
  return (
    <span className="underline" title="ฟีเจอร์ของ LINE Premium">
      {link.text}
    </span>
  );
}

export const NoticeRow = memo(function NoticeRow({ m, ctx }: { m: Message; ctx: RowContext }) {
  const dev = useUi((s) => s.developerMode);
  const n = noticeFor(m, ctx.name)!;
  const link = n.link;
  const body = (
    <div className="flex justify-center pt-2.5" data-message={m.id}>
      {/* Measured from LINE: #262729 chip, #949494 12px text on 15px lines,
          10pt corners, 12pt sides; two lines are 38pt tall, a bit more
          with a time on top. */}
      <div
        className={`chip-text rounded-[10px] bg-date-chip px-3 text-center text-[12px] leading-[15px] ${n.time ? "pt-[5.5px] pb-[5.5px]" : "py-[4px]"}`}
      >
        {n.time && <div>{n.time}</div>}
        <div>
          {n.text}
          {link && !link.newLine && <NoticeLink link={link} />}
        </div>
        {link?.newLine && (
          <div>
            <NoticeLink link={link} />
          </div>
        )}
      </div>
    </div>
  );
  return dev ? <Ctx menu={rowMenu(m, dev)}>{body}</Ctx> : body;
});

export const MessageRow = memo(function MessageRow({
  m,
  ctx,
  runStart,
  runEnd,
  readers,
  highlight,
  current,
}: {
  m: Message;
  ctx: RowContext;
  runStart: boolean;
  /** Last of its group: LINE shows the time and read mark only here. */
  runEnd: boolean;
  readers: Reader[];
  highlight?: [number, number][];
  current?: boolean;
}) {
  const dev = useUi((s) => s.developerMode);
  const translation = useUi((s) => s.translations[m.id]);
  const [hover, setHover] = useState(false);
  const myReaction = m.reactions.find((r) => r.userId === ctx.me)?.reaction;
  const react = (picked: ReactionName) => {
    api.react(ctx.chatId, m.id, nextReaction(myReaction, picked)).catch((e) => notice(errorText(e)));
  };
  const tail = runStart && !["STICKER", "IMAGE", "VIDEO", "FILE", "FLEX", "RICH", "CONTACT"].includes(m.contentType);
  const [picking, setPicking] = useState(false);
  // Hovering shows the reaction button beside the read mark and time, not
  // in their place: replacing them made "อ่านแล้ว N" impossible to click.
  const showReact = (hover || picking) && ctx.signedIn;
  const meta = (
    <div className={`relative flex shrink-0 flex-col justify-end gap-px self-end pb-0.5 ${m.mine ? "items-end" : "items-start"}`}>
      {runEnd ? (
        <div className={`flex flex-col gap-px ${m.mine ? "items-end" : "items-start"}`}>
          <ReadMark direct={!ctx.group} readers={readers} onOpen={ctx.loadMembers} />
          <span className="text-[11px] text-secondary">{clock(m.createdTime)}</span>
        </div>
      ) : (
        // Mid-group: no time, but keep room for the hover reaction button.
        <div className="h-7 w-7" />
      )}
      <div
        className={`absolute bottom-0 ${
          runEnd ? (m.mine ? "right-full mr-1.5" : "left-full ml-1.5") : m.mine ? "right-0" : "left-0"
        } ${showReact ? "" : "invisible"}`}
      >
        <ReactButton mine={myReaction} onPick={react} onOpenChange={setPicking} />
      </div>
    </div>
  );
  const bubble = (
    <div className="relative">
      <Ctx menu={rowMenu(m, dev, ctx)}>
        <Content m={m} ctx={ctx} tail={tail} highlight={highlight} current={current} />
      </Ctx>
      {translation && (
        <div className={`selectable mt-1 max-w-[420px] rounded-xl bg-field px-3 py-1.5 text-[12.5px] ${m.mine ? "ml-auto" : ""}`}>
          <div className="mb-0.5 text-[11px] text-secondary">คำแปล</div>
          {translation}
        </div>
      )}
      {m.reactions.length > 0 && (
        <div className="absolute top-full right-0 z-10 mt-1">
          <ReactionBadge list={m.reactions} name={ctx.name} picture={ctx.picture} me={ctx.me} />
        </div>
      )}
    </div>
  );
  return (
    <div
      data-message={m.id}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className={`flex gap-2 ${m.mine ? "items-end" : "items-start"} ${runStart ? "pt-3.5" : "pt-1.5"} ${m.reactions.length ? "pb-7" : ""} ${m.mine ? "justify-end pl-12" : "pr-12"}`}
    >
      {m.mine ? (
        <>
          {meta}
          {bubble}
        </>
      ) : (
        <>
          <div className="w-[38px] shrink-0">
            {runStart && m.senderId && (
              <button
                aria-label={`โปรไฟล์ของ ${ctx.name(m.senderId)}`}
                className="block rounded-full"
                onClick={() => setUi({ profile: { mid: m.senderId!, name: ctx.name(m.senderId!), picture: ctx.picture(m.senderId!) } })}
              >
                <Avatar id={m.senderId} name={ctx.name(m.senderId)} picture={ctx.picture(m.senderId)} size={38} />
              </button>
            )}
          </div>
          <div className={`flex min-w-0 flex-col ${tail ? "pl-2" : ""}`}>
            {runStart && ctx.group && m.senderId && <span className="mb-1 text-[12px] text-secondary">{ctx.name(m.senderId)}</span>}
            <div className="flex items-end gap-1.5">
              {bubble}
              {meta}
            </div>
          </div>
        </>
      )}
    </div>
  );
});
