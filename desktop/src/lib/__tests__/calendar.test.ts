import { describe, expect, it } from "vitest";
import {
  agendaTimes,
  durationLabel,
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
    // An edit keeps the event's zone.
    const tokyo = inputOf({ ...f, allDay: false }, "Asia/Tokyo");
    if ("error" in tokyo) throw new Error(tokyo.error);
    expect(tokyo.timezone).toBe("Asia/Tokyo");
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
});
