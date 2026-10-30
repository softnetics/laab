import { Dialog } from "@base-ui/react/dialog";
import type { ReactNode, RefObject } from "react";

/** A centred modal on the list background, like a macOS sheet. */
export function Sheet({
  open,
  onClose,
  children,
  className = "",
  title,
  initialFocus,
  instant = false,
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  title?: string;
  /** What gets focus on open; Base UI's default is the first focusable. */
  initialFocus?: RefObject<HTMLElement | null>;
  /** No open/close animation, for sheets opened by a keyboard shortcut. */
  instant?: boolean;
}) {
  const fade = instant ? "" : "transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0";
  const pop = instant
    ? ""
    : "transition-[scale,opacity] duration-150 ease-out data-ending-style:scale-[0.97] data-ending-style:opacity-0 data-starting-style:scale-[0.97] data-starting-style:opacity-0 motion-reduce:data-ending-style:scale-100 motion-reduce:data-starting-style:scale-100";
  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className={`fixed inset-0 z-40 bg-black/25 ${fade}`} />
        <Dialog.Popup
          initialFocus={initialFocus}
          className={`fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100vh-48px)] max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl bg-list text-primary shadow-[0_20px_60px_rgba(0,0,0,0.35)] ring-1 ring-black/10 outline-none ${pop} ${className}`}
        >
          {title && <Dialog.Title className="px-5 pt-4 text-[15px] font-semibold">{title}</Dialog.Title>}
          {children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export const buttonClass = (primary = false) =>
  `h-8 rounded-md px-4 text-[13px] font-medium ${primary ? "bg-line-green text-white disabled:opacity-50" : "bg-field text-primary"}`;
