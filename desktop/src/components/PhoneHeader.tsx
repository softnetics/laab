import type { ReactNode } from "react";
import { ChevronIcon } from "./Icons";

/** A page title with an optional back button, under the status bar. */
export function PhoneHeader({
  title,
  onBack,
  back = "",
  right,
}: {
  title: ReactNode;
  onBack?: () => void;
  back?: string;
  right?: ReactNode;
}) {
  return (
    <header className="safe-top-clear shrink-0">
      <div className="relative flex h-[52px] items-center px-2">
        {onBack && (
          <button onClick={onBack} className="z-10 flex h-11 min-w-11 items-center gap-0.5 px-1 text-[16px] text-primary">
            <ChevronIcon dir="left" size={22} />
            {back}
          </button>
        )}
        <h1 className="pointer-events-none absolute inset-x-16 truncate text-center text-[17px] font-semibold">{title}</h1>
        <div className="ml-auto flex items-center">{right}</div>
      </div>
    </header>
  );
}
