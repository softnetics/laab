import { useSyncExternalStore } from "react";

/** LAAB on an iPhone (or Android). `localStorage.laabPhone = "1"` shows the
 *  phone layout in the desktop app too, for checking it without a simulator. */
export const phone = /iPhone|iPod|Android/i.test(navigator.userAgent) || localStorage.getItem("laabPhone") === "1";

/** iOS draws the tab bar natively (see the native-tabbar plugin). */
export const ios = /iPhone|iPad|iPod/.test(navigator.userAgent);

/** iPhone Duo unfolded (669 pt) and phones in landscape get two panes; the
 *  cover screen (466 pt) and other phones get one. */
const WIDE = "(min-width: 600px)";

const wideQuery = window.matchMedia(WIDE);
const subscribe = (on: () => void) => {
  wideQuery.addEventListener("change", on);
  return () => wideQuery.removeEventListener("change", on);
};
export const useWide = () => useSyncExternalStore(subscribe, () => wideQuery.matches);

/** WKWebView keeps its layout size when the keyboard opens and scrolls the
 *  page instead. Size the app to the visible part and keep it in place, and
 *  mark the root while the keyboard is up so bottom bars drop the home
 *  indicator inset. */
export function fitPhoneViewport() {
  const root = document.documentElement;
  root.classList.add("phone");
  const vv = window.visualViewport;
  if (!vv) return;
  const fit = () => {
    root.style.setProperty("--app-height", `${vv.height}px`);
    root.classList.toggle("keyboard", window.innerHeight - vv.height > 120);
    if (window.scrollY !== 0) window.scrollTo(0, 0);
  };
  vv.addEventListener("resize", fit);
  vv.addEventListener("scroll", fit);
  fit();
}
