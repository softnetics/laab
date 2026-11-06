import { Combobox } from "@base-ui/react/combobox";
import { Dialog } from "@base-ui/react/dialog";
import { Select } from "@base-ui/react/select";
import { useQuery } from "@tanstack/react-query";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, errorText, q, queryClient, useChatList } from "../lib/api";
import { GROUPS, TYPES, accessLabel, resolve, setupSnippets, type McpScope } from "../lib/mcp";
import type { ChatList, McpAccess, McpGroup, McpRule, McpSettings } from "../lib/types";
import { notice } from "../lib/ui";
import { Avatar } from "./Avatar";
import { BubbleIcon, ChevronIcon, FolderIcon, GroupIcon, MegaphoneIcon, PersonIcon, PlugIcon, PlusIcon, SendIcon, SlidersIcon, WarningIcon } from "./Icons";
import { Switch } from "./kit";
import { buttonClass } from "./Sheet";

const CheckIcon = () => (
  <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.6} strokeLinecap="round" strokeLinejoin="round">
    <path d="M5 12.5 10 17.5 19 7" />
  </svg>
);

/** An inset grouped box, as in macOS System Settings. */
const groupBox = "overflow-hidden rounded-[10px] bg-field/45 ring-1 ring-black/5";

type Tab = "access" | "connect" | "activity";
type Level = McpAccess | "inherit";

