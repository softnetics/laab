import type { Message } from "./types";
import { clock } from "./time";

// Wording for call rows, group events and unsent rows.

export interface CallInfo {
  kind: "voice" | "video";
  outcome: "completed" | "no_response" | "rejected" | "canceled" | "unknown";
  phase: "start" | "end" | null;
  group: boolean;
  durationMs: number;
}

export function readCall(m: Message): CallInfo | null {
  if (m.contentType !== "CALL") return null;
  const meta = m.metadata;
  const group = meta.TYPE === "G" || "GC_EVT_TYPE" in meta;
  const video = group ? meta.GC_MEDIA_TYPE === "VIDEO" || meta.GC_MEDIA_TYPE === "V" : meta.TYPE === "V";
  const outcomes: Record<string, CallInfo["outcome"]> = {
    NORMAL: "completed",
    NO_RESPONSE: "no_response",
    REJECTED: "rejected",
    CANCELED: "canceled",
  };
  return {
    kind: video ? "video" : "voice",
    outcome: outcomes[meta.RESULT ?? ""] ?? "unknown",
    phase: meta.GC_EVT_TYPE === "S" ? "start" : meta.GC_EVT_TYPE === "E" ? "end" : null,
    group,
    durationMs: Number(meta.DURATION ?? m.durationMs ?? 0) || 0,
  };
}

export function duration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** The label inside a call bubble. */
export function callBubbleLabel(call: CallInfo, mine: boolean): string {
  if (call.group && call.phase === "start") return "เริ่มการโทรแบบกลุ่มแล้ว";
  switch (call.outcome) {
    case "completed":
      return duration(call.durationMs);
    case "no_response":
      return mine ? "ไม่มีผู้รับสาย" : "สายที่ไม่ได้รับ";
    case "rejected":
      return mine ? "สายถูกปฏิเสธ" : "ปฏิเสธสาย";
    case "canceled":
      return mine ? "ยกเลิก" : "สายที่ไม่ได้รับ";
    default:
      return "การโทร";
  }
}

/** A group call's end is a centred notice, not a bubble. */
export const isCallNotice = (c: CallInfo | null) => !!c && c.group && c.phase === "end";

export interface ChatEventInfo {
  key: string;
  args: string[];
}

export function readChatEvent(m: Message): ChatEventInfo | null {
  if (m.contentType !== "CHATEVENT") return null;
  return {
    key: m.metadata.LOC_KEY ?? "",
    args: (m.metadata.LOC_ARGS ?? "").split("\u001e").filter(Boolean),
  };
}

/** "<actor> เพิ่ม <a, b> เข้ากลุ่ม"; other keys get a generic line. */
export function chatEventText(e: ChatEventInfo, name: (mid: string) => string): string {
  if (e.key === "C_MI" && e.args.length >= 2) {
    const [actor, ...added] = e.args;
    return `${name(actor)} เพิ่ม ${added.map(name).join(", ")} เข้ากลุ่ม`;
  }
  if (e.key === "C_MA" && e.args.length >= 1) return `${name(e.args[0])} เพิ่มประกาศ`;
  return "มีการเปลี่ยนแปลงในกลุ่ม";
}

export function unsentText(m: Message, name: (mid: string) => string): string {
  return m.mine ? "ยกเลิกข้อความแล้ว" : `${m.senderId ? name(m.senderId) : "สมาชิก"} ยกเลิกข้อความ`;
}

/** A centred row: unsent notice, group-call end, or group event. LINE
 *  underlines part of some rows: "ประกาศ" opens the announcements, and your
 *  own unsend gets a second line about LINE Premium's silent unsend. */
export type Notice = {
  kind: "unsent" | "call-end" | "event";
  time: string | null;
  text: string;
  link?: { text: string; action: "announcements" | "premium-unsend"; newLine?: boolean };
};

export function noticeFor(m: Message, name: (mid: string) => string): Notice | null {
  if (m.unsent === 1) {
    const n: Notice = { kind: "unsent", time: null, text: unsentText(m, name) };
    if (m.mine) n.link = { text: "วิธียกเลิกข้อความโดยไม่แจ้งเตือน", action: "premium-unsend", newLine: true };
    return n;
  }
  const call = readCall(m);
  if (isCallNotice(call)) return { kind: "call-end", time: clock(m.createdTime), text: "สิ้นสุดการโทรแบบกลุ่มแล้ว" };
  const ev = readChatEvent(m);
  if (ev?.key === "C_MA" && ev.args.length >= 1) {
    return {
      kind: "event",
      time: clock(m.createdTime),
      text: `${name(ev.args[0])} เพิ่ม`,
      link: { text: "ประกาศ", action: "announcements" },
    };
  }
  if (ev) return { kind: "event", time: clock(m.createdTime), text: chatEventText(ev, name) };
  return null;
}

/** Text messages LAAB could not read (usually E2EE it cannot decrypt). */
export const isBlank = (m: Message) => m.unsent === 0 && m.contentType === "NONE" && !m.imagePath && m.text.trim() === "";
