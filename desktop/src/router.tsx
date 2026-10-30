import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
  type ErrorComponentProps,
  Outlet,
} from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { AlbumsNotesWindow, MediaWindow } from "./components/AuxWindows";
import { ChatWindow } from "./components/ChatWindow";
import { LiffDevTools } from "./components/LiffDevTools";
import { MainLayout } from "./components/MainLayout";
import { buttonClass } from "./components/Sheet";
import { Viewer } from "./components/Viewer";
import { useSession } from "./lib/api";

/** When the session ends (logout, expiry, kicked out), the main window shows
 *  the login screen; chat, album and viewer windows close. */
function CloseWhenSignedOut() {
  const signedOut = useSession()?.state === "signed_out";
  useEffect(() => {
    const win = getCurrentWindow();
    if (signedOut && win.label !== "main") win.close();
  }, [signedOut]);
  return null;
}

const root = createRootRoute({
  component: () => (
    <>
      <CloseWhenSignedOut />
      <Outlet />
    </>
  ),
});

const home = createRoute({ getParentRoute: () => root, path: "/", component: () => <MainLayout chatId={null} /> });

const chat = createRoute({
  getParentRoute: () => root,
  path: "/chat/$chatId",
  component: function Chat() {
    const { chatId } = chat.useParams();
    return <MainLayout chatId={chatId} />;
  },
});

const detached = createRoute({
  getParentRoute: () => root,
  path: "/window/$chatId",
  component: function Detached() {
    const { chatId } = detached.useParams();
    return <ChatWindow chatId={chatId} />;
  },
});

const tabSearch = (s: Record<string, unknown>) => ({
  tab: typeof s.tab === "string" ? s.tab : "",
  post: typeof s.post === "string" || typeof s.post === "number" ? String(s.post) : "",
  at: typeof s.at === "string" ? s.at : "",
});

const albums = createRoute({
  getParentRoute: () => root,
  path: "/albums/$chatId",
  validateSearch: tabSearch,
  component: function AlbumsRoute() {
    const { chatId } = albums.useParams();
    const { tab, post, at } = albums.useSearch();
    return <AlbumsNotesWindow chatId={chatId} tab={tab === "notes" ? "notes" : "albums"} post={post || null} at={at} />;
  },
});

const media = createRoute({
  getParentRoute: () => root,
  path: "/media/$chatId",
  validateSearch: tabSearch,
  component: function MediaRoute() {
    const { chatId } = media.useParams();
    const { tab } = media.useSearch();
    return <MediaWindow chatId={chatId} initial={tab === "files" || tab === "links" ? tab : "media"} />;
  },
});

const viewer = createRoute({ getParentRoute: () => root, path: "/viewer", component: Viewer });

const liffDevtools = createRoute({ getParentRoute: () => root, path: "/liff-devtools", component: LiffDevTools });

// Keep query values as strings. The default parses them as JSON, which turns
// LINE's 19-digit note ids into floats and loses their last digits.
const parseSearch = (search: string) => Object.fromEntries(new URLSearchParams(search.replace(/^\?/, "")));
const stringifySearch = (search: Record<string, unknown>) => {
  const q = new URLSearchParams(
    Object.entries(search)
      .filter(([, v]) => v != null && v !== "")
      .map(([k, v]) => [k, String(v)]),
  ).toString();
  return q ? `?${q}` : "";
};

/** A render error in any window: say so and offer a reload, instead of the
 *  router's developer error screen. */
function RenderError({ error }: ErrorComponentProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
      <div className="text-[15px] font-semibold">หน้าต่างนี้แสดงผลไม่ได้</div>
      <p className="selectable max-w-96 text-[12.5px] break-words text-secondary">{error instanceof Error ? error.message : String(error)}</p>
      <button className={buttonClass(true)} onClick={() => location.reload()}>
        โหลดใหม่
      </button>
    </div>
  );
}

export const router = createRouter({
  routeTree: root.addChildren([home, chat, detached, albums, media, viewer, liffDevtools]),
  defaultErrorComponent: RenderError,
  history: createHashHistory(),
  parseSearch,
  stringifySearch,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
