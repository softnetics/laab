import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { api, errorText, queryClient, useChatList, usePeople } from "../lib/api";
import {
  ALL_CALENDARS,
  agendaTimes,
  calendarName,
  byDay,
  eventDays,
  type EventForm,
  formOf,
  inputOf,
  localZone,
  longDayLabel,
  minutesBetween,
  monthGrid,
  newForm,
  shortMonths,
  thaiMonths,
  timeLabel,
  todayKey,
  weekdayLetters,
} from "../lib/calendar";
import { linkify } from "../lib/links";
import { openUrl } from "../lib/open";
import type { CalendarEvent } from "../lib/types";
import { notice, setUi } from "../lib/ui";
import { ConfirmDialog } from "./Dialogs";
import { BubbleIcon, CalendarIcon, ChevronIcon, ClockIcon, LinkIcon, PersonIcon, PlaceIcon, TextIcon } from "./Icons";
import { DatePicker, TimeField } from "./DateTimePicker";
import { Spinner, Switch } from "./kit";

// A chat's LINE Calendar, in its own window like albums and notes: a month,
// the chosen day's agenda, an event, and a form to add or edit one. Opened
// from the rail with ALL_CALENDARS, it shows every calendar at once, as
// LINE's Calendar tab does, and names each event's chat.

type View = { kind: "month" } | { kind: "event"; id: string } | { kind: "edit"; event: CalendarEvent | null };

/** A calendar the window can add to. */
type Target = { chatId: string; name: string };

// Both kinds of window share the "calendar" prefix, so an edit in one
// refreshes the other.
const refresh = () => queryClient.invalidateQueries({ queryKey: ["calendar"] });

/** `day` is the day to open on (a day key), from an event card's link;
 *  empty for today. */
export function CalendarWindow({ chatId, day: openOn, at }: { chatId: string; day: string; at: string }) {
  const all = chatId === ALL_CALENDARS;
  const chats = useChatList()?.chats;
  const chatName = (id: string) => chats?.find((c) => c.id === id)?.name;
  const title = all ? "" : (chatName(chatId) ?? "");
  useEffect(() => {
    getCurrentWindow()
      .setTitle(title ? `ปฏิทิน · ${title}` : "ปฏิทิน")
      .catch(() => {});
  }, [title]);
  // Events loaded earlier (ahead of time, or by the last window), shown
  // while fresh ones load.
  const { data: cached } = useQuery({
    queryKey: ["calendar-cache", chatId],
    queryFn: () => api.cachedCalendar(chatId),
    staleTime: Infinity,
  });
  const {
    data: loaded,
    isLoading,
    error,
  } = useQuery({
    queryKey: ["calendar", chatId],
    queryFn: () => (all ? api.allCalendarEvents() : api.calendarEvents(chatId).then((events) => ({ calendars: [], events }))),
    staleTime: 0,
    placeholderData: cached ?? undefined,
  });
  const data = loaded?.events;
  // The window's calendars by name, for the agenda and the add form; null
  // in a chat's own window, where every event is that chat's.
  const targets: Target[] | null = useMemo(
    () =>
      all
        ? (loaded?.calendars ?? [])
            .map((c) => ({ chatId: c.chatId, name: calendarName(c.chatId, chatName(c.chatId), c.name) }))
            .sort((a, b) => a.name.localeCompare(b.name, "th"))
        : null,
    [all, loaded, chats],
  );
  const nameOf = (id: string) => targets?.find((t) => t.chatId === id)?.name ?? calendarName(id, chatName(id));
  const [day, setDay] = useState(() => openOn || todayKey());
  const [view, setView] = useState<View>({ kind: "month" });
  // Opening the window again (a calendar link, the menu) goes to the link's
  // day, or back to today.
  useEffect(() => {
    setDay(openOn || todayKey());
    setView({ kind: "month" });
  }, [at, openOn]);

  const viewing = view.kind === "event" ? data?.find((e) => e.id === view.id) : undefined;
  const pane = view.kind === "edit" ? `edit-${view.event?.id ?? "new"}` : viewing ? `event-${viewing.id}` : "month";
  return (
    <div className="flex h-full flex-col bg-chat">
      <div className="drag h-[26px] shrink-0" data-tauri-drag-region />
      <div
        className="drag flex h-[26px] shrink-0 items-center justify-center px-16 text-[12.5px] font-semibold text-secondary"
        data-tauri-drag-region
      >
        <span className="truncate">{all ? "ทุกปฏิทิน" : title || "ปฏิทิน"}</span>
      </div>
      <div key={pane} className="flex min-h-0 flex-1 flex-col motion-safe:animate-pane-in">
        {view.kind === "edit" ? (
          <Editor
            chatId={chatId}
            targets={targets}
            event={view.event}
            day={day}
            onDone={(saved) => {
              if (saved) setDay(saved);
              setView(view.event ? { kind: "event", id: view.event.id } : { kind: "month" });
            }}
          />
        ) : viewing ? (
          <Detail
            source={all ? nameOf(viewing.chatId) : null}
            event={viewing}
            onBack={() => setView({ kind: "month" })}
            onEdit={() => setView({ kind: "edit", event: viewing })}
          />
        ) : (
          <Month
            events={data}
            loading={isLoading}
            error={error}
            day={day}
            onDay={setDay}
            source={all ? (e) => nameOf(e.chatId) : null}
            onOpen={(id) => setView({ kind: "event", id })}
            onAdd={() => setView({ kind: "edit", event: null })}
          />
        )}
      </div>
      <ConfirmDialog />
    </div>
  );
}

