// Thai date labels in LINE's style: "13.08 น.", "เมื่อวาน", "พุธ", "21 ก.ย.".

const weekdays = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"];
const months = ["ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค."];

/** LINE timestamps arrive in milliseconds; older rows may be in seconds. */
export function toDate(value: number): Date | null {
  if (!(value > 0)) return null;
  return new Date(value > 1e11 ? value : value * 1000);
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function dayDiff(date: Date, now: Date): number {
  return Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
}

const pad = (n: number) => String(n).padStart(2, "0");

export function clock(value: number): string {
  const d = toDate(value);
  if (!d) return "";
  return `${pad(d.getHours())}.${pad(d.getMinutes())} น.`;
}

export function listLabel(value: number, now = new Date()): string {
  const d = toDate(value);
  if (!d) return "";
  const days = dayDiff(d, now);
  if (days === 0) return clock(value);
  if (days === 1) return "เมื่อวาน";
  if (days > 1 && days < 7) return weekdays[d.getDay()];
  const dm = `${d.getDate()} ${months[d.getMonth()]}`;
  return d.getFullYear() === now.getFullYear() ? dm : `${dm} ${d.getFullYear()}`;
}

export function dayHeader(value: number, now = new Date()): string {
  const d = toDate(value);
  if (!d) return "";
  const days = dayDiff(d, now);
  if (days === 0) return "วันนี้";
  if (days === 1) return "เมื่อวาน";
  // LINE for Mac: "19 ต.ค.", with the year when it is not this year.
  const base = `${d.getDate()} ${months[d.getMonth()]}`;
  return d.getFullYear() === now.getFullYear() ? base : `${base} ${d.getFullYear()}`;
}

export function sameDay(a: number, b: number): boolean {
  const x = toDate(a);
  const y = toDate(b);
  return !!x && !!y && startOfDay(x) === startOfDay(y);
}

/** The viewer's stamp: "29 ต.ค. 2026 7.40 น.". */
export function fullStamp(value: number): string {
  const d = toDate(value);
  if (!d) return "";
  return `${d.getDate()} ${months[d.getMonth()]} ${d.getFullYear()} ${d.getHours()}.${pad(d.getMinutes())} น.`;
}
