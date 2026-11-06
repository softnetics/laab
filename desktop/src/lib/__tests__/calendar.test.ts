import { describe, expect, it } from "vitest";
import {
  agendaTimes,
  durationLabel,
  expand,
  occurrenceAt,
  occurrenceKey,
  occurrenceStarts,
  recurrenceLabel,
  reminderLabel,
  zonedMs,
  parseTime,
  eventDays,
  longDayLabel,
  formOf,
  inputOf,
  linkDay,
  calendarName,
  monthGrid,
  newForm,
  timeLabel,
} from "../calendar";
import type { CalendarEvent } from "../types";

const ev = (p: Partial<CalendarEvent>): CalendarEvent => ({
  id: "e",
  title: "t",
  chatId: "c",
  start: 0,
  end: 0,
  allDay: false,
  timezone: "Asia/Bangkok",
  location: "",
  url: "",
  description: "",
  creator: "",
  updatedAt: 0,
  editable: true,
  ...p,
});

describe("calendar", () => {
  it("reads all-day events in their own zone and ends them before midnight", () => {
    // 4–5 Nov 2026 all day in Bangkok (UTC+7): 3 Nov 17:00Z to 5 Nov 17:00Z.
    const e = ev({ allDay: true, start: Date.UTC(2026, 10, 3, 17), end: Date.UTC(2026, 10, 5, 17) });
    expect(eventDays(e)).toEqual(["2026-11-04", "2026-11-05"]);
    expect(timeLabel({ ...e, end: Date.UTC(2026, 10, 4, 17) })).toBe("ทั้งวัน");
  });

  it("labels timed events with Thai clock times", () => {
    const start = new Date(2026, 10, 4, 15, 0).getTime();
    const e = ev({ start, end: new Date(2026, 10, 4, 16, 30).getTime() });
    expect(eventDays(e)).toEqual(["2026-11-04"]);
    expect(timeLabel(e)).toBe("15.00 – 16.30 น.");
    // Ending exactly at midnight stays on the first day.
    expect(eventDays({ ...e, end: new Date(2026, 10, 5).getTime() })).toEqual(["2026-11-04"]);
  });

  it("fills the agenda's time column per day", () => {
    const e = ev({ start: new Date(2026, 10, 4, 22, 0).getTime(), end: new Date(2026, 10, 6, 1, 30).getTime() });
    expect(agendaTimes(e, "2026-11-04")).toEqual(["22.00", "→"]);
    expect(agendaTimes(e, "2026-11-05")).toEqual(["ทั้งวัน", ""]);
    expect(agendaTimes(e, "2026-11-06")).toEqual(["→", "1.30"]);
    expect(agendaTimes(ev({ allDay: true }), "2026-11-04")).toEqual(["ทั้งวัน", ""]);
    expect(longDayLabel("2026-11-04", 2026)).toBe("วันพุธที่ 4 พฤศจิกายน");
    expect(longDayLabel("2027-01-01", 2026)).toBe("วันศุกร์ที่ 1 มกราคม 2027");
  });

  it("draws six weeks from the Sunday before the 1st", () => {
    const g = monthGrid(2026, 10); // November 2026 starts on a Sunday.
    expect(g).toHaveLength(42);
    expect(g[0]).toEqual({ key: "2026-11-01", day: 1, inMonth: true });
    expect(g[41].inMonth).toBe(false);
    expect(monthGrid(2026, 9)[0].key).toBe("2026-09-27");
  });

  it("round-trips the form, all-day ending at the next midnight in the event's zone", () => {
    const f = { ...newForm("2026-11-04"), title: "Release", allDay: true, endDate: "2026-11-05" };
    const input = inputOf(f, "Asia/Bangkok");
    if ("error" in input) throw new Error(input.error);
    // Midnight in Bangkok (UTC+7), whatever the Mac's zone.
    expect(input.start).toBe(Date.UTC(2026, 10, 3, 17));
    expect(input.end).toBe(Date.UTC(2026, 10, 5, 17));
    expect(input.timezone).toBe("Asia/Bangkok");
    const back = formOf(ev({ ...input }));
    expect([back.startDate, back.endDate]).toEqual(["2026-11-04", "2026-11-05"]);
    // An edit keeps the event's zone and sends the chosen reminders.
    const tokyo = inputOf({ ...f, allDay: false }, "Asia/Tokyo", [1, 4]);
    if ("error" in tokyo) throw new Error(tokyo.error);
    expect(tokyo.timezone).toBe("Asia/Tokyo");
    expect(tokyo.reminders).toEqual([1, 4]);
    expect("reminders" in (inputOf(f) as object)).toBe(false);
  });

  it("turns a wall time in a zone into an instant", () => {
    expect(zonedMs("2026-11-04", "09:30", "Asia/Bangkok")).toBe(Date.UTC(2026, 10, 4, 2, 30));
    expect(zonedMs("2026-11-04", "00:00", "Asia/Tokyo")).toBe(Date.UTC(2026, 10, 3, 15));
    // Across New York's DST change (1 Nov 2026).
    expect(zonedMs("2026-10-31", "12:00", "America/New_York")).toBe(Date.UTC(2026, 9, 31, 16));
    expect(zonedMs("2026-11-02", "12:00", "America/New_York")).toBe(Date.UTC(2026, 10, 2, 17));
  });

  it("refuses an empty title or an end before the start", () => {
    expect(inputOf(newForm("2026-11-04"))).toEqual({ error: "ใส่ชื่อนัดหมายก่อน" });
    const f = { ...newForm("2026-11-04"), title: "x", startTime: "10:00", endTime: "09:00" };
    expect("error" in inputOf(f)).toBe(true);
    expect("error" in inputOf({ ...f, allDay: true, endDate: "2026-11-03" })).toBe(true);
    expect("error" in inputOf({ ...f, allDay: true })).toBe(false);
  });

  it("reads times typed in any common way", () => {
    const cases: [string, string | null][] = [
      ["9", "09:00"],
      ["930", "09:30"],
      ["0930", "09:30"],
      ["9.30", "09:30"],
      ["9:3", "09:30"],
      ["21.15 น.", "21:15"],
      ["9.30pm", "21:30"],
      ["12am", "00:00"],
      ["12 pm", "12:00"],
      ["บ่าย 2", "14:00"],
      ["บ่ายโมง", null],
      ["24", null],
      ["9.75", null],
      ["abc", null],
    ];
    for (const [text, want] of cases) expect(parseTime(text), text).toBe(want);
  });

  it("says how long an event runs", () => {
    expect(durationLabel(15)).toBe("15 นาที");
    expect(durationLabel(60)).toBe("1 ชม.");
    expect(durationLabel(90)).toBe("1.5 ชม.");
    expect(durationLabel(130)).toBe("2 ชม. 10 นาที");
  });
});

