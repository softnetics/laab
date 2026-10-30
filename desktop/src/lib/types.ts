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
}

export interface ChatMember {
  mid: string;
  name: string;
  all?: boolean;
  picture?: string | null;
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
  media: { objectId: string; video: boolean }[];
  text: string;
  createdTime: number;
  mediaCount: number;
  likeCount: number;
  commentCount: number;
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
