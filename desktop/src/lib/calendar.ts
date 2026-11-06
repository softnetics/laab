// LINE Calendar dates. Timed events read in the Mac's zone; all-day events
// read in the zone they were made in, so a Bangkok all-day event stays on its
// day for someone in Tokyo. On the wire an all-day event ends at midnight
// after its last day.

import type { CalendarEvent, CalendarEventInput, CalendarRecurrence, RepeatInput } from "./types";

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
export function inputOf(f: EventForm, zone = localZone(), reminders?: number[]): CalendarEventInput | { error: string } {
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
    ...(reminders ? { reminders } : {}),
  };
}

// -- repeats -----------------------------------------------------------------

const DOW = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];
/** Days since the epoch of a y/m(0-based)/d date. */
const dayNum = (y: number, m: number, d: number) => Math.round(Date.UTC(y, m, d) / DAY);
const dateOf = (n: number) => {
  const t = new Date(n * DAY);
  return { y: t.getUTCFullYear(), m: t.getUTCMonth(), d: t.getUTCDate() };
};
const daysInMonth = (y: number, m: number) => new Date(Date.UTC(y, m + 1, 0)).getUTCDate();

/** A BYDAY entry: its weekday (0 = Sunday) and ordinal (0 when none). */
function parseByDay(s: string): { dow: number; nth: number } | null {
  const m = s.trim().toUpperCase().match(/^([+-]?\d{1,2})?(SU|MO|TU|WE|TH|FR|SA)$/);
  return m ? { dow: DOW.indexOf(m[2]), nth: m[1] ? Number(m[1]) : 0 } : null;
}

/** The `nth` (1…5, or -1 for last) `dow` of a month, or null if there isn't one. */
function nthWeekday(y: number, m: number, dow: number, nth: number): number | null {
  const len = daysInMonth(y, m);
  if (nth > 0) {
    const first = new Date(Date.UTC(y, m, 1)).getUTCDay();
    const d = 1 + ((dow - first + 7) % 7) + (nth - 1) * 7;
    return d <= len ? d : null;
  }
  const last = new Date(Date.UTC(y, m, len)).getUTCDay();
  const d = len - ((last - dow + 7) % 7) + (nth + 1) * 7;
  return d >= 1 ? d : null;
}

/** The dates (day numbers) a rule gives in period `k` (the k-th day, week,
 *  month or year from the first), in order. */
function periodDays(r: CalendarRecurrence, first: { y: number; m: number; d: number }, k: number): number[] {
  const start = dayNum(first.y, first.m, first.d);
  const days = r.byDay.map(parseByDay).filter((x): x is { dow: number; nth: number } => !!x);
  switch (r.frequency) {
    case "daily":
      return [start + k];
    case "weekly": {
      // Weeks start on Monday (iCalendar's default WKST).
      const monday = start - ((new Date(start * DAY).getUTCDay() + 6) % 7) + k * 7;
      const dows = days.length ? days.map((x) => x.dow) : [new Date(start * DAY).getUTCDay()];
      return [...new Set(dows.map((dow) => monday + ((dow + 6) % 7)))].sort((a, b) => a - b);
    }
    case "monthly": {
      const ym = first.y * 12 + first.m + k;
      const y = Math.floor(ym / 12);
      const m = ym % 12;
      if (!days.length) return first.d <= daysInMonth(y, m) ? [dayNum(y, m, first.d)] : [];
      const out: number[] = [];
      for (const { dow, nth } of days) {
        if (nth) {
          const d = nthWeekday(y, m, dow, nth);
          if (d) out.push(dayNum(y, m, d));
        } else {
          for (let n = 1; n <= 5; n++) {
            const d = nthWeekday(y, m, dow, n);
            if (d) out.push(dayNum(y, m, d));
          }
        }
      }
      return [...new Set(out)].sort((a, b) => a - b);
    }
    case "yearly": {
      const y = first.y + k;
      return first.d <= daysInMonth(y, first.m) ? [dayNum(y, first.m, first.d)] : [];
    }
    default:
      return [];
  }
}

/** The longest period of each frequency, in days: skipping ahead by these never skips too far. */
const PERIOD_DAYS = { daily: 1, weekly: 7, monthly: 31, yearly: 366 } as const;

/** Original starts (ms) of a recurring event's occurrences that start
 *  before `to` and end after `from`, before exceptions and changes. */
