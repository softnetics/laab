import {
  Action,
  ActionPanel,
  closeMainWindow,
  Icon,
  Image,
  Keyboard,
  LaunchProps,
  List,
  open,
} from "@raycast/api";
import { showFailureToast, useCachedPromise } from "@raycast/utils";
import { existsSync } from "node:fs";
import { useState } from "react";
import { chatURL, DB_PATH, Hit, isPlainFile, Kind, search } from "./laab-db";

const FILTERS: Record<string, Kind[]> = {
  all: ["chat", "message", "file"],
  chat: ["chat"],
  message: ["message"],
  file: ["file"],
};

const SECTIONS: { kind: Kind; title: string }[] = [
  { kind: "chat", title: "แชท" },
  { kind: "message", title: "ข้อความ" },
  { kind: "file", title: "ไฟล์" },
];

/** A deeplink can point at another database, e.g. LAAB's `--demo` data for screenshots. */
type LaunchContext = { database?: string };

export default function Command(
  props: LaunchProps<{ launchContext?: LaunchContext }>,
) {
  const database = props.launchContext?.database || DB_PATH;
  const [query, setQuery] = useState(props.fallbackText ?? "");
  const [filter, setFilter] = useState("all");
  const { data = [], isLoading } = useCachedPromise(
    (q: string, f: string, db: string) => search(q, new Set(FILTERS[f]), db),
    [query, filter, database],
    {
      keepPreviousData: true,
      execute: existsSync(database),
      onError: (error) => {
        showFailureToast(error, { title: "อ่านข้อมูล LAAB ไม่ได้" });
      },
    },
  );

  if (!existsSync(database)) {
    return (
      <List>
        <List.EmptyView
          icon={Icon.Warning}
          title="ยังไม่พบข้อมูล LAAB"
          description="เปิด LAAB และเชื่อม LINE ก่อน แล้วลองใหม่"
        />
      </List>
    );
  }

  return (
    <List
      isLoading={isLoading}
      filtering={false}
      throttle
      searchText={query}
      onSearchTextChange={setQuery}
      searchBarPlaceholder="ค้นหาแชท ข้อความ หรือชื่อไฟล์"
      searchBarAccessory={
        <List.Dropdown tooltip="ประเภท" storeValue onChange={setFilter}>
          <List.Dropdown.Item value="all" title="ทั้งหมด" />
          <List.Dropdown.Item value="chat" title="แชท" />
          <List.Dropdown.Item value="message" title="ข้อความ" />
          <List.Dropdown.Item value="file" title="ไฟล์" />
        </List.Dropdown>
      }
    >
      <List.EmptyView
        icon={Icon.MagnifyingGlass}
        title={query ? "ไม่พบผลลัพธ์" : "ไม่มีแชท"}
      />
      {SECTIONS.map(({ kind, title }) => {
        const hits = data.filter((hit) => hit.kind === kind);
        return (
          <List.Section
            key={kind}
            title={query.trim() ? title : "แชทล่าสุด"}
            subtitle={query.trim() ? String(hits.length) : undefined}
          >
            {hits.map((hit) => (
              <HitItem
                key={`${hit.kind}:${hit.chatID}:${hit.messageID ?? ""}`}
                hit={hit}
              />
            ))}
          </List.Section>
        );
      })}
    </List>
  );
}

function HitItem({ hit }: { hit: Hit }) {
  const title =
    hit.kind === "chat"
      ? hit.chatName
      : hit.kind === "file"
        ? (hit.fileName ?? hit.text)
        : oneLine(hit.text);
  const subtitle = hit.kind === "chat" ? oneLine(hit.text) : hit.chatName;
  return (
    <List.Item
      icon={icon(hit)}
      title={title}
      subtitle={subtitle}
      accessories={
        hit.time
          ? [
              {
                date: new Date(hit.time),
                tooltip: new Date(hit.time).toLocaleString("th-TH"),
              },
            ]
          : []
      }
      actions={
        <ActionPanel>
          <Action
            title="เปิดใน LAAB"
            icon={Icon.Message}
            onAction={async () => {
              await open(chatURL(hit));
              await closeMainWindow({ clearRootSearch: true });
            }}
          />
          <Action.CopyToClipboard
            title="คัดลอกข้อความ"
            content={hit.kind === "chat" ? hit.chatName : hit.text}
          />
          <Action.CopyToClipboard
            title="คัดลอกลิงก์"
            content={chatURL(hit)}
            shortcut={Keyboard.Shortcut.Common.Copy}
          />
        </ActionPanel>
      }
    />
  );
}

function icon(hit: Hit): Image.ImageLike {
  if (hit.kind === "file") return Icon.Document;
  if (hit.kind === "message") return Icon.SpeechBubble;
  if (hit.avatar && isPlainFile(hit.avatar))
    return { source: hit.avatar, mask: Image.Mask.Circle };
  return hit.chatKind === "group" ? Icon.TwoPeople : Icon.Person;
}

function oneLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}
