import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  dayKey,
  dayLabel,
  durationLabel,
  minutesBetween,
  monthGrid,
  parseTime,
  thaiMonths,
  todayKey,
  weekdayLetters,
} from "../lib/calendar";
import { ChevronIcon } from "./Icons";
import { Popover } from "./Popover";

// Date and time fields drawn by LAAB rather than WebKit: the native pickers
// follow the Mac's calendar (Buddhist years, English month names) and look
// out of place next to the rest of the app.

const chipClass = (open: boolean) =>
  `h-[26px] rounded-[7px] px-2 text-[12.5px] tabular-nums outline-none transition-colors focus-visible:ring-2 focus-visible:ring-line-green/50 ${
    open ? "bg-line-green/12 text-line-green" : "bg-chat shadow-[0_0_0_0.5px_rgba(0,0,0,0.08)] hover:bg-row-hover"
  }`;

/** Day keys ("2026-11-04") shifted by `n` days. */
const shift = (key: string, n: number) => {
  const [y, m, d] = key.split("-").map(Number);
  const t = new Date(Date.UTC(y, m - 1, d + n));
  return dayKey(t.getUTCFullYear(), t.getUTCMonth(), t.getUTCDate());
};

/** The same day `n` months on, clamped to the month's length. */
const shiftMonth = (key: string, n: number) => {
  const [y, m, d] = key.split("-").map(Number);
  const last = new Date(Date.UTC(y, m - 1 + n + 1, 0)).getUTCDate();
  const t = new Date(Date.UTC(y, m - 1 + n, Math.min(d, last)));
  return dayKey(t.getUTCFullYear(), t.getUTCMonth(), t.getUTCDate());
};

export function DatePicker({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      align="end"
      trigger={
        <button type="button" aria-label={`${label} ${dayLabel(value)}`} className={chipClass(open)}>
          {dayLabel(value)}
        </button>
      }
    >
      {open && (
        <Month
          value={value}
          onPick={(v) => {
            onChange(v);
            setOpen(false);
          }}
        />
      )}
    </Popover>
  );
}

function Month({ value, onPick }: { value: string; onPick: (v: string) => void }) {
  // The focused day: arrows move it, Enter or a click picks it.
  const [cursor, setCursor] = useState(value);
  const [y, m] = cursor.split("-").map(Number);
  const grid = useRef<HTMLDivElement>(null);
  const today = todayKey();
  useLayoutEffect(() => {
    grid.current?.querySelector<HTMLButtonElement>(`[data-day="${cursor}"]`)?.focus();
  }, [cursor]);
  const keys = (e: KeyboardEvent) => {
    const move = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
    if (move) setCursor(shift(cursor, move));
    else if (e.key === "PageUp") setCursor(shiftMonth(cursor, -1));
    else if (e.key === "PageDown") setCursor(shiftMonth(cursor, 1));
    else return;
    e.preventDefault();
  };
  return (
    <div className="w-[236px] p-2.5 select-none">
      <div className="flex items-center gap-0.5 pb-1.5 pl-1">
        <span className="flex-1 text-[13px] font-semibold">
          {thaiMonths[m - 1]} <span className="font-normal text-secondary tabular-nums">{y}</span>
        </span>
        <button type="button" tabIndex={-1} aria-label="เดือนก่อน" onClick={() => setCursor(shiftMonth(cursor, -1))} className={arrowClass}>
          <ChevronIcon dir="left" size={13} />
        </button>
        <button type="button" tabIndex={-1} aria-label="เดือนถัดไป" onClick={() => setCursor(shiftMonth(cursor, 1))} className={arrowClass}>
          <ChevronIcon dir="right" size={13} />
        </button>
      </div>
      <div className="grid grid-cols-7 text-center text-[10px] font-medium text-secondary">
        {weekdayLetters.map((w, i) => (
          <div key={w} className={`pb-1 ${i === 0 ? "text-badge-red/75" : ""}`}>
            {w}
          </div>
        ))}
      </div>
      <div ref={grid} role="grid" aria-label={`${thaiMonths[m - 1]} ${y}`} onKeyDown={keys} className="grid grid-cols-7 gap-y-0.5">
        {monthGrid(y, m - 1).map((c) => {
          const picked = c.key === value;
          return (
            <button
              key={c.key}
              type="button"
              data-day={c.key}
              tabIndex={c.key === cursor ? 0 : -1}
              aria-pressed={picked}
              aria-label={dayLabel(c.key)}
              onClick={() => onPick(c.key)}
              className={`mx-auto grid h-[28px] w-[28px] place-items-center rounded-full text-[12.5px] tabular-nums outline-none transition-colors focus-visible:ring-2 focus-visible:ring-line-green/50 ${
                picked
                  ? "bg-line-green font-semibold text-white"
                  : c.key === today
                    ? "font-semibold text-line-green hover:bg-row-hover"
                    : c.inMonth
                      ? "hover:bg-row-hover"
                      : "text-secondary/50 hover:bg-row-hover"
              }`}
            >
              {c.day}
            </button>
          );
        })}
      </div>
      <div className="mt-1.5 flex justify-end border-t border-separator pt-1.5">
        <button
          type="button"
          onClick={() => onPick(today)}
          className="h-6 rounded-full px-2.5 text-[12px] font-medium text-line-green transition-colors hover:bg-line-green/10"
        >
          วันนี้
        </button>
      </div>
    </div>
  );
}

