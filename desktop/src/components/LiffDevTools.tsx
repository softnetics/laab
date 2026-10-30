import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, errorText, queryClient } from "../lib/api";
import type { LiffDevEvent, LiffDevSession } from "../lib/types";
import { SearchIcon } from "./Icons";

const KEY = ["liff-dev"];

type Update = { label: string; session: LiffDevSession | null; events: LiffDevEvent[] };

/** Merge live updates from the LIFF windows into the session list. */
function useSessions() {
  const { data = [] } = useQuery({ queryKey: KEY, queryFn: api.liffDevSessions, staleTime: Infinity });
  useEffect(() => {
    const off = listen<Update>("liff-dev", ({ payload: u }) => {
      queryClient.setQueryData<LiffDevSession[]>(KEY, (old = []) => {
        const i = old.findIndex((s) => s.label === u.label);
        if (i < 0) return u.session ? [...old, u.session] : old;
        const s = old[i];
        const next = { ...s, ...(u.session ? { ...u.session, events: s.events } : {}), events: u.events.length ? [...s.events, ...u.events] : s.events };
        return old.map((x, j) => (j === i ? next : x));
      });
    });
    return () => {
      off.then((f) => f());
    };
  }, []);
  return data;
}

const FILTERS = [
  { key: "all", label: "ทั้งหมด" },
  { key: "problems", label: "ปัญหา" },
  { key: "console", label: "Console" },
  { key: "network", label: "Network" },
  { key: "bridge", label: "Bridge" },
  { key: "nav", label: "Navigation" },
] as const;
type Filter = (typeof FILTERS)[number]["key"];

const matches = (e: LiffDevEvent, f: Filter) =>
  f === "all" ||
  (f === "problems" ? e.level === "warn" || e.level === "error" : f === "console" ? e.kind === "console" || e.kind === "error" : f === "nav" ? e.kind === "nav" || e.kind === "page" : e.kind === f);

const clock = (t: number) => {
  const d = new Date(t);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
};

export function LiffDevTools() {
  const sessions = useSessions();
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    const off = listen<string>("liff-dev-focus", ({ payload }) => setSelected(payload));
    return () => {
      off.then((f) => f());
    };
  }, []);
  const newestFirst = [...sessions].reverse();
  const current = sessions.find((s) => s.label === selected) ?? newestFirst[0] ?? null;

  return (
    <div className="flex h-full bg-chat text-primary">
      <aside className="flex w-[232px] shrink-0 flex-col bg-rail">
        <div className="drag h-[44px] shrink-0" data-tauri-drag-region />
        <div className="px-4 pb-2 text-[11px] font-semibold tracking-[0.06em] text-secondary uppercase">หน้าต่าง LIFF</div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          {newestFirst.map((s) => {
            const problems = s.events.filter((e) => e.level === "error").length;
            return (
              <button
                key={s.label}
                onClick={() => setSelected(s.label)}
                className={`flex w-full flex-col gap-0.5 rounded-lg px-2.5 py-2 text-left transition-colors duration-150 ease-out ${
                  s.label === current?.label ? "bg-row-selected" : "hover:bg-row-hover"
                }`}
              >
                <span className="flex items-center gap-2">
                  <span
                    aria-label={s.closed ? "ปิดแล้ว" : "เปิดอยู่"}
                    className={`size-1.5 shrink-0 rounded-full ${s.closed ? "bg-secondary/50" : "bg-line-green"}`}
                  />
                  <span className={`truncate text-[13px] ${s.closed ? "text-secondary" : ""}`}>{s.info.appName || s.liffId}</span>
                  {problems > 0 && <span className="ml-auto shrink-0 text-[11px] text-badge-red tabular-nums">{problems}</span>}
                </span>
                <span className="truncate pl-3.5 text-[11.5px] text-secondary tabular-nums">
                  {clock(s.openedAt).slice(0, 8)} · {s.mode === "client" ? "LIFF" : "เว็บ"}
                </span>
              </button>
            );
          })}
        </div>
      </aside>
      {current ? (
        <Detail key={current.label} session={current} />
      ) : (
        <div className="flex flex-1 flex-col">
          <div className="drag h-[44px] shrink-0" data-tauri-drag-region />
          <div className="flex flex-1 flex-col items-center justify-center gap-1 px-8 pb-12 text-center">
            <div className="text-[14px]">ยังไม่มีหน้าต่าง LIFF</div>
            <div className="text-[12.5px] text-secondary">เปิด LIFF จากแชทหรือเมนูของบัญชีทางการ แล้วจะเห็นข้อมูลและ log ที่นี่</div>
          </div>
        </div>
      )}
    </div>
  );
}

