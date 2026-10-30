import type { Message } from "./types";
import { sameDay } from "./time";
import { noticeFor } from "./notices";

export interface Row {
  message: Message;
  /** First message of a day: show the date chip. */
  dayStart: boolean;
  /** First message of a group: avatar and name (or the bubble tail). */
  runStart: boolean;
  /** Last message of a group: time and read mark. */
  runEnd: boolean;
  notice: boolean;
}

// LINE for Mac groups messages by sender and minute: one avatar at the
// start, one time at the end. Photos, videos and calls always stand alone.
// (Checked against LINE for Mac 2026-10-22.)
const STANDALONE = new Set(["IMAGE", "VIDEO", "CALL"]);
const minute = (t: number) => Math.floor((t > 1e11 ? t : t * 1000) / 60_000);

function sameGroup(prev: Message, m: Message): boolean {
  return (
    prev.mine === m.mine &&
    (m.mine || prev.senderId === m.senderId) &&
    minute(prev.createdTime) === minute(m.createdTime) &&
    !STANDALONE.has(prev.contentType) &&
    !STANDALONE.has(m.contentType)
  );
}

/** Silent unsends vanish; everything else is grouped. */
export function buildRows(messages: Message[], name: (mid: string) => string): Row[] {
  const rows: Row[] = [];
  let prev: Message | null = null;
  let prevNotice = false;
  for (const m of messages) {
    if (m.unsent === 2) continue;
    const notice = noticeFor(m, name) != null;
    const dayStart = !prev || !sameDay(prev.createdTime, m.createdTime);
    const runStart = !prev || dayStart || prevNotice || notice || !sameGroup(prev, m);
    if (runStart && rows.length) rows[rows.length - 1].runEnd = true;
    rows.push({ message: m, dayStart, runStart, runEnd: false, notice });
    prev = m;
    prevNotice = notice;
  }
  if (rows.length) rows[rows.length - 1].runEnd = true;
  return rows;
}
