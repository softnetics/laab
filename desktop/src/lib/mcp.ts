import type { McpAccess, McpGroup, McpRule, McpRuleKind, McpSettings, McpSetup } from "./types";

export const GROUPS: { id: McpGroup; label: string; hint: string }[] = [
  { id: "read", label: "อ่านแชท", hint: "ข้อความ รูป สมาชิก โน้ต และเขียนร่างไว้ให้คุณกดส่งเอง" },
  { id: "send", label: "ส่งข้อความ", hint: "ข้อความ สติกเกอร์ ไฟล์ รีแอคชัน และโพสต์โน้ต ในชื่อคุณ" },
  { id: "manage", label: "จัดการแชท", hint: "แก้หรือยกเลิกข้อความ ปักหมุด ปิดเสียง ซ่อน และจัดโฟลเดอร์" },
  { id: "account", label: "ลบ บล็อก เข้าออกกลุ่ม", hint: "ลบประวัติแชท บล็อก เพิ่มเพื่อน ทำแล้วย้อนกลับยาก" },
];

export const ACCESS: { id: McpAccess; label: string }[] = [
  { id: "off", label: "ปิด" },
  { id: "ask", label: "ถามก่อน" },
  { id: "on", label: "อนุญาต" },
];

export const accessLabel = (a: McpAccess) => ACCESS.find((x) => x.id === a)?.label ?? a;

export const TYPES: { id: string; label: string }[] = [
  { id: "dm", label: "แชทกับเพื่อนทุกคน" },
  { id: "group", label: "กลุ่มทุกกลุ่ม" },
  { id: "bot", label: "บัญชีทางการทุกบัญชี" },
];

const rank: Record<McpAccess, number> = { off: 0, ask: 1, on: 2 };

export type McpScope = { kind: "global" } | { kind: "folder"; id: string } | { kind: "chat"; id: string; type: string; folders: string[] };

const ruleOf = (s: McpSettings, kind: McpRuleKind, id: string) => s.rules.find((r) => r.kind === kind && r.id === id);
const levelOf = (r: McpRule | undefined, g: McpGroup) => r?.levels[g] ?? null;

/** What a client may do, as `mcp.rs` decides it: a chat's rule, then the
 *  strictest of its folders' rules, then its kind's, then the defaults.
 *  `skip` leaves one rule out, to show what it falls back to. */
export function resolve(s: McpSettings, g: McpGroup, scope: McpScope, skip?: McpRule): McpAccess {
  if (!s.enabled) return "off";
  const settings = skip ? { ...s, rules: s.rules.filter((r) => r !== skip) } : s;
  const fallback = settings.defaults[g];
  if (scope.kind === "global") return fallback;
  if (scope.kind === "folder") return levelOf(ruleOf(settings, "folder", scope.id), g) ?? fallback;
  const own = levelOf(ruleOf(settings, "chat", scope.id), g);
  if (own) return own;
  const folders = scope.folders.map((f) => levelOf(ruleOf(settings, "folder", f), g)).filter((a): a is McpAccess => !!a);
  if (folders.length) return folders.reduce((a, b) => (rank[a] <= rank[b] ? a : b));
  return levelOf(ruleOf(settings, "type", scope.type), g) ?? fallback;
}

/** `'…'` around a shell word that needs it. */
const shell = (w: string) => (/^[\w@%+=:,./-]+$/.test(w) ? w : `'${w.replace(/'/g, `'\\''`)}'`);

/** How to add LAAB to Claude Code and to apps configured with JSON. */
export function setupSnippets(setup: McpSetup) {
  const env = setup.env.map(([k, v]) => `-e ${shell(`${k}=${v}`)} `).join("");
  const claudeCode = `claude mcp add laab ${env}-- ${[setup.command, ...setup.args].map(shell).join(" ")}`;
  const server: Record<string, unknown> = { command: setup.command, args: setup.args };
  if (setup.env.length) server.env = Object.fromEntries(setup.env);
  const json = JSON.stringify({ mcpServers: { laab: server } }, null, 2);
  return { claudeCode, json };
}