describe("calendarName", () => {
  it("prefers the chat's name, then LINE's, then says whose", () => {
    expect(calendarName("c1", "ทีม", "x")).toBe("ทีม");
    expect(calendarName("c1", undefined, "ทีม LINE")).toBe("ทีม LINE");
    expect(calendarName("cal:abc")).toBe("ปฏิทินของฉัน");
  });
});

describe("linkDay", () => {
  it("reads the day from an event card's link", () => {
    // 2026-11-03 12:00 UTC: the same day in any zone within ±11 hours.
    expect(linkDay("https://line.me/R/calendar/list?startTime=1793707200&timeZone=Asia%2FBangkok&allDay=false")).toBe("2026-11-03");
    // An all-day event starts at midnight in its own zone.
    expect(linkDay("https://line.me/R/calendar/list?startTime=1793638800&timeZone=Asia%2FBangkok&allDay=true")).toBe("2026-11-03");
    expect(linkDay("https://line.me/R/calendar")).toBeNull();
    expect(linkDay("https://line.me/R/calendar/list?startTime=x")).toBeNull();
  });

  describe("repeats", () => {
    const bkk = (d: number, h = 10, m = 0, month = 10) => Date.UTC(2026, month, d, h - 7, m);
    const weekly = ev({
      id: "s",
      start: bkk(2, 10),
      end: bkk(2, 10, 30),
      recurrence: { frequency: "weekly", interval: 1, until: null, byDay: ["MO", "WE", "FR"], exceptIds: [] },
    });

    it("expands a weekly event on its days", () => {
      // Mon 2 Nov 2026 onward: Mon, Wed, Fri.
      const starts = occurrenceStarts(weekly, bkk(1, 0), bkk(10, 0));
      expect(starts).toEqual([bkk(2), bkk(4), bkk(6), bkk(9)]);
    });

    it("leaves out deleted occurrences and shows changed ones as changed", () => {
      const e = {
        ...weekly,
        recurrence: { ...weekly.recurrence!, exceptIds: [bkk(4), bkk(6)] },
        overrides: [
          { recurId: bkk(6), deleted: false, title: "Moved", start: bkk(7, 14), end: bkk(7, 15), allDay: false, timezone: "", location: "", url: "", description: "" },
          { recurId: bkk(9), deleted: true, title: "", start: 0, end: 0, allDay: false, timezone: "", location: "", url: "", description: "" },
        ],
      };
      const list = expand([e], bkk(1, 0), bkk(10, 0));
      expect(list.map((x) => [x.title, x.start, x.recurId])).toEqual([
        ["t", bkk(2), bkk(2)],
        ["Moved", bkk(7, 14), bkk(6)],
      ]);
      expect(new Set(list.map(occurrenceKey)).size).toBe(2);
      expect(occurrenceAt(e, bkk(6)).title).toBe("Moved");
      expect(occurrenceAt(e, bkk(2)).end - bkk(2)).toBe(30 * 60_000);
    });

    it("steps by the interval and stops at until", () => {
      const daily = ev({
        start: bkk(1),
        end: bkk(1, 11),
        recurrence: { frequency: "daily", interval: 3, until: bkk(10), byDay: [], exceptIds: [] },
      });
      expect(occurrenceStarts(daily, 0, bkk(30))).toEqual([bkk(1), bkk(4), bkk(7), bkk(10)]);
    });

    it("finds the nth and last weekday of a month, and skips short months", () => {
      const lastFriday = ev({
        start: Date.UTC(2026, 0, 30, 3),
        end: Date.UTC(2026, 0, 30, 4),
        recurrence: { frequency: "monthly", interval: 1, until: null, byDay: ["-1FR"], exceptIds: [] },
      });
      const days = occurrenceStarts(lastFriday, 0, Date.UTC(2026, 4, 1)).map((t) => new Date(t).getUTCDate());
      expect(days).toEqual([30, 27, 27, 24]);
      const second = { ...lastFriday, recurrence: { ...lastFriday.recurrence!, byDay: ["2TU"] } };
      expect(occurrenceStarts(second, 0, Date.UTC(2026, 2, 1)).map((t) => new Date(t).getUTCDate())).toEqual([10]);
      const the31st = ev({
        start: Date.UTC(2026, 0, 31, 3),
        end: Date.UTC(2026, 0, 31, 4),
        recurrence: { frequency: "monthly", interval: 1, until: null, byDay: [], exceptIds: [] },
      });
      expect(occurrenceStarts(the31st, 0, Date.UTC(2026, 5, 1)).map((t) => new Date(t).getUTCMonth())).toEqual([0, 2, 4]);
    });

    it("keeps all-day repeats on their day in the event's zone", () => {
      const birthday = ev({
        allDay: true,
        start: Date.UTC(2025, 1, 28, 17),
        end: Date.UTC(2025, 2, 1, 17),
        recurrence: { frequency: "yearly", interval: 1, until: null, byDay: [], exceptIds: [] },
      });
      const list = expand([birthday], Date.UTC(2027, 0, 1), Date.UTC(2027, 11, 31));
      expect(list).toHaveLength(1);
      expect(eventDays(list[0])).toEqual(["2027-03-01"]);
    });

    it("jumps ahead to a window far from the first occurrence", () => {
      const starts = occurrenceStarts(weekly, bkk(1, 0, 0, 10) + 3 * 365 * 86_400_000, bkk(1, 0, 0, 10) + (3 * 365 + 7) * 86_400_000);
      expect(starts.length).toBe(3);
    });

    it("describes the rule and the reminders in Thai", () => {
      expect(recurrenceLabel(weekly)).toBe("ทุกสัปดาห์ วันจันทร์ พุธ ศุกร์");
      expect(recurrenceLabel(ev({ start: bkk(15), recurrence: { frequency: "monthly", interval: 2, until: bkk(1, 0, 0, 11), byDay: [], exceptIds: [] } }), 2026)).toBe(
        "ทุก 2 เดือน วันที่ 15 จนถึง อ. 1 ธ.ค.",
      );
      expect(recurrenceLabel(ev({ recurrence: { frequency: "monthly", interval: 1, until: null, byDay: ["-1FR"], exceptIds: [] } }))).toBe(
        "ทุกเดือน วันศุกร์สุดท้าย",
      );
      expect(recurrenceLabel(ev({}))).toBe("");
      expect(reminderLabel(2)).toBe("12.00 น. วันก่อนหน้า");
    });
  });
});