/** Centered tab control with a sliding thumb, like the tabs in a macOS settings window. */
function Tabs<T extends string>({ value, options, onChange }: { value: T; options: { id: T; label: string }[]; onChange: (v: T) => void }) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const [thumb, setThumb] = useState<{ x: number; w: number } | null>(null);
  const index = options.findIndex((o) => o.id === value);
  useLayoutEffect(() => {
    const el = refs.current[index];
    if (el) setThumb({ x: el.offsetLeft, w: el.offsetWidth });
  }, [index]);
  return (
    <div role="tablist" className="relative mx-auto flex rounded-lg bg-black/15 p-0.5">
      {thumb && (
        <span
          aria-hidden
          className="absolute inset-y-0.5 left-0 rounded-md bg-[light-dark(#ffffff,#5c5c5f)] shadow-[0_1px_2px_rgba(0,0,0,0.25)] ring-1 ring-black/10 transition-[transform,width] duration-200 ease-out motion-reduce:transition-none"
          style={{ width: thumb.w, transform: `translateX(${thumb.x}px)` }}
        />
      )}
      {options.map((o, i) => (
        <button
          key={o.id}
          ref={(el) => {
            refs.current[i] = el;
          }}
          role="tab"
          aria-selected={i === index}
          onClick={() => onChange(o.id)}
          className={`relative h-6 min-w-[84px] rounded-md px-3 text-[12.5px] whitespace-nowrap outline-none transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-secondary ${i === index ? "font-medium text-primary" : "text-secondary hover:text-primary"}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

const folderIds = (list: ChatList | undefined, chatId: string) =>
  Object.entries(list?.pins ?? {})
    .filter(([ws, ids]) => ws !== "main" && ids.includes(chatId))
    .map(([ws]) => ws);

const kindLabel = (kind: string) => (kind === "group" ? "กลุ่ม" : kind === "bot" ? "บัญชีทางการ" : "เพื่อน");

/** What the permissions apply to: the defaults ("main profile"), or a
 *  chat, folder or kind of chat with its own settings, like a Discord
 *  per-server profile. */
type Target = { kind: "default" | McpRule["kind"]; id: string; name: string; sub: string; picture?: string | null };

const DEFAULT: Target = { kind: "default", id: "", name: "ค่าเริ่มต้น", sub: "ทุกแชทที่ไม่ได้ตั้งเฉพาะ" };
const keyOf = (t: { kind: string; id: string }) => `${t.kind}:${t.id}`;

function targetScope(t: Target, list: ChatList | undefined): McpScope {
  if (t.kind === "chat") {
    const chat = list?.chats.find((c) => c.id === t.id);
    return { kind: "chat", id: t.id, type: chat?.kind ?? (t.id.startsWith("u") ? "dm" : "group"), folders: folderIds(list, t.id) };
  }
  if (t.kind === "folder") return { kind: "folder", id: t.id };
  return { kind: "global" };
}

const SCOPE_HINT: Record<Target["kind"], string> = {
  default: "ใช้กับทุกแชทที่ไม่ได้ตั้งเฉพาะ ถ้าปิดการอ่าน AI จะไม่เห็นแชทนั้นเลย",
  chat: "ใช้กับแชทนี้แชทเดียว และมาก่อนการตั้งค่าของโฟลเดอร์และประเภทแชท",
  folder: "ใช้กับทุกแชทในโฟลเดอร์นี้ แชทที่อยู่หลายโฟลเดอร์จะใช้ค่าที่เข้มที่สุด",
  type: "ใช้กับทุกแชทประเภทนี้ ที่ยังไม่ได้ตั้งไว้ที่ตัวแชทหรือโฟลเดอร์",
};

const GROUP_ICON: Record<McpGroup, (p: { size?: number }) => ReactNode> = {
  read: BubbleIcon,
  send: SendIcon,
  manage: SlidersIcon,
  account: WarningIcon,
};

/** Tile tint per group, System Settings style. */
const GROUP_TINT: Record<McpGroup, string> = {
  read: "bg-[#3b82f6]",
  send: "bg-line-green",
  manage: "bg-[#8b5cf6]",
  account: "bg-[#f97316]",
};

const ACCESS_TEXT: Record<McpAccess, string> = {
  off: "text-badge-red",
  ask: "text-[#e9a23b]",
  on: "text-line-green",
};

/** Off, ask and allow as colour. Ask is amber, so the green of "allow" never doubles as a warning. */
const ACCESS_BG: Record<McpAccess, string> = {
  off: "bg-badge-red",
  ask: "bg-[#e9a23b]",
  on: "bg-line-green",
};

function TargetIcon({ t, size }: { t: Target; size: number }) {
  if (t.kind === "chat") return <Avatar id={t.id} name={t.name} picture={t.picture} size={size} />;
  const glyph = Math.round(size * 0.58);
  const icon =
    t.kind === "default" ? (
      <SlidersIcon size={glyph} />
    ) : t.kind === "folder" ? (
      <FolderIcon size={glyph} />
    ) : t.id === "group" ? (
      <GroupIcon size={glyph} />
    ) : t.id === "bot" ? (
      <MegaphoneIcon size={Math.round(glyph * 0.9)} />
    ) : (
      <PersonIcon size={glyph} />
    );
  // Folders are rounded squares, as in the chat list; the rest are round like avatars.
  return (
    <span
      className={`grid shrink-0 place-items-center bg-black/20 ${t.kind === "folder" ? "rounded-[30%]" : "rounded-full"} ${t.kind === "default" || t.kind === "folder" ? "text-line-green" : "text-menu-text/80"}`}
      style={{ width: size, height: size }}
    >
      {icon}
    </span>
  );
}

/** "Add": every kind, folder and chat without its own settings yet (most recent first), with a search field. */
function AddTarget({ targets, onChange }: { targets: Target[]; onChange: (t: Target) => void }) {
  const [query, setQuery] = useState("");
  return (
    <Combobox.Root
      items={targets}
      value={null}
      onValueChange={(t: Target | null) => t && onChange(t)}
      isItemEqualToValue={(a: Target, b: Target) => keyOf(a) === keyOf(b)}
      itemToStringLabel={(t: Target) => t.name}
      inputValue={query}
      onInputValueChange={setQuery}
      onOpenChange={(open) => !open && setQuery("")}
      filter={(t: Target, q: string) => {
        const term = q.trim().toLowerCase();
        return !term || t.name.toLowerCase().includes(term) || t.sub.toLowerCase().includes(term);
      }}
      autoHighlight
      limit={300}
    >
      <Combobox.Trigger className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-[12.5px] text-line-green outline-none hover:bg-black/10 focus-visible:outline-2 focus-visible:outline-secondary data-popup-open:bg-black/10">
        <PlusIcon size={14} />
        เพิ่มแชทหรือโฟลเดอร์
      </Combobox.Trigger>
      <Combobox.Portal>
        <Combobox.Positioner className="z-[60] outline-none" sideOffset={6} align="start">
          <Combobox.Popup className="flex max-h-[min(380px,var(--available-height))] w-[320px] flex-col overflow-hidden rounded-lg bg-menu shadow-xl ring-1 ring-black/15">
            <div className="p-2 pb-1">
              <Combobox.Input
                placeholder="ค้นหาแชท กลุ่ม หรือโฟลเดอร์"
                className="selectable h-8 w-full rounded-md bg-black/15 px-3 text-[13px] text-menu-text outline-none placeholder:text-secondary"
              />
            </div>
            <Combobox.Empty className="px-4 py-3 text-[12.5px] text-secondary empty:hidden">ไม่พบแชทหรือโฟลเดอร์ที่ตรงกัน</Combobox.Empty>
            <Combobox.List className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1.5 pt-0.5">
              {(t: Target) => (
                <Combobox.Item
                  key={keyOf(t)}
                  value={t}
                  className="flex cursor-default items-center gap-2.5 rounded-md px-2.5 py-2 text-menu-text outline-none select-none data-highlighted:bg-black/20"
                >
                  <TargetIcon t={t} size={26} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13.5px]">{t.name}</span>
                    <span className="block truncate text-[11.5px] text-secondary">{t.sub}</span>
                  </span>
                </Combobox.Item>
              )}
            </Combobox.List>
          </Combobox.Popup>
        </Combobox.Positioner>
      </Combobox.Portal>
    </Combobox.Root>
  );
}

/** A pop-up menu for one group's level, with a coloured dot per level. */
/** What each level means, under its name in the menu. */
function levelHint(g: McpGroup, l: Level): string {
  if (l === "inherit") return "ไม่ตั้งเฉพาะที่นี่ ใช้ค่าเดียวกับแชทอื่น";
  if (l === "on") return "AI ทำได้ทันที ไม่ต้องถามคุณ";
  if (l === "ask") return "ขึ้นหน้าต่างให้คุณกดอนุญาตทุกครั้ง";
  return g === "read" ? "AI มองไม่เห็นแชทนี้เลย" : "AI ทำไม่ได้";
}

function LevelMenu({ group, value, inherited, allowInherit, onChange, disabled }: { group: McpGroup; value: Level; inherited: McpAccess; allowInherit: boolean; onChange: (l: Level) => void; disabled: boolean }) {
  const name = (l: Level) => (l === "inherit" ? "ตามปกติ" : accessLabel(l));
  const options: Level[] = [...(allowInherit ? (["inherit"] as Level[]) : []), "on", "ask", "off"];
  const dot = (l: Level) =>
    l === "inherit" ? (
      <span className={`size-2 shrink-0 rounded-full ring-[1.5px] ring-current ${ACCESS_TEXT[inherited]}`} />
    ) : (
      <span className={`size-2 shrink-0 rounded-full ${ACCESS_BG[l]}`} />
    );
  return (
    <Select.Root value={value} onValueChange={(v) => onChange(v as Level)} disabled={disabled}>
      <Select.Trigger className="group flex h-7 shrink-0 items-center gap-2 rounded-full bg-black/10 pr-2 pl-2.5 text-[12.5px] text-primary outline-none transition-colors duration-150 hover:bg-black/20 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-secondary active:scale-[0.97] data-disabled:opacity-50 data-popup-open:bg-black/20">
        {dot(value)}
        <span>
          {name(value)}
          {value === "inherit" && <span className="text-secondary"> · {accessLabel(inherited)}</span>}
        </span>
        <ChevronIcon dir="down" size={10} className="text-secondary transition-transform duration-150 group-data-popup-open:rotate-180" />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner className="z-[60] outline-none" alignItemWithTrigger={false} side="bottom" align="end" sideOffset={6}>
          <Select.Popup className="w-[248px] origin-[var(--transform-origin)] rounded-[10px] bg-menu p-1 shadow-[0_10px_30px_rgba(0,0,0,0.3)] ring-1 ring-black/15 outline-none transition-[scale,opacity] duration-150 ease-out data-ending-style:scale-[0.96] data-ending-style:opacity-0 data-starting-style:scale-[0.96] data-starting-style:opacity-0 motion-reduce:data-ending-style:scale-100 motion-reduce:data-starting-style:scale-100">
            <Select.List>
              {options.map((l) => [
                <Select.Item
                  key={l}
                  value={l}
                  className="flex cursor-default items-center gap-2.5 rounded-[7px] px-2.5 py-2 text-menu-text outline-none select-none data-highlighted:bg-black/15"
                >
                  {dot(l)}
                  <span className="min-w-0 flex-1">
                    <Select.ItemText className="block text-[13px] leading-tight">
                      {name(l)}
                      {l === "inherit" && <span className="text-secondary"> · {accessLabel(inherited)}</span>}
                    </Select.ItemText>
                    <span className="mt-0.5 block text-[11.5px] leading-snug text-secondary">{levelHint(group, l)}</span>
                  </span>
                  <Select.ItemIndicator className="text-line-green">
                    <CheckIcon />
                  </Select.ItemIndicator>
                </Select.Item>,
                l === "inherit" && <Select.Separator key="sep" className="mx-2.5 my-1 h-px bg-separator" />,
              ])}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

/** Four dots: what each group comes to for a target, in the sidebar. */
function Dots({ levels }: { levels: McpAccess[] }) {
  return (
    <span className="mt-1 flex gap-[3px]" aria-label={GROUPS.map((g, i) => `${g.label} ${accessLabel(levels[i])}`).join(", ")}>
      {levels.map((a, i) => (
        <span key={i} className={`size-[5px] rounded-full ${ACCESS_BG[a]}`} />
      ))}
    </span>
  );
}

/** The defaults and every chat, folder or kind with its own settings down
 *  the side, so each rule is its own item; the selected one is edited on the right. */
function AccessTab({ settings, save }: { settings: McpSettings; save: (s: McpSettings) => void }) {
  const list = useChatList();
  const [selectedKey, setSelectedKey] = useState(keyOf(DEFAULT));
  const custom = useMemo(() => new Set(settings.rules.map(keyOf)), [settings.rules]);
  const all = useMemo(() => {
    const all: Target[] = [
      ...TYPES.map((t) => ({ kind: "type" as const, id: t.id, name: t.label, sub: "ประเภทแชท" })),
      ...(list?.workspaces ?? []).map((w) => ({ kind: "folder" as const, id: w.id, name: w.name || "โฟลเดอร์ไม่มีชื่อ", sub: "โฟลเดอร์" })),
      ...[...(list?.chats ?? [])]
        .sort((a, b) => b.lastActivity - a.lastActivity)
        .map((c) => ({ kind: "chat" as const, id: c.id, name: c.name, sub: kindLabel(c.kind), picture: c.picture })),
    ];
    // Settings for chats or folders LAAB no longer lists stay reachable.
    for (const r of settings.rules) {
      if (!all.some((t) => keyOf(t) === keyOf(r))) all.push({ kind: r.kind, id: r.id, name: r.kind === "folder" ? "โฟลเดอร์ที่ลบไปแล้ว" : r.id, sub: r.kind === "folder" ? "โฟลเดอร์" : "แชทที่ไม่อยู่ในรายการ" });
    }
    return all;
  }, [list, settings.rules]);
  const rank = { default: -1, chat: 0, folder: 1, type: 2 } as const;
  const own = all.filter((t) => custom.has(keyOf(t))).sort((a, b) => rank[a.kind] - rank[b.kind]);
  const rest = all.filter((t) => !custom.has(keyOf(t)));
  const target = [DEFAULT, ...all].find((t) => keyOf(t) === selectedKey) ?? DEFAULT;
  const rule = settings.rules.find((r) => keyOf(r) === keyOf(target));
  const isDefault = target.kind === "default";
  // Picked from "add" but nothing set yet: listed until something is, or another item is picked.
  const pending = !isDefault && !rule ? target : null;
  const off = !settings.enabled;

  const level = (g: McpGroup): Level => (isDefault ? settings.defaults[g] : (rule?.levels[g] ?? "inherit"));
  const inherited = (g: McpGroup) => resolve({ ...settings, enabled: true }, g, targetScope(target, list), rule);
  const setLevel = (g: McpGroup, a: Level) => {
    if (target.kind === "default") {
      if (a !== "inherit") save({ ...settings, defaults: { ...settings.defaults, [g]: a } });
      return;
    }
    const levels = { ...rule?.levels, [g]: a === "inherit" ? null : a };
    const empty = Object.values(levels).every((v) => !v);
    const others = settings.rules.filter((r) => r !== rule);
    save({ ...settings, rules: empty ? others : [...others, { kind: target.kind, id: target.id, levels }] });
  };
  const reset = () => {
    save({ ...settings, rules: settings.rules.filter((r) => r !== rule) });
    setSelectedKey(keyOf(DEFAULT));
  };

  const item = (t: Target, note?: string) => {
    const selected = keyOf(t) === keyOf(target);
    return (
      <button
        key={keyOf(t)}
        onClick={() => setSelectedKey(keyOf(t))}
        aria-current={selected || undefined}
        className={`flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left outline-none focus-visible:outline-2 focus-visible:outline-secondary ${selected ? "bg-black/15" : "hover:bg-black/[0.07]"}`}
      >
        <TargetIcon t={t} size={26} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] leading-tight">{t.name}</span>
          {note ? (
            <span className="block text-[11px] text-secondary">{note}</span>
          ) : (
            <Dots levels={GROUPS.map((g) => resolve({ ...settings, enabled: true }, g.id, targetScope(t, list)))} />
          )}
        </span>
      </button>
    );
  };

  return (
    // With MCP off nothing here applies, so it can't be changed either: dimmed and out of reach of clicks and Tab.
    <div className={`flex min-h-0 flex-1 transition-opacity duration-200 ${off ? "opacity-45 select-none" : ""}`} inert={off}>
      <nav aria-label="ตั้งค่าให้" className="flex w-[224px] shrink-0 flex-col border-r border-separator bg-black/[0.06] px-2 pt-3 pb-2">
        {item(DEFAULT)}
        <div className="mt-3 mb-1 px-2 text-[11px] font-medium text-secondary">ตั้งเฉพาะ</div>
        <div className="flex min-h-0 flex-1 flex-col gap-px overflow-y-auto">
          {own.map((t) => item(t))}
          {pending && item(pending, "ยังไม่ได้ตั้ง")}
          {!own.length && !pending && <p className="px-2 pb-1 text-[11.5px] leading-snug text-pretty text-secondary">ยังไม่มี เพิ่มแชทหรือโฟลเดอร์ที่อยากให้ต่างจากค่าเริ่มต้น</p>}
        </div>
        <div className="mt-1 border-t border-separator pt-1">
          <AddTarget targets={rest} onChange={(t) => setSelectedKey(keyOf(t))} />
        </div>
      </nav>

      <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-y-auto px-5 py-4">
        <header className="flex items-center gap-3 px-1">
          <TargetIcon t={target} size={36} />
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-[14.5px] font-semibold">{target.name}</h3>
            <p className="text-[11.5px] leading-snug text-pretty text-secondary">{SCOPE_HINT[target.kind]}</p>
          </div>
        </header>
        <div className={groupBox}>
          {GROUPS.map((g, i) => {
            const Icon = GROUP_ICON[g.id];
            return (
              <div key={g.id} className={`flex items-center gap-3 px-3 py-2.5 ${i ? "border-t border-separator" : ""}`}>
                <span className={`grid size-7 shrink-0 place-items-center rounded-[7px] text-white ${GROUP_TINT[g.id]}`}>
                  <Icon size={16} />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px]">{g.label}</span>
                  <span className="block text-[11.5px] leading-snug text-pretty text-secondary">{g.hint}</span>
                </span>
                <LevelMenu group={g.id} value={level(g.id)} inherited={inherited(g.id)} allowInherit={!isDefault} onChange={(l) => setLevel(g.id, l)} disabled={off} />
              </div>
            );
          })}
        </div>
        {rule && (
          <button className="self-end px-1 text-[12px] text-badge-red hover:underline" onClick={reset}>
            ลบการตั้งเฉพาะนี้
          </button>
        )}
      </section>
    </div>
  );
}

function Code({ label, text }: { label: string; text: string }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-3 px-3 text-[12px] text-secondary">
        <span className="min-w-0 flex-1">{label}</span>
        <button className="shrink-0 text-line-green hover:underline" onClick={() => writeText(text).then(() => notice("คัดลอกแล้ว"))}>
          คัดลอก
        </button>
      </div>
      <pre className={`${groupBox} selectable px-3 py-2.5 font-mono text-[11.5px] leading-[1.45] whitespace-pre-wrap break-all`}>{text}</pre>
    </div>
  );
}

function ConnectTab() {
  const { data: setup } = useQuery({ queryKey: ["mcp-setup"], queryFn: api.mcpSetup, staleTime: 0 });
  if (!setup) return null;
  const { claudeCode, json } = setupSnippets(setup);
  return (
    <div className="flex flex-col gap-4">
      <Code label="Claude Code: รันใน Terminal" text={claudeCode} />
      <Code label="Claude Desktop, Cursor และแอปอื่น: ใส่ในไฟล์ตั้งค่า MCP (Claude Desktop: Settings › Developer › Edit Config)" text={json} />
      <ul className="list-disc space-y-1 pr-3 pl-8 text-[12px] leading-snug text-secondary">
        <li>แอป AI จะเรียก LAAB ตัวนี้ ถ้า LAAB ไม่ได้เปิดอยู่จะถูกเปิดในพื้นหลังให้</li>
        <li>ทุกอย่างที่ AI ทำ ทำในชื่อบัญชี LINE ของคุณ “ถามก่อน” จะเปิดหน้าต่างให้คุณกดอนุญาตทุกครั้ง</li>
        <li>โปรแกรมใดก็ได้ที่รันในบัญชีผู้ใช้ Mac นี้เชื่อมต่อได้ขณะเปิด MCP อยู่ ปิด MCP เมื่อไม่ได้ใช้</li>
        <li>ข้อความในแชทมาจากคนอื่น อาจมีข้อความที่พยายามสั่ง AI ให้ทำสิ่งที่คุณไม่ได้ขอ จึงควรให้ “ถามก่อน” สำหรับการส่งและการจัดการ</li>
      </ul>
    </div>
  );
}

const outcomeLabel = { ok: "สำเร็จ", denied: "ไม่อนุญาต", error: "ผิดพลาด" } as const;
const outcomeClass = { ok: "text-line-green", denied: "text-badge-red", error: "text-badge-red" } as const;

function ActivityTab() {
  const { data: items = [] } = useQuery(q.mcpActivity);
  if (!items.length) return <p className="py-10 text-center text-[13px] text-secondary">ยังไม่มีอะไร สิ่งที่แอป AI ทำผ่าน MCP จะแสดงที่นี่ (ไม่เก็บหลังปิดแอป)</p>;
  return (
    <div className="flex flex-col gap-1.5">
      <div className={groupBox}>
        {items.map((a, i) => (
          <div key={i} className={`flex gap-3 px-3 py-2 text-[12.5px] ${i ? "border-t border-separator" : ""}`}>
            <span className="w-11 shrink-0 text-secondary tabular-nums">{new Date(a.time).toLocaleTimeString("th-TH", { hour: "2-digit", minute: "2-digit" })}</span>
            <span className="min-w-0 flex-1">
              <span className="selectable whitespace-pre-line">{a.summary}</span>
              <span className="block text-[11px] text-secondary">
                {a.client} · {a.tool}
              </span>
            </span>
            <span className={`shrink-0 ${outcomeClass[a.outcome]}`}>{outcomeLabel[a.outcome]}</span>
          </div>
        ))}
      </div>
      <button className="self-end px-3 pt-1 text-[12px] text-secondary hover:text-primary" onClick={() => api.clearMcpActivity()}>
        ล้างกิจกรรม
      </button>
    </div>
  );
}

/** The master switch, with whether an AI app is connected right now. */
function Status({ settings, save }: { settings: McpSettings; save: (s: McpSettings) => void }) {
  const { data: setup } = useQuery({ queryKey: ["mcp-setup", settings.enabled], queryFn: api.mcpSetup, staleTime: 0, refetchInterval: settings.enabled ? 3000 : false });
  const status = !settings.enabled
    ? "ปิดอยู่"
    : !setup
      ? "กำลังเปิด…"
      : !setup.listening
        ? "เปิดไม่สำเร็จ อาจมี LAAB อีกตัวเปิดอยู่"
        : setup.clients > 0
          ? `แอป AI เชื่อมต่ออยู่ ${setup.clients} แอป`
          : "เปิดอยู่ ยังไม่มีแอปเชื่อมต่อ";
  const tone = !settings.enabled ? "bg-secondary/60" : setup?.listening === false ? "bg-badge-red" : setup?.clients ? "bg-line-green" : "bg-[#e9a23b]";
  return (
    <div className={`${groupBox} flex items-center gap-3 px-3 py-3`}>
      <span className={`grid size-9 shrink-0 place-items-center rounded-[9px] text-white transition-colors duration-200 ${settings.enabled ? "bg-line-green" : "bg-black/30"}`}>
        <PlugIcon size={20} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13.5px] font-medium">ให้แอป AI ใช้ LAAB</span>
        <span className="flex items-center gap-1.5 text-[12px] text-secondary">
          <span className={`size-1.5 shrink-0 rounded-full ${tone}`} />
          <span className="truncate">{status}</span>
        </span>
      </span>
      <Switch checked={settings.enabled} onChange={(enabled) => save({ ...settings, enabled })} label="ให้แอป AI ใช้ LAAB ผ่าน MCP" />
    </div>
  );
}

/** ⋯ › MCP: what AI apps may do in LAAB, how to connect them, and what they did. */
export function McpSheet({ onDone }: { onDone: () => void }) {
  const { data: settings } = useQuery(q.mcp);
  const [tab, setTab] = useState<Tab>("access");
  if (!settings) return null;
  const save = (next: McpSettings) => {
    queryClient.setQueryData(["mcp"], next);
    api.setMcpSettings(next).catch((e) => {
      notice(errorText(e));
      queryClient.invalidateQueries({ queryKey: ["mcp"] });
    });
  };
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="grid grid-cols-[1fr_auto_1fr] items-center px-5 pt-4 pb-3">
        <Dialog.Title className="text-[15px] font-semibold">MCP</Dialog.Title>
        <Tabs
          value={tab}
          options={[
            { id: "access", label: "สิทธิ์" },
            { id: "connect", label: "เชื่อมต่อ" },
            { id: "activity", label: "กิจกรรม" },
          ]}
          onChange={setTab}
        />
      </header>
      <div className="px-5 pb-4">
        <Status settings={settings} save={save} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col border-t border-separator">
        {tab === "access" && <AccessTab settings={settings} save={save} />}
        {tab !== "access" && <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{tab === "connect" ? <ConnectTab /> : <ActivityTab />}</div>}
      </div>
      <div className="flex justify-end border-t border-separator px-5 py-3">
        <button className={buttonClass(true)} onClick={onDone}>
          เสร็จ
        </button>
      </div>
    </div>
  );
}
