import { execFile } from "node:child_process";
import { closeSync, existsSync, openSync, readSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);

export const DB_PATH = join(
  homedir(),
  "Library/Application Support/Laab/laab.sqlite",
);

/** LAAB encrypts laab.sqlite with SQLCipher. */
const SQLCIPHER = [
  "/opt/homebrew/bin/sqlcipher",
  "/usr/local/bin/sqlcipher",
].find((p) => existsSync(p));

/** Keychain services that hold the key, newest first. LAAB moves the key
 *  from `app.laab.mac` the next time it opens. */
const KEY_SERVICES = ["tech.softnetics.laab", "app.laab.mac"];

/** The key LAAB keeps in the login keychain (64 hex characters). */
async function databaseKey(): Promise<string> {
  let key = "";
  for (const service of KEY_SERVICES) {
    try {
      const { stdout } = await run("/usr/bin/security", [
        "find-generic-password",
        "-s",
        service,
        "-a",
        "database-key",
        "-w",
      ]);
      key = stdout.trim();
      break;
    } catch {
      // Not under this service; try the next.
    }
  }
  if (!key) throw new Error("ไม่พบกุญแจฐานข้อมูลใน Keychain เปิด LAAB ก่อน");
  if (!/^[0-9a-f]{64}$/i.test(key))
    throw new Error("กุญแจฐานข้อมูลใน Keychain ไม่ถูกต้อง");
  return key;
}

/** Run `sql` read-only and return sqlcipher's JSON output. */
async function runSQL(database: string, sql: string): Promise<string> {
  if (!SQLCIPHER)
    throw new Error("ต้องติดตั้ง sqlcipher: brew install sqlcipher");
  const sqlcipher = SQLCIPHER;
  const key = await databaseKey();
  return new Promise((resolve, reject) => {
    const child = execFile(
      sqlcipher,
      ["-readonly", "-json", database],
      { maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) =>
        error
          ? reject(new Error(stderr.trim() || error.message))
          : resolve(stdout),
    );
    // The key goes in on stdin so it never shows up in `ps`. PRAGMA key
    // prints "ok", which would break the JSON, so it runs with output off.
    child.stdin?.end(
      `.output /dev/null\nPRAGMA key = "x'${key}'";\n.output stdout\n${sql};\n`,
    );
  });
}

/** A file that exists and is not one of LAAB's encrypted cache files. */
export function isPlainFile(path: string): boolean {
  let fd: number;
  try {
    fd = openSync(path, "r");
  } catch {
    return false;
  }
  try {
    const head = Buffer.alloc(8);
    readSync(fd, head, 0, 8, 0);
    return head.toString("latin1") !== "LAABENC1";
  } finally {
    closeSync(fd);
  }
}

export type Kind = "chat" | "message" | "file";

export type Hit = {
  kind: Kind;
  chatID: string;
  chatName: string;
  chatKind: string;
  avatar: string | null;
  messageID: string | null;
  text: string;
  fileName: string | null;
  time: number;
};

type Row = {
  kind: Kind;
  chat_id: string;
  chat_name: string | null;
  chat_kind: string | null;
  avatar: string | null;
  message_id: string | null;
  text: string | null;
  file_name: string | null;
  time: number | null;
  sender: string | null;
};

/** Mirrors Store::search in desktop/src-tauri/src/store.rs, so results match ⌘K. */
export async function search(
  query: string,
  kinds: Set<Kind>,
  database = DB_PATH,
): Promise<Hit[]> {
  const term = query.trim();
  const statements: string[] = [];
  // Latest incoming sender of a group: a friend or group member name (people, kept by the app), or NULL.
  const sender =
    "CASE WHEN kind = 'group' THEN (SELECT COALESCE((SELECT u.name FROM chats u WHERE u.id = m.sender_id AND u.name <> u.id), (SELECT p.name FROM people p WHERE p.id = m.sender_id)) FROM messages m WHERE m.chat_id = chats.id AND m.mine = 0 ORDER BY m.created DESC LIMIT 1) END AS sender";
  const chatColumns = `'chat' AS kind, id AS chat_id, name AS chat_name, kind AS chat_kind, avatar, NULL AS message_id, preview AS text, NULL AS file_name, activity AS time, ${sender}`;

  if (!term) {
    if (kinds.has("chat"))
      statements.push(
        `SELECT ${chatColumns} FROM chats ORDER BY activity DESC LIMIT 30`,
      );
  } else {
    const like = quote(`%${term.replace(/[\\%_]/g, (c) => `\\${c}`)}%`);
    if (kinds.has("chat")) {
      statements.push(
        `SELECT ${chatColumns} FROM chats WHERE name LIKE ${like} ESCAPE '\\' ORDER BY activity DESC LIMIT 30`,
      );
    }
    if (kinds.has("message") || kinds.has("file")) {
      const types = [
        kinds.has("message") ? "m.content_type <> 'FILE'" : null,
        kinds.has("file") ? "m.content_type = 'FILE'" : null,
      ]
        .filter(Boolean)
        .join(" OR ");
      const columns =
        "CASE m.content_type WHEN 'FILE' THEN 'file' ELSE 'message' END AS kind, m.chat_id, c.name AS chat_name, c.kind AS chat_kind, c.avatar, m.id AS message_id, m.body AS text, m.file_name, m.created AS time, NULL AS sender";
      // The trigram index needs three characters; shorter terms fall back to LIKE.
      const match =
        [...term].length >= 3
          ? `m.id IN (SELECT id FROM message_search WHERE message_search MATCH ${quote(`"${term.replace(/"/g, '""')}"`)})`
          : `(m.body LIKE ${like} ESCAPE '\\' OR m.file_name LIKE ${like} ESCAPE '\\')`;
      statements.push(
        `SELECT ${columns} FROM messages m LEFT JOIN chats c ON c.id = m.chat_id WHERE ${match} AND (${types}) ORDER BY m.created DESC LIMIT 80`,
      );
    }
  }
  if (!statements.length) return [];

  const sql = statements.map((s) => `SELECT * FROM (${s})`).join(" UNION ALL ");
  const stdout = await runSQL(database, sql);
  const rows: Row[] = stdout.trim() ? JSON.parse(stdout) : [];
  return rows.map((row) => ({
    kind: row.kind,
    chatID: row.chat_id,
    chatName: row.chat_name ?? "แชท",
    chatKind: row.chat_kind ?? "dm",
    avatar: row.avatar,
    messageID: row.message_id,
    text:
      row.kind === "chat"
        ? chatPreview(row.text ?? "", row.chat_kind ?? "dm", row.sender)
        : (row.text ?? ""),
    fileName: row.file_name,
    time: row.time ?? 0,
  }));
}

/**
 * The sidecar prefixes previews with "เขา: "/"คุณ: ", which suits 1:1 chats.
 * Groups replace "เขา" with the latest incoming sender; "คุณ" stays.
 */
export function chatPreview(
  preview: string,
  chatKind: string,
  sender: string | null,
): string {
  if (chatKind !== "group" || /^(คุณ|You): /.test(preview)) return preview;
  const text = preview.replace(/^(เขา|They): /, "");
  return sender && text ? `${sender}: ${text}` : text;
}

function quote(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

export function chatURL(hit: Hit): string {
  const base = `laab://chat/${encodeURIComponent(hit.chatID)}`;
  return hit.messageID
    ? `${base}?message=${encodeURIComponent(hit.messageID)}`
    : base;
}