function Detail({ session: s }: { session: LiffDevSession }) {
  const [tab, setTab] = useState<"log" | "info">("log");
  const [error, setError] = useState<string | null>(null);
  const act = (action: Parameters<typeof api.liffDevAction>[1]) => {
    setError(null);
    api.liffDevAction(s.label, action).catch((e) => setError(errorText(e)));
  };
  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <header className="drag flex h-[52px] shrink-0 items-center gap-3 border-b border-separator pt-1 pr-3 pl-5" data-tauri-drag-region>
        <div className="min-w-0 flex-1 leading-tight">
          <div className="truncate text-[14px] font-semibold">{s.info.appName || "LIFF"}</div>
          <div className="selectable truncate font-mono text-[11.5px] text-secondary">
            {s.liffId}
            {s.reason && <span className="font-sans text-badge-red"> · เปิดเป็นเว็บ: {s.reason}</span>}
            {s.closed && <span className="font-sans"> · ปิดแล้ว</span>}
          </div>
        </div>
        <div className="no-drag flex items-center gap-1">
          <ToolButton disabled={s.closed} onClick={() => act("reload")} title="โหลดหน้าเดิมซ้ำ ใช้ token เดิม">
            รีโหลด
          </ToolButton>
          <ToolButton onClick={() => act("reopen")} title="ขอ token ใหม่จาก LINE แล้วเปิดหน้าต่างใหม่">
            เปิดใหม่
          </ToolButton>
          <ToolButton onClick={() => act("web")} title="เปิดเป็นหน้าเว็บธรรมดา ไม่มี token">
            เปิดเป็นเว็บ
          </ToolButton>
          <ToolButton disabled={s.closed} onClick={() => act("inspect")} title="Web Inspector ของหน้าต่างนี้ (dev build)">
            Inspector
          </ToolButton>
          <ToolButton disabled={s.closed} onClick={() => act("close")} title="ปิดหน้าต่าง LIFF">
            ปิด
          </ToolButton>
        </div>
      </header>
      {error && <div className="border-b border-separator bg-badge-red/10 px-5 py-1.5 text-[12px] text-badge-red">{error}</div>}
      <div className="flex h-10 shrink-0 items-center gap-1 border-b border-separator px-3" role="tablist">
        {(["log", "info"] as const).map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`h-7 rounded-md px-3 text-[12.5px] transition-colors duration-150 ease-out ${tab === t ? "bg-field text-primary" : "text-secondary hover:text-primary"}`}
          >
            {t === "log" ? `Log · ${s.events.length}` : "ข้อมูล"}
          </button>
        ))}
      </div>
      {tab === "log" ? <Log session={s} /> : <Info session={s} />}
    </section>
  );
}

function ToolButton({ children, ...p }: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...p}
      className="h-7 rounded-md px-2.5 text-[12.5px] text-secondary transition-[color,background-color,scale] duration-150 ease-out hover:bg-field hover:text-primary active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40"
    >
      {children}
    </button>
  );
}

const KIND_LABEL: Record<LiffDevEvent["kind"], string> = {
  console: "console",
  error: "error",
  network: "network",
  bridge: "bridge",
  nav: "nav",
  page: "page",
};

