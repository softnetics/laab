import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setUi, ui } from "./ui";

// app.windows[0].minWidth and width in tauri.conf.json.
const MIN_WIDTH = 760;
const DEFAULT_WIDTH = 1100;

/** How the window looked before it shrank to the chat list. */
let before: { width: number; rail: number; list: number } | null = null;

/** The page background as `[r, g, b]`, for the window behind the web view. */
function background(): [number, number, number] {
  const [r = 0, g = 0, b = 0] = getComputedStyle(document.body).backgroundColor.match(/\d+/g)?.map(Number) ?? [];
  return [r, g, b];
}

const resize = (width: number, minWidth: number, release: boolean) =>
  invoke("resize_window", { width, minWidth, background: background(), release });

/** Resolves after the browser has painted the current state. */
const painted = () => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));

/** Fold the conversation pane away and slide the window down to the rail and
 *  the chat list (`rail` + `list` wide), the way LINE's own button does. Both
 *  panes keep their widths while the window closes over the conversation, so
 *  nothing reflows mid-animation. A full-screen or maximized window keeps its
 *  size; the chat list fills it instead. */
export async function foldChat(rail: number, list: number) {
  const win = getCurrentWindow();
  if ((await win.isFullscreen()) || (await win.isMaximized())) {
    setUi({ chatFolded: true });
    return;
  }
  const width = rail + list;
  before = { width: window.innerWidth, rail, list };
  setUi({ foldFrame: { list, chat: window.innerWidth - width } });
  try {
    // The web view stays wide through the slide. Drop the conversation pane
    // (the list keeps its width) before fitting it to the window, so the page
    // is never wider than the web view showing it.
    await resize(width, width, false);
    setUi({ chatFolded: true });
    await painted();
    await invoke("release_web_view");
  } finally {
    setUi({ chatFolded: true, foldFrame: null });
  }
}

/** The window state plugin brings a window back at the size it closed at,
 *  so one closed while folded opens narrower than the unfolded minimum. Open
 *  it folded too. */
export function foldIfNarrow() {
  if (window.innerWidth < MIN_WIDTH) setUi({ chatFolded: true });
}

/** What to unfold to when the window opened folded: the default width. */
function openedFolded() {
  if (window.innerWidth >= MIN_WIDTH) return null;
  const rail = (document.querySelector("[data-rail]")?.getBoundingClientRect().width ?? 0) + 2;
  return { width: DEFAULT_WIDTH, rail, list: window.innerWidth - rail };
}

/** Bring the conversation pane back: lay it out at its final width first,
 *  then slide the window open over it. */
export async function unfoldChat() {
  if (!ui.state.chatFolded) return;
  const was = before ?? openedFolded();
  before = null;
  if (!was) {
    setUi({ chatFolded: false });
    return;
  }
  const width = Math.max(was.width, MIN_WIDTH);
  setUi({ chatFolded: false, foldFrame: { list: was.list, chat: width - was.rail - was.list } });
  try {
    await resize(width, MIN_WIDTH, true);
  } finally {
    setUi({ foldFrame: null });
  }
}
