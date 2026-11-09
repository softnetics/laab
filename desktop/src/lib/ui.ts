import { createStore, useStore } from "@tanstack/react-store";
import type { Forwarding } from "../components/ForwardSheet";
import type { FoundVia, Message, PickerAction, PickerStart, Workspace } from "./types";

/** `requests`: LINE's message request box, which the other lists leave out. */
export type ChatFilter = "all" | "friends" | "groups" | "official" | "requests" | `folder:${string}`;
export type ChatSort = "recent" | "unread";

const saved = <T>(key: string, fallback: T): T => {
  try {
    const raw = localStorage.getItem(key);
    return raw == null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
};

export interface UiState {
  focusMessageId: string | null;
  /** The conversation pane is folded away and the chat list fills the window. */
  chatFolded: boolean;
  /** Fixed pane widths while the window slides between folded and unfolded. */
  foldFrame: { list: number; chat: number } | null;
  listWidth: number;
  filter: ChatFilter;
  sort: ChatSort;
  listQuery: string;
  searchOpen: boolean;
  searchInitial: string;
  developerMode: boolean;
  /** Chat ids in list order, for ⌥↑ / ⌥↓. */
  visibleChatIds: string[];
  /** A short status line from the frontend (errors, progress). */
  notice: string | null;
  inspectMessageId: string | null;
  /** Bumped to move keyboard focus to the composer. */
  composerFocus: number;
  /** A group member selected from a message sender label for @-mentioning. */
  mentionRequest: { chatId: string; mid: string; name: string } | null;
  /** The "new folder" dialog; `chatId` is added to the folder once made.
   *  With `rename`, it renames that folder instead. */
  folderDialog: { chatId: string | null; rename?: Workspace } | null;
  /** Folders the user closed; all others, and new ones, show open. */
  collapsedFolders: string[];
  /** The folder whose name is being typed in the chat list. */
  renamingFolder: string | null;
  /** The composer is replying to, or editing, a message. */
  composerMode: { chatId: string; kind: "reply" | "edit"; message: Message } | null;
  /** A message or contact card being sent on to other chats. */
  forwarding: Forwarding | null;
  /** Translations shown under messages, by message id. */
  translations: Record<string, string>;
  /** A confirmation dialog. */
  confirm: {
    title: string;
    body?: string;
    action: string;
    danger?: boolean;
    run: () => Promise<unknown>;
    /** Called when the dialog closes without `run`. */
    dismiss?: () => void;
    /** The label of the close button, "ปิด" by default. */
    dismissLabel?: string;
  } | null;
  /** Open sheets: add friend, notes and albums of a chat. */
  sheet:
    | { kind: "add-friend" }
    | { kind: "report"; messageId: string | null }
    | { kind: "mcp" }
    | { kind: "bookmarks" }
    | { kind: "create-group" }
    | { kind: "group-members"; chatId: string }
    | { kind: "datetime-picker"; chatId: string; messageId: string; label: string | null; action: PickerAction; start: PickerStart }
    | null;
  /** Someone's profile card; name and picture show while it loads. `via`
   *  is where they were found, for adding them as a friend. */
  profile: { mid: string; name?: string; picture?: string | null; via?: FoundVia } | null;
  /** What the column next to the left bar shows: chats, friends, add friend. */
  railPage: "chats" | "friends" | "add";
  /** When this app last sent a message; that message scrolls the chat down. */
  sentAt: number;
  /** The announcement list over the chat is open. */
  announcementsOpen: boolean;
  /** The personal bookmarks strip over the chat is expanded. */
  bookmarksOpen: boolean;
  /** A LIFF app is waiting for the user to allow access. */
  liffConsent: { url: string; chatId: string | null } | null;
  /** shareTargetPicker request from a LIFF window. */
  sharePicker: { window: string; liffId: string; ott: string; data: { messages?: unknown[]; isMultiple?: boolean } } | null;
}

export const ui = createStore<UiState>({
  focusMessageId: null,
  chatFolded: false,
  foldFrame: null,
  listWidth: saved("chatListWidth", 320),
  filter: "all",
  sort: "recent",
  listQuery: "",
  searchOpen: false,
  searchInitial: "",
  developerMode: saved("developerMode", false),
  visibleChatIds: [],
  notice: null,
  inspectMessageId: null,
  composerFocus: 0,
  mentionRequest: null,
  folderDialog: null,
  collapsedFolders: saved("collapsedFolders", []),
  renamingFolder: null,
  liffConsent: null,
  sharePicker: null,
  announcementsOpen: false,
  bookmarksOpen: false,
  sentAt: 0,
  railPage: "chats",
  composerMode: null,
  forwarding: null,
  translations: {},
  confirm: null,
  sheet: null,
  profile: null,
});

export const useUi = <T>(select: (s: UiState) => T) => useStore(ui, select);

export function setUi(patch: Partial<UiState>) {
  ui.setState((s) => ({ ...s, ...patch }));
  if (patch.listWidth != null) localStorage.setItem("chatListWidth", JSON.stringify(patch.listWidth));
  if (patch.collapsedFolders != null) localStorage.setItem("collapsedFolders", JSON.stringify(patch.collapsedFolders));
  if (patch.developerMode != null) localStorage.setItem("developerMode", JSON.stringify(patch.developerMode));
}

let noticeTimer: ReturnType<typeof setTimeout> | undefined;
export function notice(text: string) {
  setUi({ notice: text });
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => setUi({ notice: null }), 6000);
}

/** Popovers register how to close themselves, so Escape closes the newest
 *  one before it closes anything else. */
const dismissers: (() => void)[] = [];
export function pushDismiss(fn: () => void) {
  dismissers.push(fn);
  return () => {
    const i = dismissers.lastIndexOf(fn);
    if (i >= 0) dismissers.splice(i, 1);
  };
}
export function dismissTop(): boolean {
  const fn = dismissers.pop();
  if (!fn) return false;
  fn();
  return true;
}

export const LAST_CHAT_KEY = "lastChatID";
