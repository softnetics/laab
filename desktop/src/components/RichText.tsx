import { Fragment, type ReactNode } from "react";
import { liffId, linkify } from "../lib/links";
import type { MentionSpan } from "../lib/types";
import { openUrl } from "../lib/open";
import { splitLineEmoji } from "../lib/lineEmoji";

type Piece = { start: number; end: number; mention?: MentionSpan };
type EmojiResource = { start: number; end: number; url: string };

/** Text with mentions, links and in-chat search highlights. */
export function RichText({
  text,
  mentions,
  mine,
  highlight,
  current,
  onMention,
  chatId,
  metadata,
}: {
  text: string;
  mentions: MentionSpan[];
  mine: boolean;
  highlight?: [number, number][];
  current?: boolean;
  onMention?: (m: MentionSpan, el: HTMLElement) => void;
  chatId: string;
  metadata?: Record<string, string>;
}) {
  const spans = mentions.filter((m) => m.start >= 0 && m.end <= text.length).sort((a, b) => a.start - b.start);
  const pieces: Piece[] = [];
  let at = 0;
  for (const m of spans) {
    if (m.start < at) continue;
    if (m.start > at) pieces.push({ start: at, end: m.start });
    pieces.push({ start: m.start, end: m.end, mention: m });
    at = m.end;
  }
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
  const emojiResources = (() => {
    try {
      const resources = JSON.parse(metadata?.REPLACE ?? "{}").sticon?.resources;
      if (!Array.isArray(resources)) return [] as EmojiResource[];
      return resources.flatMap((r: any) => {
        const start = Number(r.S), end = Number(r.E);
        if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start || !r.productId || !r.sticonId) return [];
        return [{ start, end, url: `https://stickershop.line-scdn.net/sticonshop/v1/sticon/${encodeURIComponent(r.productId)}/android/${encodeURIComponent(r.sticonId)}.png` }];
      });
    } catch { return [] as EmojiResource[]; }
  })();
  const textualWholeEmoji = splitLineEmoji(text);
  const singleEmoji = !mentions.length && ((emojiResources.length === 1 && emojiResources[0].start === 0 && emojiResources[0].end === text.length) || (textualWholeEmoji.length === 1 && !!textualWholeEmoji[0].emoji));
  const emojiText = (value: string, keyPrefix: string, base: number): ReactNode[] => {
    const ranges = emojiResources.filter((r) => r.end > base && r.start < base + value.length);
    if (!ranges.length) return splitLineEmoji(value).map((part, i) =>
      part.emoji ? <span key={`${keyPrefix}e${i}`} role="img" aria-label={part.text} title={part.text} className={singleEmoji ? "inline-block text-[96px] leading-none" : "inline-block text-[1.15em] align-[-0.12em]"}>{part.emoji}</span> : <Fragment key={`${keyPrefix}t${i}`}>{part.text}</Fragment>,
    );
    const out: ReactNode[] = [];
    let at = 0;
    ranges.forEach((r, i) => {
      const a = Math.max(0, r.start - base), b = Math.min(value.length, r.end - base);
      if (a > at) out.push(<Fragment key={`${keyPrefix}t${at}`}>{value.slice(at, a)}</Fragment>);
      out.push(<img key={`${keyPrefix}e${i}`} src={r.url} alt={value.slice(a, b)} className={singleEmoji ? "inline-block max-h-[140px] max-w-[140px] object-contain" : "inline-block h-[1.35em] w-[1.35em] align-[-0.2em]"} />);
      at = b;
    });
    if (at < value.length) out.push(<Fragment key={`${keyPrefix}t${at}`}>{value.slice(at)}</Fragment>);
    return out;
  };
  return (
    <>
      {pieces.map((p) => {
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
              const body = marks(s, s + seg.text.length, (x) => emojiText(x, `${s}-`, s));
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
                    openUrl(url, liffId(url) ? chatId : null);
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
