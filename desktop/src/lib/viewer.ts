import { api, errorText, mediaUrl } from "./api";
import type { Message } from "./types";
import { notice } from "./ui";

/** One photo or video in the viewer window. */
export interface ViewerItem {
  /** The full-size photo, or the video itself. */
  src: string;
  /** Shown while `src` loads. */
  thumb?: string;
  /** Plays as a video. Album and note videos only have a still. */
  video?: boolean;
  /** A chat attachment: save, save as and open with use it. */
  chatId?: string;
  messageId?: string;
  fileName?: string | null;
  /** A file already on disk. */
  path?: string;
  /** Who sent it and when, shown above the photo. */
  senderId?: string | null;
  mine?: boolean;
  time?: number;
}

export interface ViewerPayload {
  items: ViewerItem[];
  index: number;
}

export function messageItem(chatId: string, m: Message): ViewerItem {
  const video = m.contentType === "VIDEO";
  return {
    src: mediaUrl.message(chatId, m.id, false),
    thumb: m.imagePath && !video ? mediaUrl.local(m.imagePath) : mediaUrl.message(chatId, m.id, true),
    video,
    chatId,
    messageId: m.id,
    fileName: m.fileName,
    senderId: m.senderId,
    mine: m.mine,
    time: m.createdTime,
  };
}

export function openViewer(items: ViewerItem[], index: number) {
  api.openViewer({ items, index }).catch((e) => notice(errorText(e)));
}

/** Open a chat photo or video, with the chat's other ones a step away. */
export async function viewMessage(chatId: string, m: Message) {
  const synced = await api.chatMedia(chatId, "media").catch(() => [] as Message[]);
  const items = synced
    .slice()
    .sort((a, b) => a.createdTime - b.createdTime)
    .map((x) => messageItem(chatId, x));
  let index = items.findIndex((i) => i.messageId === m.id);
  if (index < 0) {
    items.push(messageItem(chatId, m));
    index = items.length - 1;
  }
  openViewer(items, index);
}
