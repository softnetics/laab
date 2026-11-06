import { describe, expect, it } from "vitest";
import { compareIds, later, placement, readersOf } from "../reads";
import { backspaceMention, composingQuery, fromSpans, insertMention, matching, present, targets, ALL_MEMBER } from "../mentions";
import { callBubbleLabel, chatEventText, noticeFor, readCall, readChatEvent, isBlank } from "../notices";
import { buildRows } from "../timeline";
import { isAnimated } from "../media";
import { findHits, ranges } from "../inChatSearch";
import { linkify, liffId, previewUrl } from "../links";
import { adjacentChat, filterChats, folderName, groupRows, listPreview } from "../chatList";
import { nextReaction, topTypes } from "../reactions";
import { dayHeader, fullStamp, listLabel } from "../time";
import type { ChatList, Message } from "../types";

const msg = (p: Partial<Message>): Message => ({
  id: "1",
  chatId: "c1",
  text: "",
  fileName: null,
  filePath: null,
  imagePath: null,
  contentType: "NONE",
  createdTime: Date.now(),
  mine: false,
  senderId: "u1",
  stickerId: null,
  stickerPackage: null,
  stickerVersion: null,
  mentions: [],
  mentionsMe: false,
  metadata: {},
  replyTo: null,
  unsent: 0,
  edited: false,
  fileSize: null,
  durationMs: null,
  reactions: [],
  ...p,
});

describe("read marks", () => {
  it("compares 64-bit ids exactly", () => {
    expect(compareIds("9007199254740993", "9007199254740992")).toBe(1);
    expect(later("200", "100")).toBe("200");
  });
  it("places marks on your messages in a 1:1 and on any message in a group", () => {
    const ms = [msg({ id: "10", mine: true }), msg({ id: "20" }), msg({ id: "30", mine: true })];
    expect(placement(ms, { u1: "25" }, true)).toEqual({ "10": ["u1"] });
    expect(placement(ms, { u1: "25" }, false)).toEqual({ "20": ["u1"] });
    // Each of your messages counts everyone who read that far.
    expect(readersOf(ms, { u1: "25", u2: "30" })).toEqual({ "10": ["u1", "u2"], "30": ["u2"] });
  });
});

describe("mentions", () => {
  it("opens on @ at a token start and inserts the label", () => {
    expect(composingQuery("hi @Ar", 6, [])).toBe("Ar");
    expect(composingQuery("mail@x", 6, [])).toBeNull();
    const r = insertMention("hi @Ar", 6, { mid: "u_arthit", name: "Arthit" }, [])!;
    expect(r.text).toBe("hi @Arthit ");
    expect(targets(r.text, r.mentions)).toEqual([{ start: 3, end: 10, mid: "u_arthit" }]);
    expect(composingQuery(r.text, 10, r.mentions)).toBeNull();
  });
  it("sends @All as all", () => {
    const r = insertMention("@", 1, ALL_MEMBER, [])!;
    expect(targets(r.text, r.mentions)).toEqual([{ start: 0, end: 4, all: true }]);
  });
  it("drops edited mentions and deletes a whole tag on backspace", () => {
    const m = [{ mid: "u1", all: false, label: "@Arthit" }];
    expect(present("@Arthi x", m)).toEqual([]);
    expect(backspaceMention("hi @Arthit", 10, m)).toEqual({ text: "hi ", caret: 3, mentions: [] });
  });
  it("takes a sent message's mentions back into the composer to edit", () => {
    const text = "@Arthit @All hi";
    const m = fromSpans(text, [
      { start: 8, end: 12, all: true },
      { start: 0, end: 7, mid: "u1" },
    ]);
    expect(targets(text, m)).toEqual([
      { start: 0, end: 7, mid: "u1" },
      { start: 8, end: 12, all: true },
    ]);
    expect(fromSpans("hi there", [{ start: 0, end: 2, mid: "u1" }])).toEqual([]);
  });
  it("matches members, All by either name", () => {
    const members = [ALL_MEMBER, { mid: "u1", name: "Arthit" }];
    expect(matching(members, "ทั้ง").map((m) => m.mid)).toEqual(["all"]);
    expect(matching(members, "ar").map((m) => m.mid)).toEqual(["u1"]);
  });
});

