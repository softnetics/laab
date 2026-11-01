import { createStore, useStore } from "@tanstack/react-store";
import type { Fold, Hinge } from "./nativeTabbar";
import type { Message, SearchKind, Workspace } from "./types";
import type { ViewerPayload } from "./viewer";

export type ChatFilter = "all" | "friends" | "groups" | "official" | `folder:${string}`;
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
  /** The "new folder" dialog; `chatId` is added to the folder once made.
   *  With `rename`, it renames that folder instead. */
  folderDialog: { chatId: string | null; rename?: Workspace } | null;
  /** Folders open in the chat list. */
  openFolders: string[];
  /** The folder whose name is being typed in the chat list. */
  renamingFolder: string | null;
  /** The composer is replying to, or editing, a message. */
  composerMode: { chatId: string; kind: "reply" | "edit"; message: Message } | null;
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
  sheet: { kind: "add-friend" } | { kind: "report"; messageId: string | null } | null;
  /** Someone's profile card; name and picture show while it loads. */
  profile: { mid: string; name?: string; picture?: string | null } | null;
  /** What the column next to the left bar shows: chats, friends, add friend. */
  railPage: "chats" | "friends" | "add" | "more";
  /** ⌘K's results kept in the list column of the unfolded iPhone Duo, so
   *  each hit opens beside them; `index` is the hit shown in the chat. */
  searchDock: { query: string; kinds: SearchKind[]; index: number } | null;
  /** The phone's photo/video viewer; the desktop opens a window instead. */
  gallery: ViewerPayload | null;
  /** The phone's file viewer (photos, videos, PDFs, other files). */
  viewer: { path: string; chatId?: string; messageId?: string; name?: string | null } | null;
  /** iOS shows a vertical bar at this edge (the iPhone Duo's cover screen);
   *  the native plugin draws the tabs and chat buttons there. */
  sideBar: "leading" | "trailing" | null;
  /** The iPhone Duo's hinge while it is partly open, in CSS pixels. */
  fold: Fold | null;
  /** The status bar sits in a top corner beside the camera (the iPhone Duo's
   *  inner screen) instead of across the top. */
  statusCorner: boolean;
  /** Bumped by the vertical bar's search button to open in-chat search. */
  chatSearch: number;
  /** The chat's ⋮ menu, opened from the vertical bar. */
  chatMenuOpen: boolean;
  /** The chat's card drawn for a moment for the long-press menu to show
   *  (see `answerChatMenu`). */
  menuCard: string | null;
  /** The chat shown beside the list without telling LINE it was read. */
  preview: string | null;
  /** The chat picked on the iPhone Duo's cover screen, shown once the device
   *  is opened a little and opened for real (read) when it is opened flat. */
  peek: string | null;
  /** A foldable's hinge, from iOS; null elsewhere. */
  hinge: Hinge | null;
  /** When this app last sent a message; that message scrolls the chat down. */
  sentAt: number;
  /** The announcement list over the chat is open. */
  announcementsOpen: boolean;
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
  folderDialog: null,
  openFolders: saved("openFolders", []),
  renamingFolder: null,
  liffConsent: null,
  sharePicker: null,
  announcementsOpen: false,
  sentAt: 0,
  railPage: "chats",
  composerMode: null,
  translations: {},
  confirm: null,
  sheet: null,
  searchDock: null,
  gallery: null,
  viewer: null,
  sideBar: null,
  fold: null,
  statusCorner: false,
  chatSearch: 0,
  chatMenuOpen: false,
  menuCard: null,
  preview: null,
  peek: null,
  hinge: null,
  profile: null,
});

export const useUi = <T>(select: (s: UiState) => T) => useStore(ui, select);

export function setUi(patch: Partial<UiState>) {
  ui.setState((s) => ({ ...s, ...patch }));
  if (patch.listWidth != null) localStorage.setItem("chatListWidth", JSON.stringify(patch.listWidth));
  if (patch.openFolders != null) localStorage.setItem("openFolders", JSON.stringify(patch.openFolders));
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