function Log({ session: s }: { session: LiffDevSession }) {
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  // "ล้าง" hides what came before; the full log stays in the app.
  const [from, setFrom] = useState(0);
  const events = useMemo(() => {
    const q = query.trim().toLowerCase();
    return s.events.slice(from).filter((e) => matches(e, filter) && (!q || e.text.toLowerCase().includes(q)));
  }, [s.events, from, filter, query]);
  const counts = useMemo(() => {
    const visible = s.events.slice(from);
    return Object.fromEntries(FILTERS.map((f) => [f.key, visible.filter((e) => matches(e, f.key)).length]));
  }, [s.events, from]);

  // Follow new events while scrolled to the bottom.
  const list = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    if (pinned.current && list.current) list.current.scrollTop = list.current.scrollHeight;
  }, [events.length]);

  return (
    <>
      <div className="flex shrink-0 items-center gap-1 border-b border-separator px-3 py-1.5">
        {FILTERS.map((f) => (
          <button
            key={f.key}
            onClick={() => setFilter(f.key)}
            aria-pressed={filter === f.key}
            className={`flex h-6 items-center gap-1 rounded-full px-2.5 text-[12px] transition-colors duration-150 ease-out ${
              filter === f.key ? "bg-primary text-chat" : "text-secondary hover:bg-field hover:text-primary"
            }`}
          >
            {f.label}
            {counts[f.key] > 0 && <span className={`tabular-nums ${filter === f.key ? "opacity-70" : "opacity-60"}`}>{counts[f.key]}</span>}
          </button>
        ))}
        <label className="ml-auto flex h-7 w-48 items-center gap-1.5 rounded-md bg-field px-2">
          <SearchIcon size={12} className="shrink-0 text-secondary" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="กรองข้อความ"
            aria-label="กรอง log"
            className="selectable min-w-0 flex-1 bg-transparent text-[12.5px] outline-none placeholder:text-secondary"
          />
        </label>
        <ToolButton onClick={() => setFrom(s.events.length)} disabled={!s.events.length || from === s.events.length}>
          ล้าง
        </ToolButton>
      </div>
      <div
        ref={list}
        onScroll={(e) => {
          const el = e.currentTarget;
          pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className="selectable min-h-0 flex-1 overflow-y-auto py-1 font-mono text-[11.5px] leading-[1.55]"
      >
        {events.length === 0 ? (
          <div className="px-5 py-6 font-sans text-[12.5px] text-secondary">
            {s.events.length === 0 ? "ยังไม่มี log จากหน้านี้" : "ไม่มี log ที่ตรงกับตัวกรอง"}
          </div>
        ) : (
          events.map((e, i) => (
            <div
              key={i}
              className={`flex gap-3 border-b border-separator/50 px-5 py-[3px] ${
                e.level === "error" ? "bg-badge-red/[0.07] text-badge-red" : e.level === "warn" ? "bg-amber-400/[0.07] text-amber-600 dark:text-amber-300" : e.level === "debug" ? "text-secondary" : ""
              }`}
            >
              <span className="shrink-0 text-secondary tabular-nums">{clock(e.t)}</span>
              <span className="w-[52px] shrink-0 text-secondary">{KIND_LABEL[e.kind] ?? e.kind}</span>
              <span className="min-w-0 flex-1 break-all whitespace-pre-wrap">{e.text}</span>
            </div>
          ))
        )}
      </div>
      {!s.closed && <Prompt label={s.label} onRun={() => (pinned.current = true)} />}
    </>
  );
}

/** Run JavaScript in the LIFF page; ↑ and ↓ walk earlier entries. */
function Prompt({ label, onRun }: { label: string; onRun: () => void }) {
  const [code, setCode] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [at, setAt] = useState(-1);
  const [error, setError] = useState<string | null>(null);
  const run = () => {
    const c = code.trim();
    if (!c) return;
    onRun();
    setHistory((h) => [c, ...h.filter((x) => x !== c)].slice(0, 50));
    setAt(-1);
    setCode("");
    setError(null);
    api.liffDevEval(label, c).catch((e) => setError(errorText(e)));
  };
  return (
    <div className="shrink-0 border-t border-separator">
      {error && <div className="px-5 pt-1.5 text-[12px] text-badge-red">{error}</div>}
      <label className="flex items-center gap-2 px-5 py-2 font-mono text-[12px]">
        <span className="text-line-green" aria-hidden>
          ›
        </span>
        <input
          value={code}
          onChange={(e) => setCode(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) {
              e.preventDefault();
              run();
            } else if (e.key === "ArrowUp" && history.length) {
              e.preventDefault();
              const i = Math.min(at + 1, history.length - 1);
              setAt(i);
              setCode(history[i]);
            } else if (e.key === "ArrowDown" && at >= 0) {
              e.preventDefault();
              const i = at - 1;
              setAt(i);
              setCode(i < 0 ? "" : history[i]);
            }
          }}
          placeholder="รัน JavaScript ในหน้านี้ เช่น location.href"
          aria-label="รัน JavaScript ในหน้า LIFF"
          spellCheck={false}
          autoComplete="off"
          className="selectable min-w-0 flex-1 bg-transparent outline-none placeholder:font-sans placeholder:text-secondary"
        />
      </label>
    </div>
  );
}