describe("notices", () => {
  const name = (mid: string) => ({ me: "Nara", u2: "O" })[mid] ?? "สมาชิก";
  it("words calls", () => {
    const missed = readCall(msg({ contentType: "CALL", metadata: { TYPE: "A", RESULT: "NO_RESPONSE" } }))!;
    expect(callBubbleLabel(missed, false)).toBe("สายที่ไม่ได้รับ");
    expect(callBubbleLabel(missed, true)).toBe("ไม่มีผู้รับสาย");
    const done = readCall(msg({ contentType: "CALL", metadata: { TYPE: "V", RESULT: "NORMAL", DURATION: "3725000" } }))!;
    expect(done.kind).toBe("video");
    expect(callBubbleLabel(done, true)).toBe("1:02:05");
    const busy = readCall(msg({ contentType: "CALL", metadata: { TYPE: "A", RESULT: "BUSY" } }))!;
    expect(callBubbleLabel(busy, true)).toBe("สายไม่ว่าง");
    expect(callBubbleLabel(busy, false)).toBe("สายที่ไม่ได้รับ");
    expect(callBubbleLabel(readCall(msg({ contentType: "CALL", metadata: { RESULT: "FAIL" } }))!, true)).toBe("โทรไม่สำเร็จ");
    expect(callBubbleLabel(readCall(msg({ contentType: "CALL", metadata: { RESULT: "INFO" } }))!, true)).toBe("การโทร");
    const invite = readCall(msg({ contentType: "CALL", metadata: { GC_EVT_TYPE: "I", GC_MEDIA_TYPE: "VIDEO" } }))!;
    expect([invite.group, invite.kind, callBubbleLabel(invite, false)]).toEqual([true, "video", "เชิญเข้าร่วมการโทรแบบกลุ่ม"]);
    // Older call logs carry voip… keys.
    const legacy = readCall(msg({ contentType: "CALL", metadata: { voipType: "V", voipResult: "NORMAL", voipDuration: "61000" } }))!;
    expect([legacy.kind, callBubbleLabel(legacy, false)]).toEqual(["video", "01:01"]);
  });
  it("turns group events and unsends into notice rows", () => {
    const ev = readChatEvent(msg({ contentType: "CHATEVENT", metadata: { LOC_KEY: "C_MI", LOC_ARGS: "me\u001eu2" } }))!;
    expect(chatEventText(ev, name)).toBe("Nara เพิ่ม O เข้ากลุ่ม");
    expect(chatEventText({ key: "C_MA", args: ["me"] }, name)).toBe("Nara เพิ่มประกาศ");
    expect(chatEventText({ key: "A_MI", args: ["me", "u2"] }, name)).toBe("Nara เพิ่ม O เข้ากลุ่ม");
    expect(chatEventText({ key: "A_MC", args: ["me", "u2"] }, name)).toBe("Nara สร้างกลุ่มนี้");
    expect(chatEventText({ key: "C_ML", args: ["u2"] }, name)).toBe("O ออกจากกลุ่ม");
    expect(chatEventText({ key: "X_YZ", args: ["u2"] }, name)).toBe("มีการเปลี่ยนแปลงในกลุ่ม");
    expect(noticeFor(msg({ unsent: 1, senderId: "u2" }), name)?.text).toBe("O ยกเลิกข้อความ");
    expect(noticeFor(msg({ unsent: 1, mine: true }), name)?.text).toBe("ยกเลิกข้อความแล้ว");
    expect(isBlank(msg({ text: "  " }))).toBe(true);
  });
  it("groups by sender and minute, as LINE does", () => {
    const t = new Date(2026, 9, 21, 14, 57, 5).getTime();
    const rows = buildRows(
      [
        msg({ id: "a", createdTime: t }),
        msg({ id: "b", createdTime: t + 20_000, contentType: "STICKER" }),
        msg({ id: "c", createdTime: t + 60_000 }),
        msg({ id: "d", createdTime: t + 61_000, contentType: "IMAGE" }),
        msg({ id: "e", createdTime: t + 62_000, contentType: "IMAGE" }),
      ],
      name,
    );
    expect(rows.map((r) => [r.message.id, r.runStart])).toEqual([
      ["a", true],
      ["b", false],
      ["c", true],
      ["d", true],
      ["e", true],
    ]);
  });
  it("puts photos sent together in one grid", () => {
    const t = new Date(2026, 9, 21, 14, 57, 5).getTime();
    const photo = (id: string, at: number, gid?: string, more: Partial<Message> = {}) =>
      msg({ id, createdTime: t + at, contentType: "IMAGE", metadata: gid ? { GID: gid } : {}, ...more });
    const rows = buildRows(
      [
        photo("a", 0, "g1"),
        photo("b", 0, "g1"),
        photo("c", 1_000, "g1"),
        photo("d", 2_000, "g2"), // another batch right after
        photo("e", 2_000, "g2"),
        photo("f", 3_000), // sent on its own
        photo("g", 4_000), // also on its own, though only a second later
        photo("h", 5_000, "g3"),
        photo("i", 5_000, "g3", { unsent: 1 }),
      ],
      name,
    );
    expect(rows.map((r) => [r.message.id, r.photos?.map((m) => m.id) ?? null])).toEqual([
      ["a", ["a", "b", "c"]],
      ["d", ["d", "e"]],
      ["f", null],
      ["g", null],
      ["h", null],
      ["i", null],
    ]);
  });
  it("hides silent unsends and starts runs after notices", () => {
    const rows = buildRows([msg({ id: "a" }), msg({ id: "b", unsent: 2 }), msg({ id: "c", unsent: 1 }), msg({ id: "d" })], name);
    expect(rows.map((r) => [r.message.id, r.runStart])).toEqual([
      ["a", true],
      ["c", true],
      ["d", true],
    ]);
  });
});

