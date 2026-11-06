import type { Message } from "./types";
import { sameDay } from "./time";
import { noticeFor } from "./notices";

export interface Row {
  message: Message;
  /** First message of a day: show the date chip. */
  dayStart: boolean;
  /** First message of a group: avatar and name (or the bubble tail). */
  runStart: boolean;
  notice: boolean;
  /** Photos sent together, shown as one grid; `message` is the first. */
  photos?: Message[];
}

// LINE shows photos sent together as a grid. It marks them with a shared
// group id (`GID`, with `GSEQ` and `GTOTAL`); photos sent one by one have
// none and stand alone, however close together they come.
const joinsPhotos = (prev: Message, m: Message) =>
  m.contentType === "IMAGE" &&
  prev.contentType === "IMAGE" &&
  !m.unsent &&
  !prev.unsent &&
  !!m.metadata.GID &&
  m.metadata.GID === prev.metadata.GID;

// LINE groups messages by sender and minute under one avatar and name. Photos, videos and calls always stand alone.
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
    const last = rows[rows.length - 1];
    if (prev && last && !last.notice && joinsPhotos(prev, m)) {
      last.photos = [...(last.photos ?? [last.message]), m];
      prev = m;
      continue;
    }
    const notice = noticeFor(m, name) != null;
    const dayStart = !prev || !sameDay(prev.createdTime, m.createdTime);
    const runStart = !prev || dayStart || prevNotice || notice || !sameGroup(prev, m);
    rows.push({ message: m, dayStart, runStart, notice });
    prev = m;
    prevNotice = notice;
  }
  return rows;
}