// -- month -------------------------------------------------------------------

const iconButton = "grid h-7 w-7 place-items-center rounded-full text-secondary transition-colors hover:bg-row-hover hover:text-primary";

function Month({
  events,
  loading,
  error,
  day,
  onDay,
  source,
  onOpen,
  onAdd,
}: {
  events: CalendarEvent[] | undefined;
  loading: boolean;
  error: unknown;
  day: string;
  onDay: (d: string) => void;
  /** The event's chat name, in the all-calendars window. */
  source: ((e: CalendarEvent) => string) | null;
  onOpen: (id: string) => void;
  onAdd: () => void;
}) {
  const [y, m] = day.split("-").map(Number);
  const [shown, setShown] = useState({ y, m: m - 1 });
  // Picking a day in the next month's spill-over moves the month with it.
  useEffect(() => setShown({ y, m: m - 1 }), [y, m]);
  const days = useMemo(() => byDay(events ?? []), [events]);
  const today = todayKey();
  const cells = monthGrid(shown.y, shown.m);
  const step = (n: number) => {
    const t = new Date(shown.y, shown.m + n, 1);
    setShown({ y: t.getFullYear(), m: t.getMonth() });
  };
  const [ty, tm] = today.split("-").map(Number);
  const atToday = day === today && shown.y === ty && shown.m === tm - 1;
  // All-day events first, then by start.
  const list = [...(days.get(day) ?? [])].sort((a, b) => Number(b.allDay) - Number(a.allDay) || a.start - b.start);
  const now = Date.now();

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 px-4 pt-1 pb-2">
        <h2 className="flex-1 text-[19px] font-semibold tracking-[-0.01em]">
          {thaiMonths[shown.m]} <span className="font-normal text-secondary tabular-nums">{shown.y}</span>
        </h2>
        {!atToday && (
          <button
            onClick={() => onDay(today)}
            className="mr-1 h-6 rounded-full border border-separator px-2.5 text-[11.5px] font-medium transition-colors hover:bg-row-hover"
          >
            วันนี้
          </button>
        )}
        <button aria-label="เดือนก่อน" onClick={() => step(-1)} className={iconButton}>
          <ChevronIcon dir="left" size={15} />
        </button>
        <button aria-label="เดือนถัดไป" onClick={() => step(1)} className={iconButton}>
          <ChevronIcon dir="right" size={15} />
        </button>
      </div>

      <div className="grid grid-cols-7 px-2 text-center text-[10.5px] font-medium text-secondary">
        {weekdayLetters.map((w, i) => (
          <div key={w} className={i === 0 ? "text-badge-red/75" : undefined}>
            {w}
          </div>
        ))}
      </div>
      <div className="grid grid-cols-7 px-2 pt-0.5 pb-2" aria-label={`${thaiMonths[shown.m]} ${shown.y}`}>
        {cells.map((c) => {
          const n = days.get(c.key)?.length ?? 0;
          const selected = c.key === day;
          const isToday = c.key === today;
          return (
            <button
              key={c.key}
              onClick={() => onDay(c.key)}
              aria-label={`${longDayLabel(c.key)}${n ? `, ${n} นัด` : ""}`}
              aria-pressed={selected}
              className="group flex h-[38px] flex-col items-center justify-center gap-[3px] outline-none"
            >
              <span
                className={`grid h-[26px] w-[26px] place-items-center rounded-full text-[13px] tabular-nums transition-colors group-focus-visible:ring-2 group-focus-visible:ring-line-green/50 ${
                  selected
                    ? isToday
                      ? "bg-line-green font-semibold text-white"
                      : "bg-primary font-semibold text-chat"
                    : isToday
                      ? "font-semibold text-line-green group-hover:bg-row-hover"
                      : c.inMonth
                        ? "group-hover:bg-row-hover"
                        : "text-secondary/50 group-hover:bg-row-hover"
                }`}
              >
                {c.day}
              </span>
              <span className="flex h-[4px] gap-[2px]">
                {Array.from({ length: Math.min(n, 3) }, (_, i) => (
                  <span key={i} className={`h-[4px] w-[4px] rounded-full bg-line-green ${c.inMonth ? "" : "opacity-40"}`} />
                ))}
              </span>
            </button>
          );
        })}
      </div>

      <div className="flex items-baseline gap-2 border-t border-separator px-4 pt-3 pb-1.5">
        <span className="text-[13px] font-semibold">{longDayLabel(day)}</span>
        {day === today && (
          <span className="rounded-full bg-line-green/12 px-1.5 py-px text-[10.5px] font-semibold text-line-green">วันนี้</span>
        )}
        {list.length > 0 && <span className="ml-auto text-[11.5px] text-secondary tabular-nums">{list.length} นัด</span>}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-20">
        {loading && (
          <div className="grid place-items-center py-10 text-secondary">
            <Spinner size={18} />
          </div>
        )}
        {error ? <p className="selectable px-3 py-6 text-center text-[12px] text-pretty text-badge-red">{errorText(error)}</p> : null}
        {events && list.length === 0 && (
          <div className="flex flex-col items-center px-6 pt-7 text-center">
            <span className="grid h-11 w-11 place-items-center rounded-full bg-line-green/10 text-line-green">
              <CalendarIcon size={22} />
            </span>
            <p className="mt-2.5 text-[13px] font-medium">{day === today ? "วันนี้ยังไม่มีนัด" : "ไม่มีนัด"}</p>
            <button onClick={onAdd} className="mt-1 text-[12.5px] font-medium text-line-green hover:underline">
              เพิ่มนัดหมาย
            </button>
          </div>
        )}
        {list.map((e) => {
          const [from, to] = agendaTimes(e, day);
          const past = e.end <= now && !(e.allDay && day === today);
          return (
            <button
              key={e.id}
              onClick={() => onOpen(e.id)}
              className={`flex w-full items-stretch gap-3 rounded-[10px] px-2 py-2 text-left transition-colors hover:bg-row-hover ${past ? "opacity-55" : ""}`}
            >
              <span className="flex w-[46px] shrink-0 flex-col items-end pt-px text-[12px] leading-[17px] tabular-nums">
                <span className="font-medium">{from}</span>
                {to && <span className="text-secondary">{to}</span>}
              </span>
              <span className={`w-[3px] shrink-0 rounded-full ${e.allDay ? "bg-line-green/45" : "bg-line-green"}`} />
              <span className="min-w-0 flex-1 py-px">
                <span className="block truncate text-[13.5px] leading-[18px] font-medium">{e.title || "(ไม่มีชื่อ)"}</span>
                {source && (
                  <span className="mt-0.5 flex items-center gap-1 text-[11.5px] text-secondary">
                    <BubbleIcon size={10} className="shrink-0" />
                    <span className="truncate">{source(e)}</span>
                  </span>
                )}
                {e.location && (
                  <span className="mt-0.5 flex items-center gap-1 text-[11.5px] text-secondary">
                    <PlaceIcon size={11} className="shrink-0" />
                    <span className="truncate">{e.location}</span>
                  </span>
                )}
              </span>
            </button>
          );
        })}
      </div>

      <button
        title="เพิ่มนัดหมาย"
        aria-label="เพิ่มนัดหมาย"
        onClick={onAdd}
        className="absolute right-4 bottom-4 grid h-11 w-11 place-items-center rounded-full bg-line-green text-white shadow-[0_1px_3px_rgba(0,0,0,0.25)] transition-transform duration-150 hover:scale-105 active:scale-95"
      >
        <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden>
          <path d="M9 3v12M3 9h12" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}

