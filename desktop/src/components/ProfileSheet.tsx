import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorText, mediaUrl, queryClient, useSession } from "../lib/api";
import { openChat } from "../lib/nav";
import { notice, setUi, useUi } from "../lib/ui";
import { Avatar } from "./Avatar";
import { BlockIcon, BubbleIcon, CloseIcon, PersonPlusIcon, WarningIcon } from "./Icons";
import { Spinner } from "./kit";
import { Sheet } from "./Sheet";

/** Someone's profile, like LINE's: the cover behind, picture, name and
 *  status message in the middle, actions at the bottom. People who are not
 *  friends get LINE's scam warning. `main` is false in a detached chat
 *  window, which cannot open another chat. */
export function ProfileSheet({ main }: { main: boolean }) {
  const p = useUi((s) => s.profile);
  const me = useSession()?.profile?.mid;
  const mid = p?.mid ?? "";
  const self = !!me && mid === me;
  const { data, error } = useQuery({
    queryKey: ["profile", mid],
    queryFn: () => api.contactProfile(mid),
    enabled: !!p && !self,
    staleTime: 60_000,
  });
  const [busy, setBusy] = useState(false);
  const close = () => setUi({ profile: null });
  const name = data?.name ?? p?.name ?? "";
  const picture = data?.picture ?? p?.picture ?? null;

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["profile", mid] });
      queryClient.invalidateQueries({ queryKey: ["chats"] });
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const block = () => {
    // One dialog at a time: the confirmation replaces the card.
    close();
    setUi({
      confirm: {
        title: `บล็อค ${name} หรือไม่`,
        body: "คุณจะไม่ได้รับข้อความจากผู้ใช้นี้ ปลดบล็อคได้จากโปรไฟล์ของผู้ใช้นี้",
        action: "บล็อค",
        danger: true,
        run: () => api.setBlocked(mid, true).then(() => queryClient.invalidateQueries({ queryKey: ["profile", mid] })),
      },
    });
  };

  const actions: { label: string; icon: React.ReactNode; onClick: () => void }[] = [];
  if (data && !self) {
    if (data.blocked) {
      actions.push({ label: "ปลดบล็อค", icon: <BlockIcon size={30} />, onClick: () => run(() => api.setBlocked(mid, false)) });
    } else if (data.friend) {
      if (main) actions.push({ label: "แชท", icon: <BubbleIcon size={30} />, onClick: () => (close(), openChat(mid)) });
    } else {
      actions.push({ label: "เพิ่มเพื่อน", icon: <PersonPlusIcon size={30} />, onClick: () => run(() => api.addFriend(mid)) });
      actions.push({ label: "บล็อค", icon: <BlockIcon size={30} />, onClick: block });
    }
  }
  const stranger = !!data && !self && !data.friend && !data.blocked;

  return (
    <Sheet open={!!p} onClose={close} className="h-[min(640px,calc(100vh-48px))] w-[360px] rounded-[18px]! bg-[#2a2a2a]! text-white">
      {/* The cover, or the profile picture blurred when there is none. A
          gradient keeps the white text readable on any photo. */}
      <div className="absolute inset-0 overflow-hidden">
        {data?.cover ? (
          <img src={mediaUrl.cover(data.cover)} className="h-full w-full object-cover" draggable={false} alt="" />
        ) : (
          picture && (
            <img
              src={mediaUrl.avatar(mid, picture)}
              className="h-full w-full scale-125 object-cover blur-lg brightness-75"
              draggable={false}
              alt=""
            />
          )
        )}
        <div className="absolute inset-0 bg-linear-to-b from-black/25 via-black/15 to-black/60" />
      </div>
      <button aria-label="ปิด" onClick={close} className="absolute top-4 right-4 z-10 grid h-9 w-9 place-items-center text-white/90 hover:text-white">
        <CloseIcon size={26} />
      </button>
      <div className="relative flex h-full flex-col">
        <div className="flex flex-1 flex-col items-center justify-center px-8 text-center">
          <Avatar id={mid} name={name} picture={picture} size={124} />
          <div className="selectable mt-5 max-w-full truncate text-[26px] font-medium drop-shadow">{name}</div>
          {data?.statusMessage && (
            <div className="selectable mt-1.5 line-clamp-3 text-[14.5px] whitespace-pre-line text-white/90 drop-shadow">{data.statusMessage}</div>
          )}
          {error && <div className="mt-3 text-[12.5px] text-white/80">{errorText(error)}</div>}
        </div>
        {stranger && (
          <div className="mx-6 mb-6 flex items-center gap-3.5 rounded-xl bg-white px-4 py-3.5 text-left text-[14px] text-[#111]">
            <WarningIcon size={28} className="shrink-0 text-badge-red" />
            โปรดระวังมิจฉาชีพหลอกลวงผ่าน LINE
          </div>
        )}
        <div className="flex min-h-[96px] items-start justify-around px-6 pb-7">
          {!data && !error && !self && (
            <div className="pt-4 text-white/80">
              <Spinner size={18} />
            </div>
          )}
          {actions.map((a) => (
            <button
              key={a.label}
              disabled={busy}
              onClick={a.onClick}
              className="flex min-w-20 flex-col items-center gap-2 text-[13.5px] text-white transition-transform duration-150 ease-out active:scale-[0.96] disabled:opacity-60"
            >
              {a.icon}
              {a.label}
            </button>
          ))}
        </div>
      </div>
    </Sheet>
  );
}