export function occurrenceStarts(e: CalendarEvent, from: number, to: number, limit = 1000): number[] {
  const r = e.recurrence;
  if (!r || !(r.frequency in PERIOD_DAYS)) return [e.start];
  const zone = e.timezone || undefined;
  const first = partsIn(e.start, zone);
  const time = `${pad(first.h)}:${pad(first.min)}`;
  const length = Math.max(0, e.end - e.start);
  const step = Math.max(1, r.interval || 1);
  const per = PERIOD_DAYS[r.frequency as keyof typeof PERIOD_DAYS];
  // Skip the periods that end well before the window.
  const behind = Math.floor((from - length - e.start) / DAY / per / step) - 1;
  let k = Math.max(0, behind) * step;
  const out: number[] = [];
  const startDay = dayNum(first.y, first.m, first.d);
  for (let guard = 0; guard < 5000 && out.length < limit; guard++, k += step) {
    const days = periodDays(r, first, k);
    let past = false;
    for (const n of days) {
      if (n < startDay) continue;
      const d = dateOf(n);
      const t = zonedMs(dayKey(d.y, d.m, d.d), time, zone);
      if (t >= to || (r.until != null && t > r.until)) {
        past = true;
        break;
      }
      if (t + Math.max(length, 1) > from) out.push(t);
    }
    if (past) break;
    // A month or year with no matching day can't stop the search on its own.
    if (!days.length && k / step > 400) break;
  }
  return out;
}

/** One occurrence of `e`, by its original start: changed as LINE says, or
 *  as the series has it. */
export function occurrenceAt(e: CalendarEvent, recurId?: number): CalendarEvent {
  if (recurId == null || !e.recurrence) return e;
  const o = e.overrides?.find((x) => x.recurId === recurId && !x.deleted);
  if (o) {
    return { ...e, ...o, timezone: o.timezone || e.timezone, recurId };
  }
  return { ...e, start: recurId, end: recurId + (e.end - e.start), recurId };
}

/** The events and occurrences that touch `from`…`to` (epoch ms): a
 *  recurring event becomes one entry per occurrence, without the ones LINE
 *  says were deleted, and with changed ones as changed. */
export function expand(events: CalendarEvent[], from: number, to: number): CalendarEvent[] {
  const out: CalendarEvent[] = [];
  for (const e of events) {
    const r = e.recurrence;
    if (!r || !(r.frequency in PERIOD_DAYS)) {
      out.push(e);
      continue;
    }
    const overrides = new Map((e.overrides ?? []).map((o) => [o.recurId, o]));
    const except = new Set(r.exceptIds);
    const used = new Set<number>();
    for (const s of occurrenceStarts(e, from, to)) {
      const o = overrides.get(s);
      if (o) {
        used.add(s);
        if (!o.deleted) out.push(occurrenceAt(e, s));
      } else if (!except.has(s)) {
        out.push(occurrenceAt(e, s));
      }
    }
    // A changed occurrence moved into the window from outside it.
    for (const o of overrides.values()) {
      if (!used.has(o.recurId) && !o.deleted && o.start < to && Math.max(o.end, o.start + 1) > from) out.push(occurrenceAt(e, o.recurId));
    }
  }
  return out;
}

/** A unique key for an event or one occurrence of it. */
export const occurrenceKey = (e: CalendarEvent) => (e.recurId != null ? `${e.id}:${e.recurId}` : e.id);

const thaiDow = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"];
const ordinal: Record<number, string> = { 1: "แรก", 2: "ที่ 2", 3: "ที่ 3", 4: "ที่ 4", 5: "ที่ 5", [-1]: "สุดท้าย" };

/** "ทุกสัปดาห์ วันจันทร์ พุธ", "ทุก 2 เดือน วันศุกร์สุดท้าย", with "จนถึง …"
 *  when it ends. Empty for a one-off or a rule LAAB can't read. */
export function recurrenceLabel(e: CalendarEvent, thisYear = new Date().getFullYear()): string {
  const r = e.recurrence;
  if (!r || !(r.frequency in PERIOD_DAYS)) return "";
  const n = Math.max(1, r.interval || 1);
  const unit = { daily: "วัน", weekly: "สัปดาห์", monthly: "เดือน", yearly: "ปี" }[r.frequency as keyof typeof PERIOD_DAYS];
  let text = n === 1 ? `ทุก${unit}` : `ทุก ${n} ${unit}`;
  const days = r.byDay.map(parseByDay).filter((x): x is { dow: number; nth: number } => !!x);
  const first = partsIn(e.start, e.timezone || undefined);
  if (r.frequency === "weekly" && days.length) {
    text += ` วัน${days.map((d) => thaiDow[d.dow]).join(" ")}`;
  } else if (r.frequency === "monthly") {
    text += days.length
      ? ` ${days.map((d) => `วัน${thaiDow[d.dow]}${d.nth ? ordinal[d.nth] ?? `ที่ ${d.nth}` : ""}`).join(" ")}`
      : ` วันที่ ${first.d}`;
  } else if (r.frequency === "yearly") {
    text += ` วันที่ ${first.d} ${thaiMonths[first.m]}`;
  }
  if (r.until != null) text += ` จนถึง ${dayLabel(keyOf(partsIn(r.until, e.timezone || undefined)), thisYear)}`;
  return text;
}

