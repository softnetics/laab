import { Fragment, type ReactNode } from "react";
import { mediaUrl } from "../lib/api";
import { linkify } from "../lib/links";
import type { Sticon } from "../lib/media";
import type { MentionSpan } from "../lib/types";
import { openUrl } from "../lib/open";

type Piece = { start: number; end: number; mention?: MentionSpan; sticon?: Sticon };

/** Text with mentions, links and in-chat search highlights. */
export function RichText({
  text,
  mentions,
  mine,
  highlight,
  current,
  onMention,
  chatId,
  sticons = [],
}: {
  text: string;
  mentions: MentionSpan[];
  mine: boolean;
  highlight?: [number, number][];
  current?: boolean;
  onMention?: (m: MentionSpan, el: HTMLElement) => void;
  chatId: string;
  /** LINE emoji drawn over the text they replace. */
  sticons?: Sticon[];
}) {
  const spans: Piece[] = [
    ...mentions.filter((m) => m.start >= 0 && m.end <= text.length).map((m) => ({ start: m.start, end: m.end, mention: m })),
    ...sticons.filter((s) => s.end <= text.length).map((s) => ({ start: s.start, end: s.end, sticon: s })),
  ].sort((a, b) => a.start - b.start);
  const pieces: Piece[] = [];
  let at = 0;
  for (const p of spans) {
    if (p.start < at) continue;
    if (p.start > at) pieces.push({ start: at, end: p.start });
    pieces.push(p);
    at = p.end;
  }
  // A message of emoji alone shows them big, as LINE does.
  const onlySticons = sticons.length > 0 && pieces.every((p) => p.sticon || !text.slice(p.start, p.end).trim());
  const size = onlySticons && sticons.length <= 3 ? 48 : 20;
  if (at < text.length) pieces.push({ start: at, end: text.length });

  const marks = (start: number, end: number, node: (s: string) => ReactNode): ReactNode[] => {
    // Split [start,end) by highlight ranges.
    const cuts = (highlight ?? []).filter(([a, b]) => b > start && a < end);
    if (!cuts.length) return [node(text.slice(start, end))];
    const out: ReactNode[] = [];
    let i = start;
    for (const [a0, b0] of cuts) {
      const a = Math.max(a0, start);
      const b = Math.min(b0, end);
      if (a > i) out.push(node(text.slice(i, a)));
      out.push(
        // LINE for Mac: every match blue with white text, the current one yellow.
        <mark key={`h${a}`} style={{ background: current ? "#F8F850" : "#6890E8", color: current ? "#111111" : "#FFFFFF" }}>
          {text.slice(a, b)}
        </mark>,
      );
      i = b;
    }
    if (i < end) out.push(node(text.slice(i, end)));
    return out;
  };

  const linkColor = mine ? "#0B4F8A" : "var(--color-mention)";
  return (
    <>
      {pieces.map((p) => {
        if (p.sticon) {
          const s = p.sticon;
          return (
            <img
              key={`s${p.start}`}
              src={mediaUrl.sticon(s.productId, s.sticonId, s.version, s.animated)}
              alt={text.slice(p.start, p.end)}
              title={text.slice(p.start, p.end)}
              draggable={false}
              className="inline-block align-text-bottom"
              style={{ width: size, height: size }}
              onError={(e) => {
                // An animation that isn't there: the still.
                const still = mediaUrl.sticon(s.productId, s.sticonId, s.version);
                if (e.currentTarget.src !== still) e.currentTarget.src = still;
              }}
            />
          );
        }
        if (p.mention) {
          const m = p.mention;
          return (
            <span
              key={`m${p.start}`}
              className="cursor-pointer"
              style={{ color: mine ? "#0B3D91" : "var(--color-mention)" }}
              onClick={(e) => onMention?.(m, e.currentTarget)}
            >
              {marks(p.start, p.end, (s) => s)}
            </span>
          );
        }
        let offset = p.start;
        return (
          <Fragment key={`t${p.start}`}>
            {linkify(text.slice(p.start, p.end)).map((seg) => {
              const s = offset;
              offset += seg.text.length;
              const body = marks(s, s + seg.text.length, (x) => x);
              if (!seg.url) return <Fragment key={s}>{body}</Fragment>;
              const url = seg.url;
              return (
                <a
                  key={s}
                  href={url}
                  className="underline"
                  style={{ color: linkColor }}
                  onClick={(e) => {
                    e.preventDefault();
                    openUrl(url, chatId);
                  }}
                >
                  {body}
                </a>
              );
            })}
          </Fragment>
        );
      })}
    </>
  );
}