describe("in-chat search", () => {
  it("finds loaded text messages newest first, skipping media", () => {
    const ms = [
      msg({ id: "1", text: "API spec" }),
      msg({ id: "2", contentType: "IMAGE", text: "api" }),
      msg({ id: "3", text: "ａｐｉ docs" }),
    ];
    expect(findHits(ms, "api")).toEqual(["3", "1"]);
    expect(ranges("API and api", "api")).toEqual([
      [0, 3],
      [8, 11],
    ]);
  });
});

describe("links", () => {
  it("previews the first web link only", () => {
    expect(previewUrl("hi line://app/1 then www.a.com and https://b.com")).toBe("https://www.a.com");
    expect(previewUrl("only line://nv/chat")).toBeNull();
    expect(previewUrl("no links")).toBeNull();
  });
  it("splits urls out of text and spots LIFF links", () => {
    expect(linkify("see https://a.com/x. ok").map((s) => s.url ?? null)).toEqual([null, "https://a.com/x", null]);
    expect(liffId("https://liff.line.me/1234567-AbCdEf12/x")).toBe("1234567-AbCdEf12");
    expect(liffId("line://app/1234567-AbCdEf12")).toBe("1234567-AbCdEf12");
    expect(liffId("https://example.com")).toBeNull();
  });
});

