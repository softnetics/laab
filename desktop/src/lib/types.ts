// Mirrors src-tauri/src/model.rs.

export type ChatKind = "dm" | "group" | "bot";

export interface Chat {
  id: string;
  name: string;
  kind: ChatKind | string;
  preview: string;
  lastActivity: number;
  unread: number;
  avatar: string | null;
  muted: boolean;
  picture: string | null;
  memberCount: number | null;
  /** A group we are invited to and have not joined. */
  invited?: boolean;
  /** In LINE's message request box: from someone who is not a friend, not accepted yet. */
  request?: boolean;
}

export interface ChatMember {
  mid: string;
  name: string;
  all?: boolean;
  picture?: string | null;
  invited?: boolean;
}

export interface MentionSpan {
  start: number;
  end: number;
  mid?: string;
  all?: boolean;
}

export interface Reaction {
  userId: string;
  reaction: ReactionName;
}

export type ReactionName = "NICE" | "LOVE" | "FUN" | "AMAZING" | "SAD" | "OMG" | "PAID";

export interface Message {
  id: string;
  chatId: string;
  text: string;
  fileName: string | null;
  filePath: string | null;
  imagePath: string | null;
  contentType: string;
  createdTime: number;
  mine: boolean;
  senderId: string | null;
  stickerId: string | null;
  stickerPackage: string | null;
  stickerVersion: string | null;
  mentions: MentionSpan[];
  mentionsMe: boolean;
  metadata: Record<string, string>;
  replyTo: string | null;
  /** 0 = no, 1 = unsent with a notice row, 2 = silently unsent. */
  unsent: number;
  edited: boolean;
  fileSize: number | null;
  durationMs: number | null;
  reactions: Reaction[];
}

export interface Workspace {
  id: string;
  name: string;
  position: number;
}

export interface ChatList {
  chats: Chat[];
  workspaces: Workspace[];
  pins: Record<string, string[]>;
}

export type SearchKind = "chat" | "message" | "file";

/** Usage statistics: `unavailable` in demo mode and in builds without a PostHog key. */
export type AnalyticsStatus = "unavailable" | "unasked" | "on" | "off";

export type McpAccess = "off" | "ask" | "on";
export type McpGroup = "read" | "send" | "manage" | "account";
export type McpLevels = Record<McpGroup, McpAccess>;
/** A rule's levels; null (or missing) follows the next rule down. */
export type McpOverrides = Partial<Record<McpGroup, McpAccess | null>>;
export type McpRuleKind = "chat" | "folder" | "type";
export interface McpRule {
  kind: McpRuleKind;
  /** Chat id, folder id, or `dm` / `group` / `bot`. */
  id: string;
  levels: McpOverrides;
}
export interface McpSettings {
  enabled: boolean;
  defaults: McpLevels;
  rules: McpRule[];
}
export interface McpActivity {
  time: number;
  client: string;
  tool: string;
  summary: string;
  outcome: "ok" | "denied" | "error";
}
export interface McpSetup {
  listening: boolean;
  clients: number;
  socket: string;
  command: string;
  args: string[];
  env: [string, string][];
}

export interface SearchHit {
  kind: SearchKind;
  chatId: string;
  messageId: string | null;
  title: string;
  detail: string;
}

export interface Sticker {
  stickerId: string;
  packageId: string;
  version: string | null;
}

export interface StickerPack {
  id: string;
  name: string;
  version: string | null;
  recent: boolean;
  stickers: Sticker[];
}

export interface Profile {
  mid: string;
  displayName: string;
  picture: string | null;
  device: string;
}

export interface Session {
  state: "starting" | "signed_out" | "logging_in" | "signed_in" | "demo";
  status: string;
  qrUrl: string | null;
  pin: string | null;
  profile: Profile | null;
  syncDone: number;
  syncTotal: number;
  /** LINE Premium; null until LINE says. Editing needs it. */
  premium: boolean | null;
}

export interface Person {
  name: string;
  picture: string | null;
}

export interface DraftMention {
  mid: string;
  all: boolean;
  label: string;
}

export interface Draft {
  text: string;
  mentions: DraftMention[];
}

export interface NotePost {
  id: string;
  authorMid: string;
  authorName: string;
  authorPicture: string | null;
  /** `obs`: where it lies, `service/namespace/objectId`. */
  media: { objectId: string; obs: string; video: boolean }[];
  text: string;
  createdTime: number;
  mediaCount: number;
  likeCount: number;
  commentCount: number;
}

/** An event on a chat's LINE Calendar. Times are epoch ms; an all-day
 *  event ends at midnight after its last day, in `timezone`. */
