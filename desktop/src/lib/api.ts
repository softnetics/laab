import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { QueryClient, queryOptions, useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { setUi } from "./ui";
import type { ViewerPayload } from "./viewer";
import type {
  AllCalendars,
  CalendarEvent,
  CalendarEventInput,
  Album,
  AnalyticsStatus,
  McpActivity,
  McpSettings,
  McpSetup,
  AlbumPhoto,
  ChatList,
  ChatMember,
  ContactProfile,
  Draft,
  MentionSpan,
  Message,
  NoteDetail,
  NotePost,
  Person,
  LiffDevSession,
  ReactionName,
  RichMenu,
  SearchHit,
  SearchKind,
  Session,
  Sticker,
  StickerPack,
  Workspace,
} from "./types";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false } },
});

/** Calls a command. A failure also goes to LAAB's log, so a problem report
 *  can show which command failed and why. */
function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  return tauriInvoke<T>(command, args).catch((e: unknown) => {
    tauriInvoke("log_error", { command, error: String(e) }).catch(() => {});
    throw e;
  });
}

/** Mark a send from this app, so the chat scrolls down to it. */
function sent<T>(p: Promise<T>): Promise<T> {
  setUi({ sentAt: Date.now() });
  return p;
}

export const api = {
  session: () => invoke<Session>("session"),
  loginQr: () => invoke<void>("login_qr"),
  logout: () => invoke<void>("logout"),
  chats: () => invoke<ChatList>("chats"),
  people: () => invoke<Record<string, Person>>("people"),
  messages: (chatId: string, limit: number) => invoke<Message[]>("messages", { chatId, limit }),
  /** Fetch the page of history before the oldest stored message; 0 at its start. */
  olderMessages: (chatId: string) => invoke<number>("older_messages", { chatId }),
  reads: (chatId: string) => invoke<Record<string, string>>("reads", { chatId }),
  members: (chatId: string, force = false) => invoke<ChatMember[]>("members", { chatId, force }),
  setFocus: (chatId: string | null, active: boolean) => invoke<void>("set_focus", { chatId, active }),
  openChat: (chatId: string) => invoke<void>("open_chat", { chatId }),
  markRead: (chatId: string) => invoke<void>("mark_read", { chatId }),
  refresh: (full = true) => invoke<void>("refresh", { full }),
  reloadChat: (chatId: string) => invoke<void>("reload_chat", { chatId }),
  /** A reqSeq for one message, sent again with each retry of it. */
  newReqSeq: () => invoke<number>("new_req_seq"),
  sendText: (chatId: string, text: string, mentions: MentionSpan[], replyTo: string | null = null, reqSeq: number | null = null) =>
    sent(invoke<Message>("send_text", { chatId, text, mentions, replyTo, reqSeq })),
  sendSticker: (chatId: string, sticker: Sticker) => sent(invoke<void>("send_sticker", { chatId, sticker })),
  stickers: () => invoke<StickerPack[]>("stickers"),
  /** Sends each file, going on past failures; resolves to the ones that failed. */
  sendFiles: (chatId: string, paths: string[]) => sent(invoke<{ path: string; error: string }[]>("send_files", { chatId, paths })),
  download: (chatId: string, messageId: string, fileName: string | null) => invoke<string>("download", { chatId, messageId, fileName }),
  react: (chatId: string, messageId: string, reaction: ReactionName | null) => invoke<void>("react", { chatId, messageId, reaction }),
  setMuted: (chatId: string, muted: boolean) => invoke<void>("set_muted", { chatId, muted }),
  setPinned: (chatId: string, workspaceId: string, pinned: boolean) => invoke<void>("set_pinned", { chatId, workspaceId, pinned }),
  addWorkspace: (name: string) => invoke<Workspace>("add_workspace", { name }),
  createFolder: (name: string, chatIds: string[], from: string | null) => invoke<Workspace>("create_folder", { name, chatIds, from }),
  moveToFolder: (chatId: string, from: string | null, to: string | null) => invoke<void>("move_to_folder", { chatId, from, to }),
  renameFolder: (id: string, name: string) => invoke<void>("rename_folder", { id, name }),
  deleteFolder: (id: string) => invoke<void>("delete_folder", { id }),
  search: (query: string, kinds: SearchKind[]) => invoke<SearchHit[]>("search", { query, kinds }),
  draft: (chatId: string) => invoke<Draft | null>("draft", { chatId }),
  setDraft: (chatId: string, draft: Draft) => invoke<void>("set_draft", { chatId, draft }),
  rawMessage: (chatId: string, messageId: string) => invoke<string>("raw_message", { chatId, messageId }),
  sendPostback: (chatId: string, messageId: string, data: string, displayText: string | null) =>
    invoke<void>("send_postback", { chatId, messageId, data, displayText }),
  openLiff: (req: { url: string; chatId?: string | null; consent?: boolean; web?: boolean }) => invoke<void>("open_liff", { req }),
  liffDevSessions: () => invoke<LiffDevSession[]>("liff_dev_sessions"),
  liffDevAction: (label: string, action: "reload" | "inspect" | "close" | "reopen" | "web") =>
    invoke<void>("liff_dev_action", { label, action }),
  liffDevEval: (label: string, code: string) => invoke<void>("liff_dev_eval", { label, code }),
  openLiffDevtools: (focus: string | null = null) => invoke<void>("open_liff_devtools", { focus }),
  liffShare: (window: string, liffId: string, ott: string, chatIds: string[], messages: unknown) =>
    invoke<{ sent: number; failed: string[] }>("liff_share", { window, liffId, ott, chatIds, messages }),
  openChatWindow: (chatId: string, title: string) => invoke<void>("open_chat_window", { chatId, title }),
  isDemo: () => invoke<boolean>("is_demo"),
  analyticsStatus: () => invoke<AnalyticsStatus>("analytics_status"),
  setAnalytics: (on: boolean) => invoke<void>("set_analytics", { on }),
  reportPreview: (req: { messageId: string | null; content: boolean; log: boolean }) => invoke<string>("report_preview", { req }),
  sendReport: (report: string) => invoke<void>("send_report", { report }),
  reportJsError: (name: string, stack: string) => invoke<void>("report_js_error", { name, stack }),
  announcements: (chatId: string) =>
    invoke<{ seq: string; text: string; link: string | null; creator: string | null; createdTime: number }[]>("announcements", { chatId }),
  richMenu: (chatId: string) => invoke<RichMenu | null>("rich_menu", { chatId }),
  loginPassword: (identifier: string, password: string) => invoke<void>("login_password", { identifier, password }),
  unsend: (messageId: string) => invoke<void>("unsend", { messageId }),
  editMessage: (chatId: string, messageId: string, text: string, mentions: MentionSpan[] = []) =>
    invoke<void>("edit_message", { chatId, messageId, text, mentions }),
  translate: (text: string, target = "th") => invoke<string>("translate", { text, target }),
  saveToKeep: (chatId: string, messageId: string) => invoke<void>("save_to_keep", { chatId, messageId }),
  hideChat: (chatId: string) => invoke<void>("hide_chat", { chatId }),
  deleteHistory: (chatId: string) => invoke<void>("delete_history", { chatId }),
  removeAnnouncement: (chatId: string, seq: number) => invoke<void>("remove_announcement", { chatId, seq }),
  announce: (chatId: string, messageId: string, text: string) => invoke<void>("announce", { chatId, messageId, text }),
  setBlocked: (mid: string, blocked: boolean) => invoke<void>("set_blocked", { mid, blocked }),
  findContact: (query: string) => invoke<{ mid: string; name: string; picture: string | null }>("find_contact", { query }),
  addFriend: (mid: string) => invoke<void>("add_friend", { mid }),
  contactProfile: (mid: string) => invoke<ContactProfile>("contact_profile", { mid }),
  joinGroupLink: (url: string) => invoke<string>("join_group_link", { url }),
  listNotes: (chatId: string) => invoke<NotePost[]>("list_notes", { chatId }),
  createNote: (chatId: string, text: string, photos: string[] = []) => invoke<void>("create_note", { chatId, text, photos }),
  noteDetail: (chatId: string, postId: string) => invoke<NoteDetail>("note_detail", { chatId, postId }),
  addComment: (chatId: string, postId: string, text: string) => invoke<void>("add_comment", { chatId, postId, text }),
  deleteNote: (chatId: string, postId: string) => invoke<void>("delete_note", { chatId, postId }),
  calendarEvents: (chatId: string) => invoke<CalendarEvent[]>("calendar_events", { chatId }),
  allCalendarEvents: () => invoke<AllCalendars>("all_calendar_events"),
  /** What a calendar window (a chat id, or ALL_CALENDARS) last loaded. */
  cachedCalendar: (chatId: string) => invoke<AllCalendars | null>("cached_calendar", { chatId }),
  createCalendarEvent: (chatId: string, event: CalendarEventInput, notify: boolean) =>
    invoke<void>("create_calendar_event", { chatId, event, notify }),
  updateCalendarEvent: (chatId: string, eventId: string, updatedAt: number, event: CalendarEventInput, notify: boolean) =>
    invoke<void>("update_calendar_event", { chatId, eventId, updatedAt, event, notify }),
  deleteCalendarEvent: (chatId: string, eventId: string, updatedAt: number, notify: boolean) =>
    invoke<void>("delete_calendar_event", { chatId, eventId, updatedAt, notify }),
  listAlbums: (chatId: string) => invoke<Album[]>("list_albums", { chatId }),
  albumPhotos: (chatId: string, albumId: string, limit?: number) =>
    invoke<AlbumPhoto[]>("album_photos", { chatId, albumId, limit: limit ?? null }),
  createAlbum: (chatId: string, title: string) => invoke<void>("create_album", { chatId, title }),
  renameAlbum: (chatId: string, albumId: string, title: string) => invoke<void>("rename_album", { chatId, albumId, title }),
  deleteAlbum: (chatId: string, albumId: string) => invoke<void>("delete_album", { chatId, albumId }),
  deleteMessage: (messageId: string) => invoke<void>("delete_message", { messageId }),
  leaveGroup: (chatId: string) => invoke<void>("leave_group", { chatId }),
  chatMedia: (chatId: string, kind: "media" | "files" | "links") => invoke<Message[]>("chat_media", { chatId, kind }),
  saveText: (path: string, text: string) => invoke<void>("save_text", { path, text }),
  saveAttachment: (chatId: string, messageId: string, dest: string | null = null) =>
    invoke<string>("save_attachment", { chatId, messageId, dest }),
  /** LINE-style separate windows: `albums` (tab albums/notes), `media`
   *  (tab media/links/files) or `calendar` (`post` is the day to open on). */
  openAuxWindow: (kind: "albums" | "media" | "calendar", chatId: string, title: string, tab?: string, post?: string) =>
    invoke<void>("open_aux_window", { kind, chatId, title, tab: tab ?? null, post: post ?? null }),
  /** The one photo/video viewer window; see `lib/viewer.ts`. */
  openViewer: (payload: ViewerPayload) => invoke<void>("open_viewer", { payload }),
  viewerPayload: () => invoke<ViewerPayload | null>("viewer_payload"),
  showMessage: (chatId: string, messageId: string) => invoke<void>("show_message", { chatId, messageId }),
  quit: () => invoke<void>("quit"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  cacheSize: () => invoke<number>("cache_size"),
  clearCache: () => invoke<void>("clear_cache"),
  mcpSettings: () => invoke<McpSettings>("mcp_settings"),
  setMcpSettings: (settings: McpSettings) => invoke<void>("set_mcp_settings", { settings }),
  mcpActivity: () => invoke<McpActivity[]>("mcp_activity"),
  clearMcpActivity: () => invoke<void>("clear_mcp_activity"),
  mcpSetup: () => invoke<McpSetup>("mcp_setup"),
};

/** How many stored messages a chat shows; scrolling up raises it. */
const limits = new Map<string, number>();
export const MESSAGE_PAGE = 400;
export const messageLimit = (chatId: string) => limits.get(chatId) ?? MESSAGE_PAGE;
export const setMessageLimit = (chatId: string, n: number) => limits.set(chatId, n);

export const q = {
  session: queryOptions({ queryKey: ["session"], queryFn: () => api.session().then(track) }),
  chats: queryOptions({ queryKey: ["chats"], queryFn: api.chats }),
  people: queryOptions({ queryKey: ["people"], queryFn: api.people }),
  messages: (chatId: string) =>
    queryOptions({ queryKey: ["messages", chatId], queryFn: () => api.messages(chatId, messageLimit(chatId)) }),
  reads: (chatId: string) => queryOptions({ queryKey: ["reads", chatId], queryFn: () => api.reads(chatId) }),
  members: (chatId: string) => queryOptions({ queryKey: ["members", chatId], queryFn: () => api.members(chatId), staleTime: 5 * 60_000 }),
  stickers: queryOptions({ queryKey: ["stickers"], queryFn: api.stickers, staleTime: 10 * 60_000 }),
  isDemo: queryOptions({ queryKey: ["demo"], queryFn: api.isDemo }),
  analytics: queryOptions({ queryKey: ["analytics"], queryFn: api.analyticsStatus }),
  mcp: queryOptions({ queryKey: ["mcp"], queryFn: api.mcpSettings }),
  mcpActivity: queryOptions({ queryKey: ["mcp-activity"], queryFn: api.mcpActivity }),
};

export const useSession = () => useQuery(q.session).data;

// From the moment there is no session until a login succeeds, the window is
// the login page, also while a QR or password login runs. Resuming a saved
// session at launch keeps the chats on screen instead.
let loginPage = false;
function track(s: Session) {
  if (s.state === "signed_out") loginPage = true;
  else if (s.state === "signed_in" || s.state === "demo") loginPage = false;
  return s;
}
export function useLoginPage() {
  useSession();
  return loginPage;
}
export const useChatList = () => useQuery(q.chats).data;
export const usePeople = () => useQuery(q.people).data ?? {};

/** Backend events invalidate the matching queries. */
export function useBackendEvents() {
  useEffect(() => {
    const offs = [
      listen<Session>("session", (e) => queryClient.setQueryData(["session"], track(e.payload))),
      listen("chats-changed", () => {
        queryClient.invalidateQueries({ queryKey: ["chats"] });
        queryClient.invalidateQueries({ queryKey: ["people"] });
      }),
      listen<{ chatId: string }>("messages-changed", (e) => queryClient.invalidateQueries({ queryKey: ["messages", e.payload.chatId] })),
      listen("mcp-changed", () => {
        queryClient.invalidateQueries({ queryKey: ["mcp"] });
        queryClient.invalidateQueries({ queryKey: ["mcp-setup"] });
      }),
      listen("mcp-activity", () => {
        queryClient.invalidateQueries({ queryKey: ["mcp-activity"] });
        queryClient.invalidateQueries({ queryKey: ["mcp-setup"] });
      }),
      listen<{ chatId: string }>("reads-changed", (e) => queryClient.invalidateQueries({ queryKey: ["reads", e.payload.chatId] })),
      listen<{ chatId: string }>("members-changed", (e) => {
        queryClient.invalidateQueries({ queryKey: ["members", e.payload.chatId] });
        queryClient.invalidateQueries({ queryKey: ["people"] });
      }),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  }, []);
}

const MEDIA = "media://localhost";

export const mediaUrl = {
  avatar: (mid: string, picture?: string | null, local?: string | null) =>
    local && !local.endsWith(".jpg")
      ? `${MEDIA}/local?path=${encodeURIComponent(local)}`
      : `${MEDIA}/avatar/${encodeURIComponent(mid)}${picture ? `?p=${encodeURIComponent(picture)}` : ""}`,
  message: (chatId: string, id: string, preview = true) =>
    `${MEDIA}/message/${encodeURIComponent(chatId)}/${encodeURIComponent(id)}${preview ? "?preview=1" : ""}`,
  local: (path: string) => `${MEDIA}/local?path=${encodeURIComponent(path)}`,
  sticker: (id: string, anim = true) => `${MEDIA}/sticker/${encodeURIComponent(id)}${anim ? "?anim=1" : ""}`,
  pack: (id: string) => `${MEDIA}/pack/${encodeURIComponent(id)}`,
  /** A profile cover photo (OBS `myhome/c`). */
  cover: (oid: string) => `${MEDIA}/cover/${encodeURIComponent(oid)}`,
  /** An album photo; the backend downloads it on first request. */
  album: (chatId: string, albumId: string, oid: string) =>
    `${MEDIA}/album/${encodeURIComponent(chatId)}/${encodeURIComponent(albumId)}/${encodeURIComponent(oid)}`,
  /** A note photo (`h`) or comment image (`cmt`). */
  note: (svc: "h" | "cmt", id: string) => `${MEDIA}/note/${svc}/${encodeURIComponent(id)}`,
};

/** Backend error codes that reach the UI as they are. */
const ERROR_CODES: Record<string, string> = {
  not_logged_in: "ยังไม่ได้เชื่อม LINE ลองอีกครั้งเมื่อเชื่อมแล้ว",
  message_not_found: "ไม่พบข้อความนี้",
  empty_message: "ข้อความว่าง",
  translate_empty: "ไม่มีข้อความให้แปล",
  empty_note: "โน้ตว่าง",
  empty_comment: "ความคิดเห็นว่าง",
  bad_reaction: "ใช้รีแอคชันนี้ไม่ได้",
  invalid_liff_id: "ลิงก์ LIFF ไม่ถูกต้อง",
};

export function errorText(e: unknown): string {
  const text = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  return ERROR_CODES[text] ?? text;
}
