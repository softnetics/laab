import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, q, queryClient } from "../lib/api";
import { imagesLoaded, snapshot } from "../lib/capture";
import { isGroupId } from "../lib/chatList";
import { buildRows } from "../lib/timeline";
import type { ChatMember, Message } from "../lib/types";
import { MessageRow, NoticeRow, type RowContext } from "./MessageRow";

interface Job {
  chatId: string;
  messages: Message[];
  ctx: RowContext;
  done: (png: Uint8Array) => void;
  fail: (e: unknown) => void;
}

let queue: Job[] = [];
const listeners = new Set<() => void>();
const changed = () => listeners.forEach((f) => f());

const noop = () => {};

/** Names and pictures the way the chat shows them. */
async function rowContext(chatId: string): Promise<RowContext> {
  const [session, chats, people] = await Promise.all([
    queryClient.fetchQuery(q.session),
    queryClient.fetchQuery(q.chats),
    queryClient.fetchQuery(q.people).catch(() => ({}) as Awaited<ReturnType<typeof api.people>>),
  ]);
  const group = isGroupId(chatId);
  const members: ChatMember[] = group ? await queryClient.fetchQuery(q.members(chatId)).catch(() => []) : [];
  const me = session.profile?.mid ?? "me";
  const cached = queryClient.getQueriesData<Message[]>({ queryKey: ["messages", chatId] }).flatMap(([, d]) => d ?? []);
  return {
    chatId,
    group,
    me,
    signedIn: false,
    name: (mid) => {
      if (mid === me || mid === "me") return session.profile?.displayName || "คุณ";
      const c = chats.chats.find((x) => x.id === mid && x.name !== x.id);
      return c?.name ?? members.find((m) => m.mid === mid)?.name ?? people[mid]?.name ?? "สมาชิก";
    },
    picture: (mid) => members.find((m) => m.mid === mid)?.picture ?? people[mid]?.picture ?? null,
    loadMembers: noop,
    find: (id) => cached.find((m) => m.id === id),
    bookmarked: () => false,
    onMarkUnreadFrom: noop,
  };
}

/** A PNG of `messages`, in this order, drawn as the chat draws them, with
 *  the sender's picture, name and time. */
export async function captureMessages(chatId: string, messages: Message[]): Promise<Uint8Array> {
  const ctx = await rowContext(chatId);
  return new Promise((done, fail) => {
    queue = [...queue, { chatId, messages, ctx, done, fail }];
    changed();
  });
}

/** Draws capture jobs off screen, one at a time, and answers the backend's
 *  `capture-request`s (MCP's capture_messages). */
export function CaptureHost() {
  const [job, setJob] = useState<Job | undefined>(queue[0]);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const update = () => setJob(queue[0]);
    listeners.add(update);
    update();
    return () => void listeners.delete(update);
  }, []);

  useEffect(() => {
    const off = listen<{ id: number; chatId: string; messages: Message[] }>("capture-request", ({ payload }) => {
      captureMessages(payload.chatId, payload.messages)
        .then((png) => invoke("capture_done", png, { headers: { "capture-id": String(payload.id) } }))
        .catch((e) => invoke("capture_failed", { id: payload.id, error: String(e) }));
    });
    return () => void off.then((f) => f());
  }, []);

  useLayoutEffect(() => {
    if (!job) return;
    let cancelled = false;
    (async () => {
      // Let replies, link previews and stickers settle, then wait for photos.
      await new Promise((r) => setTimeout(r, 250));
      await imagesLoaded(box.current!);
      await document.fonts.ready;
      return snapshot(box.current!);
    })()
      .then(job.done, job.fail)
      .finally(() => {
        if (cancelled) return;
        queue = queue.slice(1);
        changed();
      });
    return () => {
      cancelled = true;
    };
  }, [job]);

  if (!job) return null;
  const rows = buildRows(job.messages, job.ctx.name);
  return createPortal(
    // Off screen, as wide as the widest message up to a narrow chat's width.
    <div aria-hidden className="pointer-events-none fixed top-0 left-[-10000px]">
      <div ref={box} className="w-fit max-w-[560px] min-w-[240px] bg-chat px-3 pt-1 pb-4 text-primary [&_[data-capture-skip]]:hidden">
        {rows.map((r, i) =>
          r.notice ? (
            <NoticeRow key={r.message.id} m={r.message} ctx={job.ctx} />
          ) : (
            <MessageRow key={r.message.id} m={r.message} ctx={job.ctx} runStart={i === 0 || r.runStart} readers={[]} photos={r.photos} />
          ),
        )}
      </div>
    </div>,
    document.body,
  );
}
