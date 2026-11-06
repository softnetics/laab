// LINE Calendar dates. Timed events read in the Mac's zone; all-day events
// read in the zone they were made in, so a Bangkok all-day event stays on its
// day for someone in Tokyo. On the wire an all-day event ends at midnight
// after its last day.

import type { CalendarEvent, CalendarEventInput } from "./types";

export const thaiMonths = [
  "มกราคม",
  "กุมภาพันธ์",
  "มีนาคม",
  "เมษายน",
  "พฤษภาคม",
  "มิถุนายน",
  "กรกฎาคม",
  "สิงหาคม",
  "กันยายน",
  "ตุลาคม",
  "พฤศจิกายน",
  "ธันวาคม",
];
const shortMonths = ["ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค."];
const weekdayShort = ["อา.", "จ.", "อ.", "พ.", "พฤ.", "ศ.", "ส."];
const weekdayLong = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"];
export { shortMonths };
/** The calendar window's chat id for every calendar at once (the rail's button). */
export const ALL_CALENDARS = "all";

/** A calendar's name: the chat's, or the user's own calendar. */
export const calendarName = (chatId: string, chatName?: string, lineName?: string) =>
  chatName || lineName || (chatId.startsWith("cal:") ? "ปฏิทินของฉัน" : "แชท");

export const weekdayLetters = ["อา", "จ", "อ", "พ", "พฤ", "ศ", "ส"];

const DAY = 86_400_000;
const pad = (n: number) => String(n).padStart(2, "0");

export const localZone = () => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

interface Parts {
  y: number;
  /** 0-based, as in Date. */
  m: number;
  d: number;
  h: number;
  min: number;
}

const formatters = new Map<string, Intl.DateTimeFormat>();

/** Wall-clock parts of `ms` in `zone`, or in the Mac's zone. */
export function partsIn(ms: number, zone?: string): Parts {
  if (zone) {
    try {
      let f = formatters.get(zone);
      if (!f) {
        f = new Intl.DateTimeFormat("en-US", {
          timeZone: zone,
          hourCycle: "h23",
          year: "numeric",
          month: "numeric",
          day: "numeric",
          hour: "numeric",
          minute: "numeric",
        });
        formatters.set(zone, f);
      }
      const p = Object.fromEntries(f.formatToParts(ms).map((x) => [x.type, Number(x.value)]));
      return { y: p.year, m: p.month - 1, d: p.day, h: p.hour % 24, min: p.minute };
    } catch {
      // An unknown zone name: fall through to the Mac's zone.
    }
  }
  const t = new Date(ms);
  return { y: t.getFullYear(), m: t.getMonth(), d: t.getDate(), h: t.getHours(), min: t.getMinutes() };
}