// -- event -------------------------------------------------------------------

function Detail({
  source,
  event: e,
  onBack,
  onEdit,
}: {
  /** The event's chat name, in the all-calendars window. */
  source: string | null;
  event: CalendarEvent;
  onBack: () => void;
  onEdit: () => void;
}) {
  const chatId = e.chatId;
  const people = usePeople();
  const creator = e.creator ? (people[e.creator]?.name ?? null) : null;
  const first = eventDays(e)[0];
  const [, fm, fd] = first.split("-").map(Number);
  const remove = () =>
    setUi({
      confirm: {
        title: "ลบนัดหมายนี้หรือไม่",
        body: source ? `นัดจะหายจากปฏิทินของ ${source} สำหรับทุกคน` : "นัดจะหายจากปฏิทินของแชทนี้สำหรับทุกคน",
        action: "ลบ",
        danger: true,
        run: () =>
          api
            .deleteCalendarEvent(chatId, e.id, e.updatedAt, false)
            .then(refresh)
            .then(onBack)
            .catch((err) => notice(errorText(err))),
      },
    });
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="px-2">
        <button
          className="flex h-7 items-center gap-0.5 rounded-full pr-2.5 pl-1 text-[12.5px] font-medium text-line-green transition-colors hover:bg-row-hover"
          onClick={onBack}
        >
          <ChevronIcon dir="left" size={14} />
          ปฏิทิน
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-2 pb-4">
        <div className="flex items-start gap-3">
          {/* A tear-off calendar page for the day it starts. */}
          <div className="w-12 shrink-0 overflow-hidden rounded-[10px] bg-field text-center shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.06)]">
            <div className="bg-line-green py-0.5 text-[10px] font-semibold text-white">{shortMonths[fm - 1]}</div>
            <div className="py-1 text-[20px] leading-none font-semibold tabular-nums">{fd}</div>
          </div>
          <div className="min-w-0 flex-1 pt-0.5">
            <h2 className="selectable text-[17px] leading-snug font-semibold text-pretty">{e.title || "(ไม่มีชื่อ)"}</h2>
            <p className="mt-0.5 text-[12.5px] text-secondary">{longDayLabel(first)}</p>
          </div>
        </div>

        <div className="mt-4 overflow-hidden rounded-[12px] bg-field/70">
          <Info icon={<ClockIcon size={16} />}>
            <span className="tabular-nums">{timeLabel(e)}</span>
          </Info>
          {source && (
            <Info icon={<BubbleIcon size={15} />}>
              <span className="text-secondary">ปฏิทินของ </span>
              {source}
            </Info>
          )}
          {e.location && (
            <Info icon={<PlaceIcon size={16} />}>
              <span className="selectable">{e.location}</span>
            </Info>
          )}
          {e.url && (
            <Info icon={<LinkIcon size={16} />}>
              <button onClick={() => openUrl(e.url, chatId)} className="text-left break-all text-mention hover:underline">
                {e.url}
              </button>
            </Info>
          )}
          {e.description && (
            <Info icon={<TextIcon size={16} />}>
              <p className="selectable leading-relaxed whitespace-pre-wrap">
                {linkify(e.description).map((s, i) =>
                  s.url ? (
                    <button key={i} onClick={() => openUrl(s.url!, chatId)} className="break-all text-mention hover:underline">
                      {s.text}
                    </button>
                  ) : (
                    <span key={i}>{s.text}</span>
                  ),
                )}
              </p>
            </Info>
          )}
          {creator && (
            <Info icon={<PersonIcon size={15} />}>
              <span className="text-secondary">สร้างโดย </span>
              {creator}
            </Info>
          )}
        </div>
      </div>
      {e.editable ? (
        <div className="flex gap-2 px-4 pt-2 pb-4">
          <button
            onClick={remove}
            className="h-9 rounded-[10px] px-4 text-[13px] font-semibold text-badge-red transition-colors hover:bg-badge-red/10"
          >
            ลบนัด
          </button>
          <button
            onClick={onEdit}
            className="h-9 flex-1 rounded-[10px] bg-line-green text-[13px] font-semibold text-white transition-[filter] hover:brightness-105 active:brightness-95"
          >
            แก้ไข
          </button>
        </div>
      ) : (
        <p className="px-4 pt-2 pb-4 text-center text-[12px] text-secondary">นัดนี้แก้ใน LINE Calendar ไม่ได้</p>
      )}
    </div>
  );
}