describe("chat list", () => {
  const list: ChatList = {
    chats: [
      {
        id: "u1",
        name: "Arthit",
        kind: "dm",
        preview: "เขา: hi",
        lastActivity: 3,
        unread: 0,
        avatar: null,
        muted: false,
        picture: null,
        memberCount: null,
      },
      {
        id: "c1",
        name: "Team",
        kind: "group",
        preview: "",
        lastActivity: 2,
        unread: 2,
        avatar: null,
        muted: false,
        picture: null,
        memberCount: null,
      },
      {
        id: "u2",
        name: "Mint",
        kind: "dm",
        preview: "",
        lastActivity: 1,
        unread: 0,
        avatar: null,
        muted: false,
        picture: null,
        memberCount: null,
      },
    ],
    workspaces: [],
    pins: { main: ["u2"] },
  };
  it("puts pins first and sorts unread first when asked", () => {
    expect(filterChats(list, "all", "recent", "").map((c) => c.id)).toEqual(["u2", "u1", "c1"]);
    expect(filterChats(list, "all", "unread", "").map((c) => c.id)).toEqual(["u2", "c1", "u1"]);
    expect(filterChats(list, "groups", "recent", "").map((c) => c.id)).toEqual(["c1"]);
    expect(listPreview("เขา: hi")).toBe("hi");
  });
  it("groups a folder's chats under it, where its newest chat would be", () => {
    const foldered: ChatList = { ...list, workspaces: [{ id: "f", name: "", position: 1 }], pins: { main: [], f: ["c1", "u2"] } };
    const chats = filterChats(foldered, "all", "recent", "");
    const shape = (open: string[]) =>
      groupRows(foldered, chats, new Set(open)).map((r) => (r.kind === "folder" ? `[${r.chats.map((c) => c.id)}]` : r.chat.id));
    expect(shape([])).toEqual(["u1", "[c1,u2]"]);
    expect(shape(["f"])).toEqual(["u1", "[c1,u2]", "c1", "u2"]);
    expect(folderName(foldered.workspaces[0], foldered)).toBe("Team, Mint");
  });
  it("steps between chats without wrapping", () => {
    expect(adjacentChat(["a", "b"], null, 1)).toBe("a");
    expect(adjacentChat(["a", "b"], null, -1)).toBe("b");
    expect(adjacentChat(["a", "b"], "b", 1)).toBeNull();
  });
});

describe("reactions and time", () => {
  it("orders badge types and toggles", () => {
    expect(
      topTypes([
        { userId: "a", reaction: "LOVE" },
        { userId: "b", reaction: "NICE" },
        { userId: "c", reaction: "NICE" },
      ]),
    ).toEqual(["NICE", "LOVE"]);
    expect(nextReaction("NICE", "NICE")).toBeNull();
  });
  it("labels times like LINE", () => {
    const now = new Date(2026, 9, 22, 15, 0);
    expect(listLabel(new Date(2026, 9, 22, 13, 8).getTime(), now)).toBe("13.08 น.");
    expect(listLabel(new Date(2026, 9, 21, 9).getTime(), now)).toBe("เมื่อวาน");
    expect(listLabel(new Date(2026, 8, 21).getTime(), now)).toBe("21 ก.ย.");
    expect(dayHeader(new Date(2026, 9, 19).getTime(), now)).toBe("19 ต.ค.");
    expect(dayHeader(new Date(2025, 9, 19).getTime(), now)).toBe("19 ต.ค. 2025");
    expect(fullStamp(new Date(2026, 9, 29, 7, 40).getTime())).toBe("29 ต.ค. 2026 7.40 น.");
  });
});

describe("animated photos", () => {
  const photo = (info?: string) => msg({ contentType: "IMAGE", metadata: info ? { MEDIA_CONTENT_INFO: info } : {} });
  it("plays a GIF that LINE marks animated", () => {
    expect(isAnimated(photo('{"category":"original","extension":"gif","animated":true}'))).toBe(true);
    expect(isAnimated(photo('{"extension":"GIF"}'))).toBe(true);
  });
  it("keeps other photos still", () => {
    expect(isAnimated(photo('{"category":"original","extension":"jpg"}'))).toBe(false);
    expect(isAnimated(photo("not json"))).toBe(false);
    expect(isAnimated(photo())).toBe(false);
    expect(isAnimated(msg({ contentType: "VIDEO", metadata: { MEDIA_CONTENT_INFO: '{"animated":true}' } }))).toBe(false);
  });
});
