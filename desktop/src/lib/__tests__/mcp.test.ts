import { describe, expect, it } from "vitest";
import { resolve, setupSnippets } from "../mcp";
import type { McpSettings } from "../types";

const settings: McpSettings = {
  enabled: true,
  defaults: { read: "on", send: "ask", manage: "ask", account: "off" },
  rules: [
    { kind: "type", id: "group", levels: { send: "off" } },
    { kind: "folder", id: "work", levels: { send: "on" } },
    { kind: "folder", id: "family", levels: { read: "ask", send: "ask" } },
    { kind: "chat", id: "c1", levels: { read: "off", send: null } },
  ],
};
const chat = (id: string, type: string, folders: string[] = []) => ({ kind: "chat" as const, id, type, folders });

describe("resolve", () => {
  it("is off while MCP is off", () => {
    expect(resolve({ ...settings, enabled: false }, "read", { kind: "global" })).toBe("off");
  });
  it("matches mcp.rs: chat, then strictest folder, then kind, then default", () => {
    expect(resolve(settings, "send", chat("u1", "dm"))).toBe("ask");
    expect(resolve(settings, "send", chat("c2", "group"))).toBe("off");
    expect(resolve(settings, "send", chat("c2", "group", ["work"]))).toBe("on");
    expect(resolve(settings, "send", chat("c2", "group", ["work", "family"]))).toBe("ask");
    expect(resolve(settings, "read", chat("c1", "group", ["family"]))).toBe("off");
    expect(resolve(settings, "send", chat("c1", "group", ["work"]))).toBe("on");
    expect(resolve(settings, "send", { kind: "folder", id: "work" })).toBe("on");
  });
  it("can leave a rule out to show what it falls back to", () => {
    expect(resolve(settings, "read", chat("c1", "group"), settings.rules[3])).toBe("on");
  });
});

describe("setupSnippets", () => {
  it("quotes paths and passes env", () => {
    const { claudeCode, json } = setupSnippets({
      listening: true,
      clients: 0,
      socket: "/x/mcp.sock",
      command: "/Applications/My LAAB.app/Contents/MacOS/laab",
      args: ["mcp"],
      env: [["LAAB_DATA_DIR", "/tmp/laab test"]],
    });
    expect(claudeCode).toBe("claude mcp add laab -e 'LAAB_DATA_DIR=/tmp/laab test' -- '/Applications/My LAAB.app/Contents/MacOS/laab' mcp");
    expect(JSON.parse(json)).toEqual({
      mcpServers: { laab: { command: "/Applications/My LAAB.app/Contents/MacOS/laab", args: ["mcp"], env: { LAAB_DATA_DIR: "/tmp/laab test" } } },
    });
  });
});