function Info({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="flex gap-3 px-3 py-2.5 text-[13px] [&+&]:border-t [&+&]:border-separator">
      <span className="mt-px shrink-0 text-secondary">{icon}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

// -- form --------------------------------------------------------------------

/** A grouped block of rows, as in macOS settings. */
const Group = ({ children }: { children: ReactNode }) => (
  <div className="overflow-hidden rounded-[12px] bg-field/70 [&>*+*]:border-t [&>*+*]:border-separator">{children}</div>
);

const rowClass = "flex min-h-[38px] items-center gap-2.5 px-3";
const bare = "selectable min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-secondary";

/** Add or edit. `onDone` gets the saved event's first day, or nothing when cancelled. */
function Editor({
  chatId: windowChat,
  targets,
  event,
  day,
  onDone,
}: {
  chatId: string;
  /** The calendars a new event can go in, in the all-calendars window. */
  targets: Target[] | null;
  event: CalendarEvent | null;
  day: string;
  onDone: (day?: string) => void;
}) {
  const [f, setF] = useState<EventForm>(() => (event ? formOf(event) : newForm(day)));
  // LINE posts a card in the chat for a new event; edits stay quiet unless asked.
  const [notify, setNotify] = useState(!event);
  const [target, setTarget] = useState(() => event?.chatId ?? (targets ? lastTarget(targets) : windowChat));
  const chatId = event?.chatId ?? target;
  // The user's own calendar has no chat to post in.
  const ownCalendar = chatId.startsWith("cal:");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const set = <K extends keyof EventForm>(k: K, v: EventForm[K]) => {
    setProblem(null);
    setF((old) => {
      const next = { ...old, [k]: v };
      // A new start time keeps the length: 9.00–10.00 moved to 9.30 ends at 10.30.
      if (k === "startTime" && old.endDate === old.startDate) {
        const length = Math.max(0, minutesBetween(old.startTime, old.endTime));
        const [h, m] = next.startTime.split(":").map(Number);
        const end = Math.min(h * 60 + m + length, 23 * 60 + 59);
        next.endTime = `${String(Math.floor(end / 60)).padStart(2, "0")}:${String(end % 60).padStart(2, "0")}`;
      }
      // Moving the start moves an end that would fall before it.
      if (k === "startDate" && next.endDate < next.startDate) next.endDate = next.startDate;
      if (k === "startDate" && next.endDate === next.startDate && next.endTime < next.startTime) next.endTime = next.startTime;
      return next;
    });
  };
  const save = () => {
    if (busy) return;
    // An edit keeps the event's own zone; a new one takes the Mac's.
    const input = inputOf(f, event?.timezone || localZone());
    if ("error" in input) return setProblem(input.error);
    if (!chatId) return setProblem("เลือกปฏิทินก่อน");
    if (targets) remember(chatId);
    setBusy(true);
    const call = event
      ? api.updateCalendarEvent(chatId, event.id, event.updatedAt, input, notify && !ownCalendar)
      : api.createCalendarEvent(chatId, input, notify && !ownCalendar);
    call
      .then(refresh)
      .then(() => onDone(f.startDate))
      .catch((e) => setProblem(errorText(e)))
      .finally(() => setBusy(false));
  };
  return (
    <form
      className="flex min-h-0 flex-1 flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
      onKeyDown={(e) => {
        // Escape in a picker (portaled outside the form) closes just the picker.
        if (e.key === "Escape" && e.currentTarget.contains(e.target as Node)) onDone();
        if (e.key === "Enter" && e.metaKey) save();
      }}
    >
      <div className="grid grid-cols-[1fr_auto_1fr] items-center px-2 pb-1">
        <button
          type="button"
          onClick={() => onDone()}
          className="h-7 justify-self-start rounded-full px-2.5 text-[13px] text-secondary transition-colors hover:bg-row-hover hover:text-primary"
        >
          ยกเลิก
        </button>
        <span className="text-[13px] font-semibold">{event ? "แก้ไขนัดหมาย" : "นัดหมายใหม่"}</span>
        <button
          type="submit"
          disabled={busy || !f.title.trim()}
          className="flex h-7 items-center gap-1.5 justify-self-end rounded-full px-2.5 text-[13px] font-semibold text-line-green transition-colors hover:bg-line-green/10 disabled:text-secondary disabled:hover:bg-transparent"
        >
          {busy && <Spinner size={11} />}
          {event ? "บันทึก" : "เพิ่ม"}
        </button>
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 pt-1 pb-4">
        {targets && !event && (
          <Group>
            <label className={rowClass}>
              <BubbleIcon size={14} className="shrink-0 text-secondary" />
              <span className="flex-1 text-[13px]">ปฏิทิน</span>
              <select
                aria-label="ปฏิทิน"
                value={target}
                onChange={(e) => setTarget(e.target.value)}
                className="h-7 max-w-[60%] min-w-0 truncate rounded-[7px] bg-chat/60 px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-line-green/50"
              >
                {!target && <option value="">เลือก…</option>}
                {targets.map((t) => (
                  <option key={t.chatId} value={t.chatId}>
                    {t.name}
                  </option>
                ))}
              </select>
            </label>
          </Group>
        )}
        <Group>
          <div className="px-3 pt-2.5 pb-2">
            <input
              autoFocus
              value={f.title}
              onChange={(e) => set("title", e.target.value)}
              placeholder="ชื่อนัดหมาย"
              aria-label="ชื่อนัดหมาย"
              className={`${bare} w-full text-[16px] font-semibold`}
            />
          </div>
          <label className={rowClass}>
            <PlaceIcon size={15} className="shrink-0 text-secondary" />
            <input value={f.location} onChange={(e) => set("location", e.target.value)} placeholder="สถานที่" className={bare} />
          </label>
        </Group>

        <Group>
          <div className={rowClass}>
            <span className="flex-1 text-[13px]">ทั้งวัน</span>
            <Switch label="ทั้งวัน" checked={f.allDay} onChange={(v) => set("allDay", v)} />
          </div>
          <When
            label="เริ่ม"
            date={f.startDate}
            time={f.startTime}
            allDay={f.allDay}
            onDate={(v) => set("startDate", v)}
            onTime={(v) => set("startTime", v)}
          />
          <When
            label="สิ้นสุด"
            date={f.endDate}
            time={f.endTime}
            allDay={f.allDay}
            onDate={(v) => set("endDate", v)}
            onTime={(v) => set("endTime", v)}
            from={f.endDate === f.startDate ? f.startTime : undefined}
          />
        </Group>

        <Group>
          <label className={rowClass}>
            <LinkIcon size={15} className="shrink-0 text-secondary" />
            <input value={f.url} onChange={(e) => set("url", e.target.value)} placeholder="ลิงก์" type="url" className={bare} />
          </label>
          <label className="flex gap-2.5 px-3 py-2.5">
            <TextIcon size={15} className="mt-px shrink-0 text-secondary" />
            <textarea
              value={f.description}
              onChange={(e) => set("description", e.target.value)}
              placeholder="รายละเอียด"
              rows={4}
              className={`${bare} resize-none leading-snug`}
            />
          </label>
        </Group>

        {!ownCalendar && (
          <Group>
            <div className={`${rowClass} py-2`}>
              <span className="flex-1">
                <span className="block text-[13px]">แจ้งในแชท</span>
                <span className="block text-[11.5px] text-secondary">
                  {event ? "บอกสมาชิกว่านัดนี้เปลี่ยน" : "โพสต์การ์ดนัดหมายลงในแชท"}
                </span>
              </span>
              <Switch label="แจ้งในแชท" checked={notify} onChange={setNotify} />
            </div>
          </Group>
        )}

        {problem && (
          <p role="alert" className="selectable px-1 text-[12px] text-pretty text-badge-red">
            {problem}
          </p>
        )}
      </div>
    </form>
  );
}

// The all-calendars window offers the calendar last added to first.
const LAST_TARGET = "laab.calendar.last";

function lastTarget(targets: Target[]): string {
  let last: string | null = null;
  try {
    last = localStorage.getItem(LAST_TARGET);
  } catch {}
  return targets.find((t) => t.chatId === last)?.chatId ?? targets[0]?.chatId ?? "";
}

function remember(chatId: string) {
  try {
    localStorage.setItem(LAST_TARGET, chatId);
  } catch {}
}

function When({
  label,
  date,
  time,
  allDay,
  onDate,
  onTime,
  from,
}: {
  label: string;
  date: string;
  time: string;
  allDay: boolean;
  onDate: (v: string) => void;
  onTime: (v: string) => void;
  /** The start time, when the end is on the same day. */
  from?: string;
}) {
  return (
    <div className={rowClass}>
      <span className="flex-1 text-[13px]">{label}</span>
      <DatePicker label={`วันที่${label}`} value={date} onChange={onDate} />
      {!allDay && <TimeField label={`เวลา${label}`} value={time} onChange={onTime} from={from} />}
    </div>
  );
}