/** What the form's repeat rows edit. `monthly` picks the date ("date"),
 *  the start's nth weekday ("nth") or the month's last such weekday ("last"). */
export interface RepeatForm {
  frequency: RepeatInput["frequency"];
  interval: number;
  /** Weekdays ("MO"…) of a weekly rule. */
  weekdays: string[];
  monthly: "date" | "nth" | "last";
  /** The last day it repeats on (a day key), or null for no end. */
  until: string | null;
}

/** The weekday ("MO"…) and week of the month (1…5) of a day key, and
 *  whether it is that weekday's last in the month. */
export function weekdayOf(key: string): { dow: string; nth: number; last: boolean } {
  const [y, m, d] = key.split("-").map(Number);
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  return { dow: DOW[wd], nth: Math.ceil(d / 7), last: d + 7 > daysInMonth(y, m - 1) };
}

/** The form's repeat rows for `e` (or a new event starting on `startDate`).
 *  Null for a rule LAAB can't edit, which an edit then leaves alone. */
export function repeatFormOf(e: CalendarEvent | null, startDate: string): RepeatForm | null {
  const none: RepeatForm = { frequency: "none", interval: 1, weekdays: [weekdayOf(startDate).dow], monthly: "date", until: null };
  const r = e?.recurrence;
  if (!r) return none;
  if (!(r.frequency in PERIOD_DAYS)) return null;
  const days = r.byDay.map(parseByDay);
  if (days.some((d) => !d)) return null;
  let monthly: RepeatForm["monthly"] = "date";
  if (r.frequency === "monthly" && days.length) {
    if (days.length > 1) return null;
    monthly = days[0]!.nth === -1 ? "last" : "nth";
  }
  return {
    frequency: r.frequency as RepeatForm["frequency"],
    interval: Math.max(1, r.interval || 1),
    weekdays: r.frequency === "weekly" && days.length ? days.map((d) => DOW[d!.dow]) : none.weekdays,
    monthly,
    until: r.until != null ? keyOf(partsIn(r.until)) : null,
  };
}

/** The monthly choices for a series starting on `startDate`: "วันที่ 6",
 *  "วันศุกร์แรก", and "วันศุกร์สุดท้าย" when it is the month's last. */
export function monthlyChoices(startDate: string): { value: RepeatForm["monthly"]; label: string }[] {
  const w = weekdayOf(startDate);
  const day = `วัน${thaiDow[DOW.indexOf(w.dow)]}`;
  const out: { value: RepeatForm["monthly"]; label: string }[] = [
    { value: "date", label: `วันที่ ${Number(startDate.slice(8))}` },
    { value: "nth", label: `${day}${ordinal[w.nth]}` },
  ];
  if (w.last) out.push({ value: "last", label: `${day}${ordinal[-1]}` });
  return out;
}

/** The rule to send. As LINE for Android's editor, the last day ends at
 *  23:59:59 there in the Mac's zone (`CreateEventViewModel`). */
export function repeatOf(f: RepeatForm, startDate: string): RepeatInput {
  const w = weekdayOf(startDate);
  const byDay =
    f.frequency === "weekly"
      ? DOW.filter((d) => f.weekdays.includes(d))
      : f.frequency === "monthly" && f.monthly !== "date"
        ? [`${f.monthly === "last" ? -1 : w.nth}${w.dow}`]
        : [];
  return {
    frequency: f.frequency,
    interval: Math.max(1, Math.floor(f.interval) || 1),
    until: f.frequency !== "none" && f.until ? local(f.until, "23:59") + 59_000 : null,
    byDay,
  };
}

/** LINE Calendar's reminder choices (`Reminder`), in the app's order. */
export const reminderOptions: { value: number; label: string }[] = [
  { value: 1, label: "1 ชั่วโมงก่อน" },
  { value: 3, label: "2 ชั่วโมงก่อน" },
  { value: 4, label: "1 วันก่อน" },
  { value: 5, label: "2 วันก่อน" },
  { value: 2, label: "12.00 น. วันก่อนหน้า" },
  { value: 6, label: "12.00 น. 2 วันก่อน" },
];

export const reminderLabel = (v: number) => reminderOptions.find((o) => o.value === v)?.label ?? `แจ้งเตือน (${v})`;

/** Epoch ms of the first and last day of a month grid, with a day's margin. */
export function gridRange(year: number, month: number): [number, number] {
  const cells = monthGrid(year, month);
  const [fy, fm, fd] = cells[0].key.split("-").map(Number);
  const [ly, lm, ld] = cells[cells.length - 1].key.split("-").map(Number);
  return [Date.UTC(fy, fm - 1, fd) - 2 * DAY, Date.UTC(ly, lm - 1, ld) + 3 * DAY];
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