/** The payload of a JWT, for reading the id_token's claims. */
function claims(jwt: string): string | null {
  try {
    const part = jwt.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
    const json = new TextDecoder().decode(Uint8Array.from(atob(part), (c) => c.charCodeAt(0)));
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return null;
  }
}

function Info({ session: s }: { session: LiffDevSession }) {
  const i = s.info;
  const expires = i.expiresIn ? s.openedAt + i.expiresIn * 1000 : null;
  const tokens = Object.entries(i.tokens ?? {}).filter(([, v]) => v);
  const idClaims = i.tokens?.id_token ? claims(i.tokens.id_token) : null;
  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <dl className="grid grid-cols-[132px_1fr] gap-x-4 gap-y-2.5 text-[12.5px]">
        <Row label="ลิงก์ที่ขอ" value={i.requestedUrl} mono />
        <Row label="Endpoint" value={i.endpoint} mono />
        <Row label="URL ที่เปิด" value={s.url} mono />
        <Row label="โหมด" value={s.mode === "client" ? "LIFF (มี token)" : `หน้าเว็บ${s.reason ? ` · ${s.reason}` : ""}`} />
        <Row label="แชท" value={s.chatId} mono />
        <Row label="ขนาดหน้าต่าง" value={i.viewType} />
        <Row label="Scopes" value={i.scopes?.join(", ")} mono />
        <Row
          label="Token หมดอายุ"
          value={expires ? `${new Date(expires).toLocaleString("th-TH")}${expires < Date.now() ? " · หมดแล้ว" : ""}` : null}
        />
      </dl>
      {tokens.length > 0 && (
        <>
          <h3 className="mt-6 mb-2 text-[11px] font-semibold tracking-[0.06em] text-secondary uppercase">Tokens</h3>
          <div className="flex flex-col gap-1.5">
            {tokens.map(([name, value]) => (
              <Token key={name} name={name} value={value} />
            ))}
          </div>
        </>
      )}
      {idClaims && (
        <>
          <h3 className="mt-6 mb-2 text-[11px] font-semibold tracking-[0.06em] text-secondary uppercase">id_token claims</h3>
          <pre className="selectable overflow-x-auto rounded-lg bg-field px-3 py-2.5 font-mono text-[11.5px] leading-[1.55]">{idClaims}</pre>
        </>
      )}
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value?: string | null; mono?: boolean }) {
  return (
    <>
      <dt className="text-secondary">{label}</dt>
      <dd className={`selectable min-w-0 break-all ${mono ? "font-mono text-[11.5px] leading-[1.7]" : ""} ${value ? "" : "text-secondary"}`}>{value || "—"}</dd>
    </>
  );
}

function Token({ name, value }: { name: string; value: string }) {
  const [shown, setShown] = useState(false);
  const [copied, setCopied] = useState(false);
  const copy = () =>
    writeText(value).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  return (
    <div className="flex items-start gap-3 rounded-lg bg-field px-3 py-2">
      <div className="w-[104px] shrink-0 pt-px font-mono text-[11.5px]">{name}</div>
      <div className="selectable min-w-0 flex-1 font-mono text-[11.5px] leading-[1.55] break-all text-secondary">
        {shown ? value : `${value.slice(0, 8)}…${value.slice(-6)}`}
        <span className="ml-2 font-sans tabular-nums opacity-70">{value.length} ตัวอักษร</span>
      </div>
      <ToolButton onClick={() => setShown((v) => !v)}>{shown ? "ซ่อน" : "แสดง"}</ToolButton>
      <ToolButton onClick={copy}>{copied ? "คัดลอกแล้ว" : "คัดลอก"}</ToolButton>
    </div>
  );
}