/** "2026-11-04", a day as a string key (and an `<input type=date>` value). */
export const dayKey = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`;
const keyOf = (p: Parts) => dayKey(p.y, p.m, p.d);

export const todayKey = () => keyOf(partsIn(Date.now()));

/** The days an event covers, as day keys, in order (at most 366). */
export function eventDays(e: Pick<CalendarEvent, "start" | "end" | "allDay" | "timezone">): string[] {
  const zone = e.allDay ? e.timezone : undefined;
  const first = partsIn(e.start, zone);
  // The end is exclusive: an event ending at midnight doesn't touch that day.
  const last = partsIn(Math.max(e.start, e.end - 1), zone);
  const out: string[] = [];
  const cur = new Date(Date.UTC(first.y, first.m, first.d));
  const stop = Date.UTC(last.y, last.m, last.d);
  while (cur.getTime() <= stop && out.length < 366) {
    out.push(dayKey(cur.getUTCFullYear(), cur.getUTCMonth(), cur.getUTCDate()));
    cur.setUTCDate(cur.getUTCDate() + 1);
  }
  return out;
}

/** The day an event card's link points at: LINE's `calendar/list` links
 *  carry the event's start (seconds), its zone and whether it's all-day.
 *  Null for a link without one, such as the card's "LINE Calendar" footer. */
export function linkDay(url: string): string | null {
  let q: URLSearchParams;
  try {
    q = new URL(url).searchParams;
  } catch {
    return null;
  }
  const start = Number(q.get("startTime"));
  if (!q.get("startTime") || !Number.isFinite(start)) return null;
  const zone = q.get("allDay") === "true" ? (q.get("timeZone") ?? undefined) : undefined;
  return keyOf(partsIn(start * 1000, zone));
}

/** Events by the days they cover. */
export function byDay(events: CalendarEvent[]): Map<string, CalendarEvent[]> {
  const map = new Map<string, CalendarEvent[]>();
  for (const e of events) {
    for (const k of eventDays(e)) {
      const list = map.get(k);
      if (list) list.push(e);
      else map.set(k, [e]);
    }
  }
  return map;
}

export interface Cell {
  key: string;
  day: number;
  inMonth: boolean;
}

/** Six weeks from the Sunday on or before the 1st, as LINE draws a month. */
export function monthGrid(year: number, month: number): Cell[] {
  const first = new Date(Date.UTC(year, month, 1));
  const start = new Date(first);
  start.setUTCDate(1 - first.getUTCDay());
  return Array.from({ length: 42 }, (_, i) => {
    const t = new Date(start);
    t.setUTCDate(start.getUTCDate() + i);
    return { key: dayKey(t.getUTCFullYear(), t.getUTCMonth(), t.getUTCDate()), day: t.getUTCDate(), inMonth: t.getUTCMonth() === month };
  });
}

const clockOf = (p: Parts) => `${p.h}.${pad(p.min)}`;

/** "พ. 4 พ.ย." for a day key; with the year when it isn't this year. */
export function dayLabel(key: string, thisYear = new Date().getFullYear()): string {
  const [y, m, d] = key.split("-").map(Number);
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  return `${weekdayShort[wd]} ${d} ${shortMonths[m - 1]}${y === thisYear ? "" : ` ${y}`}`;
}

/** "วันพุธที่ 4 พฤศจิกายน", with the year when it isn't this year. */
export function longDayLabel(key: string, thisYear = new Date().getFullYear()): string {
  const [y, m, d] = key.split("-").map(Number);
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  return `วัน${weekdayLong[wd]}ที่ ${d} ${thaiMonths[m - 1]}${y === thisYear ? "" : ` ${y}`}`;
}

/** The agenda's time column on `day`: start over end, "ทั้งวัน" for an
 *  all-day event, and arrows for a timed event that runs past the day. */
export function agendaTimes(e: CalendarEvent, day: string): [string, string] {
  if (e.allDay) return ["ทั้งวัน", ""];
  const days = eventDays(e);
  const first = days[0] === day;
  const last = days[days.length - 1] === day;
  if (!first && !last) return ["ทั้งวัน", ""];
  const a = clockOf(partsIn(e.start));
  const b = clockOf(partsIn(e.end));
  if (first && last) return [a, e.end > e.start ? b : ""];
  return first ? [a, "→"] : ["→", b];
}

/** The time line under an event's title: "ทั้งวัน", "15.00 – 16.30 น.",
 *  or the dates when it spans days. */
export function timeLabel(e: CalendarEvent, thisYear = new Date().getFullYear()): string {
  const days = eventDays(e);
  if (e.allDay) {
    return days.length <= 1 ? "ทั้งวัน" : `${dayLabel(days[0], thisYear)} – ${dayLabel(days[days.length - 1], thisYear)}`;
  }
  const a = partsIn(e.start);
  const b = partsIn(e.end);
  if (days.length <= 1) return e.end > e.start ? `${clockOf(a)} – ${clockOf(b)} น.` : `${clockOf(a)} น.`;
  return `${dayLabel(keyOf(a), thisYear)} ${clockOf(a)} – ${dayLabel(keyOf(b), thisYear)} ${clockOf(b)} น.`;
}

/** What the event form edits: dates and times as `<input>` values. */
export interface EventForm {
  title: string;
  allDay: boolean;
  startDate: string;
  startTime: string;
  endDate: string;
  endTime: string;
  location: string;
  url: string;
  description: string;
}

/** A new event on `day`: the next whole hour today, else 9.00, for an hour. */
export function newForm(day: string, now = new Date()): EventForm {
  const today = dayKey(now.getFullYear(), now.getMonth(), now.getDate());
  const h = day === today ? Math.min(now.getHours() + 1, 23) : 9;
  return {
    title: "",
    allDay: false,
    startDate: day,
    startTime: `${pad(h)}:00`,
    endDate: day,
    endTime: h === 23 ? "23:59" : `${pad(h + 1)}:00`,
    location: "",
    url: "",
    description: "",
  };
}

export function formOf(e: CalendarEvent): EventForm {
  const zone = e.allDay ? e.timezone : undefined;
  const a = partsIn(e.start, zone);
  const b = partsIn(e.allDay ? Math.max(e.start, e.end - 1) : e.end, zone);
  return {
    title: e.title,
    allDay: e.allDay,
    startDate: keyOf(a),
    startTime: `${pad(a.h)}:${pad(a.min)}`,
    endDate: keyOf(b),
    endTime: `${pad(b.h)}:${pad(b.min)}`,
    location: e.location,
    url: e.url,
    description: e.description,
  };
}

/** Local wall time to epoch ms. */
function local(date: string, time = "00:00"): number {
  const [y, m, d] = date.split("-").map(Number);
  const [h, min] = time.split(":").map(Number);
  return new Date(y, m - 1, d, h || 0, min || 0).getTime();
}

/** Wall time `date` `time` in `zone` to epoch ms (the Mac's zone when the
 *  zone is unknown). */
export function zonedMs(date: string, time = "00:00", zone?: string): number {
  if (!zone) return local(date, time);
  const [y, m, d] = date.split("-").map(Number);
  const [h, min] = time.split(":").map(Number);
  const want = Date.UTC(y, m - 1, d, h || 0, min || 0);
  // Guess, see what wall time that is in the zone, and correct by the
  // difference; twice, for a guess that lands across a DST change.
  let t = want;
  for (let i = 0; i < 2; i++) {
    const p = partsIn(t, zone);
    t += want - Date.UTC(p.y, p.m, p.d, p.h, p.min);
  }
  return t;
}

/** The form as what LINE stores, or why it can't be saved. `zone` is the
 *  event's own zone (an edit keeps it); a new event takes the Mac's. Timed
 *  events are entered in the Mac's time, all-day ones as dates in `zone`. */
export function inputOf(f: EventForm, zone = localZone()): CalendarEventInput | { error: string } {
  if (!f.title.trim()) return { error: "ใส่ชื่อนัดหมายก่อน" };
  if (!f.startDate || !f.endDate) return { error: "เลือกวันที่ก่อน" };
  let start: number;
  let end: number;
  if (f.allDay) {
    start = zonedMs(f.startDate, "00:00", zone);
    const [y, m, d] = f.endDate.split("-").map(Number);
    const next = new Date(Date.UTC(y, m - 1, d + 1));
    end = zonedMs(dayKey(next.getUTCFullYear(), next.getUTCMonth(), next.getUTCDate()), "00:00", zone);
  } else {
    start = local(f.startDate, f.startTime);
    end = local(f.endDate, f.endTime);
  }
  if (!(end >= start) || (f.allDay && end - start < DAY / 2)) return { error: "เวลาสิ้นสุดต้องไม่ก่อนเวลาเริ่ม" };
  return {
    title: f.title,
    start,
    end,
    allDay: f.allDay,
    timezone: zone,
    location: f.location,
    url: f.url,
    description: f.description,
  };
}

/** What someone types in a time field, as "HH:MM", or null if it isn't a
 *  time: "9", "930", "0930", "9.30", "9:30", "21.15 น.", "9.30pm", "บ่าย 2". */
export function parseTime(text: string): string | null {
  let s = text.trim().toLowerCase().replace(/\s+/g, "").replace(/น\.?$/, "");
  let afternoon = false;
  let morning = false;
  if (/^บ่าย/.test(s)) {
    afternoon = true;
    s = s.replace(/^บ่าย(โมง)?/, "");
  }
  const suffix = s.match(/(am|pm|a|p)$/);
  if (suffix) {
    if (suffix[1].startsWith("p")) afternoon = true;
    else morning = true;
    s = s.slice(0, -suffix[1].length);
  }
  let h: number;
  let m: number;
  const sep = s.match(/^(\d{1,2})[.:;,](\d{1,2})$/);
  if (sep) {
    h = Number(sep[1]);
    // "9.3" means 9.30, as on a clock.
    m = Number(sep[2].length === 1 ? `${sep[2]}0` : sep[2]);
  } else if (/^\d{1,4}$/.test(s)) {
    h = Number(s.length <= 2 ? s : s.slice(0, -2));
    m = s.length <= 2 ? 0 : Number(s.slice(-2));
  } else return null;
  if (afternoon && h < 12) h += 12;
  if (morning && h === 12) h = 0;
  if (h > 23 || m > 59) return null;
  return `${pad(h)}:${pad(m)}`;
}

/** Minutes between two "HH:MM" times on the same day. */
export const minutesBetween = (a: string, b: string) => {
  const [ah, am] = a.split(":").map(Number);
  const [bh, bm] = b.split(":").map(Number);
  return bh * 60 + bm - (ah * 60 + am);
};

/** "30 นาที", "1 ชม.", "1.5 ชม.", "2 ชม. 10 นาที". */
export function durationLabel(minutes: number): string {
  if (minutes < 60) return `${minutes} นาที`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (m === 0) return `${h} ชม.`;
  if (m === 30) return `${h}.5 ชม.`;
  return `${h} ชม. ${m} นาที`;
}
