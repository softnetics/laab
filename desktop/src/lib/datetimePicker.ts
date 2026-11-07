// Flex and template datetime picker buttons. The values a bot sees are in
// the mode's format: "2026-11-06" (date), "09:30" (time) or
// "2026-11-06T09:30" (datetime), local wall time.

import { dayKey, dayLabel } from "./calendar";

export type PickerMode = "date" | "time" | "datetime";

const pad = (n: number) => String(n).padStart(2, "0");

export const isPickerMode = (m: string | undefined): m is PickerMode => m === "date" || m === "time" || m === "datetime";

/** `d` in the mode's format. */
export function pickerNow(mode: PickerMode, d = new Date()): string {
  const date = dayKey(d.getFullYear(), d.getMonth(), d.getDate());
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return mode === "date" ? date : mode === "time" ? time : `${date}T${time}`;
}

/** A value split into the picker's date and time fields. */
export function splitPicker(mode: PickerMode, value: string): { date: string; time: string } {
  if (mode === "date") return { date: value, time: "" };
  if (mode === "time") return { date: "", time: value };
  const [date, time = "00:00"] = value.split("T");
  return { date, time };
}

/** The picker's fields as the value to send. */
export const joinPicker = (mode: PickerMode, date: string, time: string) =>
  mode === "date" ? date : mode === "time" ? time : `${date}T${time}`;

/** "พ. 4 พ.ย. 9.30" — how a picked value reads to the user. */
export function pickerLabel(mode: PickerMode, value: string): string {
  const { date, time } = splitPicker(mode, value);
  const clock = time ? `${Number(time.slice(0, 2))}.${time.slice(3, 5)}` : "";
  if (mode === "time") return `${clock} น.`;
  return mode === "date" ? dayLabel(date) : `${dayLabel(date)} ${clock} น.`;
}

/** Why `value` can't be sent, or null. Values in one mode compare as text. */
export function outOfRange(mode: PickerMode, value: string, min: string | null, max: string | null): string | null {
  if ((min && value < min) || (max && value > max)) {
    const from = min ? pickerLabel(mode, min) : null;
    const to = max ? pickerLabel(mode, max) : null;
    if (from && to) return `เลือกได้ตั้งแต่ ${from} ถึง ${to}`;
    return from ? `เลือกได้ตั้งแต่ ${from}` : `เลือกได้ถึง ${to}`;
  }
  return null;
}
