import { ContextMenu } from "@base-ui/react/context-menu";
import { Menu } from "@base-ui/react/menu";
import type { KeyboardEvent, ReactNode } from "react";

// Shared looks for Base UI menus and popovers.

// LINE for Mac menus: #3F3F3F, full-width gray hover rows about 25pt tall.
export const popupClass =
  "min-w-44 rounded-[10px] bg-menu py-1.5 text-[13.5px] text-menu-text shadow-[0_8px_30px_rgba(0,0,0,0.35)] ring-1 ring-black/30 outline-none origin-[var(--transform-origin)] transition-[scale,opacity] duration-100 data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0";

export const itemClass =
  "flex cursor-default items-center gap-2 px-4 py-[4px] leading-[17px] outline-none select-none data-highlighted:bg-menu-hover data-disabled:opacity-40";

export const popoverClass =
  "rounded-xl bg-popover text-primary shadow-[0_10px_34px_rgba(0,0,0,0.25)] ring-1 ring-black/10 outline-none origin-[var(--transform-origin)] transition-[scale,opacity] duration-120 data-starting-style:scale-95 data-starting-style:opacity-0 data-ending-style:scale-95 data-ending-style:opacity-0";

export const Separator = () => <div className="my-1.5 h-px bg-black/25" />;

export function CtxItem({ onClick, children, disabled }: { onClick?: () => void; children: ReactNode; disabled?: boolean }) {
  return (
    <ContextMenu.Item className={itemClass} onClick={onClick} disabled={disabled}>
      {children}
    </ContextMenu.Item>
  );
}

export function MenuItem({ onClick, children, disabled }: { onClick?: () => void; children: ReactNode; disabled?: boolean }) {
  return (
    <Menu.Item className={itemClass} onClick={onClick} disabled={disabled}>
      {children}
    </Menu.Item>
  );
}

/** A right-click menu around `children`. */
export function Ctx({ children, menu, className }: { children: ReactNode; menu: ReactNode; className?: string }) {
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger className={className}>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Positioner className="z-50 outline-none">
          <ContextMenu.Popup className={popupClass}>{menu}</ContextMenu.Popup>
        </ContextMenu.Positioner>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

export function Badge({ count, className = "" }: { count: number; className?: string }) {
  if (count <= 0) return null;
  return (
    <span
      className={`inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-line-green px-1.5 text-[11px] font-bold text-white ${className}`}
    >
      {count > 999 ? "999+" : count}
    </span>
  );
}

/** Click, Return and Space for an element that cannot be a `<button>`
 *  because it holds buttons of its own. */
export const pressable = (onPress: () => void) => ({
  role: "button",
  tabIndex: 0,
  onClick: onPress,
  onKeyDown: (e: KeyboardEvent) => {
    if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onPress();
    }
  },
});

export const Spinner = ({ size = 14 }: { size?: number }) => (
  <span
    className="inline-block animate-spin rounded-full border-2 border-secondary/40 border-t-secondary"
    style={{ width: size, height: size }}
  />
);
