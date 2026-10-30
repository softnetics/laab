import { Popover as P } from "@base-ui/react/popover";
import { useEffect, type ReactElement, type ReactNode } from "react";
import { pushDismiss } from "../lib/ui";
import { popoverClass } from "./kit";

/** A controlled popover that Escape closes before anything else. */
export function Popover({
  open,
  onOpenChange,
  trigger,
  anchor,
  children,
  side = "top",
  align = "center",
  className = "",
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The element that opens it; omit and pass `anchor` to open from code. */
  trigger?: ReactElement;
  anchor?: Element | null;
  children: ReactNode;
  side?: "top" | "bottom" | "left" | "right";
  align?: "start" | "center" | "end";
  className?: string;
}) {
  useEffect(() => (open ? pushDismiss(() => onOpenChange(false)) : undefined), [open, onOpenChange]);
  return (
    <P.Root open={open} onOpenChange={onOpenChange}>
      {trigger && <P.Trigger render={trigger} />}
      <P.Portal>
        <P.Positioner side={side} align={align} sideOffset={6} anchor={anchor ?? undefined} className="z-50">
          <P.Popup className={`${popoverClass} ${className}`}>{children}</P.Popup>
        </P.Positioner>
      </P.Portal>
    </P.Root>
  );
}