const arrowClass = "grid h-6 w-6 place-items-center rounded-full text-secondary transition-colors hover:bg-row-hover hover:text-primary";

/** "09:30" → "9.30", the Thai way LAAB writes times. */
export const timeText = (v: string) => {
  const [h, m] = v.split(":").map(Number);
  return `${h}.${String(m).padStart(2, "0")}`;
};

/** Every 15 minutes, plus `value` when it falls between them. */
export function timeSlots(value: string): string[] {
  const out = Array.from(
    { length: 96 },
    (_, i) => `${String(Math.floor(i / 4)).padStart(2, "0")}:${String((i % 4) * 15).padStart(2, "0")}`,
  );
  if (value && !out.includes(value)) out.push(value);
  return out.sort();
}

/** A time you can type ("930", "9.30", "บ่าย 2") or pick from a list under
 *  it. With `from` (the start, on the same day) the list starts there and
 *  says how long the event would run. */
export function TimeField({
  value,
  onChange,
  label,
  from,
}: {
  value: string;
  onChange: (v: string) => void;
  label: string;
  from?: string;
}) {
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [text, setText] = useState(timeText(value));
  // The highlighted option; -1 until the arrows or typing pick one.
  const [active, setActive] = useState(-1);
  const editing = open || document.activeElement === input.current;
  useEffect(() => {
    if (!editing) setText(timeText(value));
  }, [value, editing]);

  const all = timeSlots(value);
  const slots = from ? all.filter((t) => t >= from) : all;
  const typed = parseTime(text);
  const bad = text.trim() !== "" && !typed;

  // Keep the highlighted option, or the nearest to what's typed, in view.
  const target =
    active >= 0
      ? active
      : Math.max(
          0,
          slots.findIndex((t) => t >= (typed ?? value)),
        );
  useLayoutEffect(() => {
    if (!open) return;
    (list.current?.children[target] as HTMLElement | undefined)?.scrollIntoView({ block: active >= 0 ? "nearest" : "center" });
  }, [open, target, active]);

  const commit = (t: string | null) => {
    if (t) onChange(t);
    setText(timeText(t ?? value));
    setActive(-1);
    setOpen(false);
  };
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    const move = { ArrowDown: 1, ArrowUp: -1, PageDown: 4, PageUp: -4 }[e.key];
    if (move) {
      e.preventDefault();
      setOpen(true);
      // The first arrow after typing a new time lands on the nearest option; otherwise it steps.
      const step = active < 0 && typed && typed !== value ? 0 : move;
      setActive(Math.min(slots.length - 1, Math.max(0, target + step)));
    } else if (e.key === "Enter") {
      e.preventDefault();
      commit(active >= 0 ? slots[active] : typed);
    } else if (e.key === "Escape" && open) {
      // Close the list and undo the typing, but leave the form open.
      e.preventDefault();
      e.stopPropagation();
      setText(timeText(value));
      setActive(-1);
      setOpen(false);
    }
  };
  return (
    <>
      <input
        ref={input}
        value={text}
        aria-label={label}
        aria-invalid={bad}
        role="combobox"
        aria-expanded={open}
        aria-autocomplete="list"
        inputMode="numeric"
        spellCheck={false}
        autoComplete="off"
        onFocus={(e) => {
          e.currentTarget.select();
          setOpen(true);
        }}
        onClick={() => setOpen(true)}
        onBlur={() => commit(typed)}
        onChange={(e) => {
          setText(e.target.value);
          setActive(-1);
          setOpen(true);
        }}
        onKeyDown={keys}
        className={`h-[26px] w-[60px] rounded-[7px] px-2 text-center text-[12.5px] tabular-nums outline-none transition-colors ${
          bad
            ? "bg-badge-red/10 text-badge-red ring-1 ring-badge-red/40"
            : open
              ? "bg-chat ring-2 ring-line-green/50"
              : "bg-chat shadow-[0_0_0_0.5px_rgba(0,0,0,0.08)] hover:bg-row-hover"
        }`}
      />
      <Popover
        open={open}
        onOpenChange={(o) => {
          // Clicks in the field itself shouldn't close its own list.
          if (!o && document.activeElement === input.current) return;
          setOpen(o);
        }}
        anchor={input.current}
        side="bottom"
        align="end"
        keepFocus
      >
        <div
          ref={list}
          role="listbox"
          aria-label={label}
          className={`max-h-[232px] overflow-y-auto p-1 ${from ? "w-[156px]" : "w-[104px]"}`}
        >
          {slots.map((t, i) => {
            const chosen = t === value;
            return (
              <button
                key={t}
                type="button"
                role="option"
                tabIndex={-1}
                aria-selected={chosen}
                // Keep focus in the field so its blur doesn't beat the click.
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => commit(t)}
                className={`flex h-[26px] w-full items-center gap-2 rounded-[6px] px-2.5 text-[12.5px] tabular-nums outline-none ${
                  chosen
                    ? "bg-line-green font-semibold text-white"
                    : i === target && (active >= 0 || typed)
                      ? "bg-row-hover"
                      : "hover:bg-row-hover"
                }`}
              >
                {timeText(t)}
                {from && t !== from && (
                  <span className={`text-[11px] ${chosen ? "text-white/80" : "text-secondary"}`}>
                    {durationLabel(minutesBetween(from, t))}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </Popover>
    </>
  );
}
