import { addPluginListener, invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { setUi } from "./ui";

export interface NativeTab {
  id: string;
  title: string;
  /** SF Symbol names. */
  icon: string;
  selectedIcon?: string;
  badge?: string | null;
}

/** A button in the vertical bar; tabs set `selected`. */
export interface SideButton {
  id: string;
  icon: string;
  selectedIcon?: string;
  selected?: boolean;
  badge?: string | null;
}

/** A menu item; the native menu gets it without `run`, and answers with an
 *  `action` of `menu:<id>`. */
export interface MenuEntry {
  id: string;
  title: string;
  /** SF Symbol name, for the native menu. */
  icon?: string;
  destructive?: boolean;
  disabled?: boolean;
  /** A checkmark item; left out for a plain one. */
  checked?: boolean;
  children?: MenuEntry[][];
  run?: () => void;
}

export const findEntry = (sections: MenuEntry[][], id: string): MenuEntry | undefined => {
  for (const e of sections.flat()) {
    const hit = e.id === id ? e : e.children && findEntry(e.children, id);
    if (hit) return hit;
  }
};

export interface NativeTabState {
  tabs: NativeTab[];
  selected: string;
  /** Hides both bars. */
  hidden: boolean;
  /** Hides only the bottom tab bar, for a chat pushed over the list. */
  bottomHidden?: boolean;
  /** Bar button groups for the vertical bar, the top ones first (back,
   *  then actions). Used where iOS has one (the iPhone Duo). */
  side?: { top: SideButton[][]; bottom: SideButton[][] };
  /** The page's menu, for the system overflow menu in the vertical bar.
   *  `run` does not cross to iOS; JSON drops it. */
  menu?: MenuEntry[][];
}

export interface Fold {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A foldable's hinge; `angle` is in degrees, 0 when closed. */
export interface Hinge {
  status: "closed" | "partial" | "open" | "unknown";
  angle: number;
}

interface Layout {
  edge?: "leading" | "trailing";
  /** The hinge, in CSS pixels of the page, while the device is partly open. */
  fold?: Fold;
  /** The top inset where nothing covers the page; less than the safe area on
   *  the iPhone Duo's inner screen, whose status bar and camera share one
   *  top corner. */
  topClear?: number;
}

/** Show iOS's own bars with this state: a tab bar, and where the system
 *  has a vertical bar (iPhone Duo) the `side` buttons and `menu` in it, with
 *  `ui.sideBar` naming its edge. The tab bar is in the page's bottom safe
 *  area, so pages pad with `env(safe-area-inset-bottom)`; headers start at
 *  `--safe-top-clear` (see index.css). */
export function useNativeTabbar(
  state: NativeTabState | null,
  handlers: { onSelect: (id: string) => void; onAction: (id: string) => void },
) {
  const [layout, setLayout] = useState<Layout>({});
  const on = useRef(handlers);
  on.current = handlers;
  const key = JSON.stringify(state);
  useEffect(() => {
    if (!state) return;
    invoke<Layout>("plugin:native-tabbar|update", { ...state })
      .then(setLayout)
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => {
    const root = document.documentElement.style;
    if (layout.topClear == null) root.removeProperty("--top-clear");
    else root.setProperty("--top-clear", `${layout.topClear}px`);
    setUi({ sideBar: layout.edge ?? null, fold: layout.fold ?? null, statusCorner: layout.topClear === 0 });
  }, [layout]);
  useEffect(() => {
    if (!state) return;
    const listeners = [
      addPluginListener<{ id: string }>("native-tabbar", "select", (e) => on.current.onSelect(e.id)),
      addPluginListener<{ id: string }>("native-tabbar", "action", (e) => on.current.onAction(e.id)),
      // Folding, unfolding or rotating moved or removed the vertical bar or
      // the fold.
      addPluginListener<Layout>("native-tabbar", "layout", setLayout),
      addPluginListener<Hinge>("native-tabbar", "hinge", (hinge) => setUi({ hinge })),
    ];
    return () => listeners.forEach((l) => l.then((x) => x.unregister()));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [!!state]);
}