export interface CalendarEvent {
  id: string;
  title: string;
  start: number;
  end: number;
  allDay: boolean;
  timezone: string;
  location: string;
  url: string;
  description: string;
  /** Mid of who made it. */
  creator: string;
  /** LINE's client_updated_at, sent back on edit and delete. */
  updatedAt: number;
  editable: boolean;
  /** Whose calendar: a chat mid, or `cal:` and an id for the user's own
   *  calendar. Edits and deletes go here. */
  chatId: string;
  /** How it repeats; `start`/`end` are then the first occurrence. */
  recurrence?: CalendarRecurrence | null;
  /** Occurrences changed or deleted on their own. */
  overrides?: CalendarOverride[];
  /** LINE `Reminder` values, see `reminderLabels`. */
  reminders?: number[];
  invitees?: CalendarInvitee[];
  colorId?: number;
  /** Set on one occurrence of a recurring event: its original start (ms),
   *  LINE's recur_id. `id` stays the series'. */
  recurId?: number;
}

export interface CalendarRecurrence {
  frequency: "daily" | "weekly" | "monthly" | "yearly" | "other";
  interval: number;
  /** Last moment an occurrence may start, epoch ms. */
  until: number | null;
  /** iCalendar days: "MO"… for weekly, "2TU" / "-1FR" for monthly. */
  byDay: string[];
  /** Original starts (ms) of occurrences left out. */
  exceptIds: number[];
}

export interface CalendarOverride {
  recurId: number;
  deleted: boolean;
  title: string;
  start: number;
  end: number;
  allDay: boolean;
  timezone: string;
  location: string;
  url: string;
  description: string;
  /** This occurrence's invitees and their answers to it. */
  invitees?: CalendarInvitee[];
}

export interface CalendarInvitee {
  mid: string;
  status: "needsAction" | "accepted" | "declined" | "";
  me: boolean;
  updatedAt: number;
}

/** Every calendar the user can see, for the rail's ปฏิทิน window. */
export interface AllCalendars {
  calendars: { chatId: string; name: string }[];
  events: CalendarEvent[];
}

export type CalendarEventInput = Pick<
  CalendarEvent,
  "title" | "start" | "end" | "allDay" | "timezone" | "location" | "url" | "description" | "reminders"
> & {
  /** Left out, an edit keeps the event's rule as it is. */
  repeat?: RepeatInput;
};

/** A repeat rule from the event form. `until` is epoch ms. */
export interface RepeatInput {
  frequency: "none" | "daily" | "weekly" | "monthly" | "yearly";
  interval: number;
  until: number | null;
  byDay: string[];
}

export interface Album {
  id: string;
  title: string;
  photoCount: number;
}

export interface AlbumPhoto {
  id: string;
  oid: string;
  /** Where the photo is cached once it has been shown. */
  path: string;
  video: boolean;
}

export interface NoteComment {
  id: string;
  authorMid: string;
  authorName: string;
  authorPicture: string | null;
  text: string;
  createdTime: number;
  /** OBS paths of its images, `service/namespace/objectId`. */
  images: string[];
}

export interface NoteDetail {
  note: NotePost;
  comments: NoteComment[];
}

/** Someone's profile card (`contact_profile`). */
export interface ContactProfile {
  mid: string;
  name: string;
  picture: string | null;
  statusMessage: string | null;
  /** OBS object id of the cover photo, if LINE returned one. */
  cover: string | null;
  friend: boolean;
  blocked: boolean;
}

/** An official account's rich menu, in canvas pixels. */
/** What a rich-menu area does: `web` opens `uri`, `postback` sends `data`, `sendMessage` sends `text`,
 *  `transition` shows `scene`, `clipboard` copies `clipboardText`. */
/** A friend or an official account the user added. */
export interface Friend {
  mid: string;
  name: string;
  picture: string | null;
  kind: "dm" | "bot";
}

export interface RichMenuAction {
  type: string;
  label: string | null;
  uri: string | null;
  data: string | null;
  text: string | null;
  scene: string | null;
  clipboardText: string | null;
}
export interface RichMenuScene {
  images: { url: string; x: number; y: number; w: number; h: number }[];
  areas: { x: number; y: number; w: number; h: number; action: RichMenuAction }[];
}
export interface RichMenu {
  label: string;
  width: number;
  height: number;
  initialScene: string;
  scenes: Record<string, RichMenuScene>;
}

/** A LIFF window as LIFF DevTools sees it. */
export interface LiffDevEvent {
  t: number;
  kind: "console" | "network" | "bridge" | "nav" | "error" | "page";
  level: "debug" | "log" | "info" | "warn" | "error";
  text: string;
}
export interface LiffDevSession {
  label: string;
  liffId: string;
  chatId: string | null;
  mode: "client" | "web";
  reason: string | null;
  url: string;
  info: {
    requestedUrl?: string;
    endpoint?: string;
    appName?: string | null;
    viewType?: string | null;
    scopes?: string[] | null;
    expiresIn?: number | null;
    tokens?: Record<string, string>;
  };
  openedAt: number;
  closed: boolean;
  events: LiffDevEvent[];
}

/** A flex or template datetime picker button, as the message has it. */
export interface PickerAction {
  data: string;
  mode: "date" | "time" | "datetime";
  initial: string | null;
  max: string | null;
  min: string | null;
}

/** How LINE would show a picker for a button: `start` and the bounds are in the mode's format. */
export interface PickerStart {
  show: boolean;
  start: string;
  min: string | null;
  max: string | null;
}
