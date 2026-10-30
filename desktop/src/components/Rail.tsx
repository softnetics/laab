import { Menu } from "@base-ui/react/menu";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorText, queryClient, useChatList, useSession } from "../lib/api";
import { openChat } from "../lib/nav";
import { notice, setUi, useUi } from "../lib/ui";
import { BookmarkIcon, BubbleIcon, MoreIcon, PersonIcon, PersonPlusIcon } from "./Icons";
import { itemClass, popupClass, Separator } from "./kit";

function RailButton({
  title,
  active,
  onClick,
  children,
  disabled,
}: {
  title: string;
  active?: boolean;
  onClick?: () => void;
  children: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <button
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={`no-drag relative grid h-10 w-11 place-items-center rounded-lg disabled:opacity-40 ${active ? "text-rail-icon-active" : "text-rail-icon hover:text-rail-icon-active"}`}
    >
      {children}
    </button>
  );
}

const megabytes = (n: number) => (n >= 1024 ** 3 ? `${(n / 1024 ** 3).toFixed(1)} GB` : `${Math.round(n / 1024 ** 2)} MB`);

/** LINE for Mac's left bar, without what LAAB does not have (VOOM,
 *  OpenChat, LINE services, screenshots). */
export function Rail() {
  const list = useChatList();
  const session = useSession();
  const page = useUi((s) => s.railPage);
  const dev = useUi((s) => s.developerMode);
  const unread = (list?.chats ?? []).reduce((n, c) => n + Math.max(0, c.unread), 0);
  const signedIn = session?.state === "signed_in";
  const me = session?.profile?.mid;
  const sync = () => api.refresh(true).catch((e) => notice(errorText(e)));
  const [menuOpen, setMenuOpen] = useState(false);
  const { data: cacheBytes } = useQuery({ queryKey: ["cache-size"], queryFn: api.cacheSize, enabled: menuOpen, staleTime: 0 });
  const clearCache = () =>
    setUi({
      confirm: {
        title: "ล้าง cache หรือไม่",
        body: "รูป วิดีโอ ไฟล์ และรูปโปรไฟล์ที่เก็บไว้ในเครื่องจะถูกลบ แล้วโหลดใหม่เมื่อเปิดดู ไฟล์ที่ LINE ลบจากเซิร์ฟเวอร์แล้วจะโหลดใหม่ไม่ได้",
        action: "ล้าง",
        danger: true,
        run: () => api.clearCache().then(() => queryClient.invalidateQueries()),
      },
    });
  return (
    <div className="drag flex h-full w-[62px] flex-col items-center gap-3 bg-rail pt-[44px] pb-3" data-tauri-drag-region>
      <RailButton title="เพื่อน" active={page === "friends"} onClick={() => setUi({ railPage: "friends" })}>
        <PersonIcon size={23} />
      </RailButton>
      <RailButton title="แชท" active={page === "chats"} onClick={() => setUi({ railPage: "chats" })}>
        <BubbleIcon size={23} />
        {unread > 0 && (
          <span className="absolute top-0 left-6 min-w-[18px] rounded-full bg-badge-red px-1 text-[10px] leading-[18px] font-bold text-white">
            {unread > 999 ? "999+" : unread}
          </span>
        )}
      </RailButton>
      <RailButton title="เพิ่มเพื่อน" active={page === "add"} onClick={() => setUi({ railPage: "add" })}>
        <PersonPlusIcon size={23} />
      </RailButton>
      <div className="flex-1" />
      <RailButton title="Keep Memo" disabled={!me} onClick={() => me && (setUi({ railPage: "chats" }), openChat(me))}>
        <BookmarkIcon size={21} />
      </RailButton>
      <Menu.Root onOpenChange={setMenuOpen}>
        <Menu.Trigger aria-label="เมนู" className="no-drag grid h-10 w-11 place-items-center rounded-lg text-rail-icon hover:text-rail-icon-active">
          <MoreIcon size={20} />
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner className="z-50" side="right" align="end" sideOffset={6}>
            <Menu.Popup className={`${popupClass} w-60`}>
              <div className="px-4 py-1 text-[12px] text-secondary">
                {session?.profile ? `${session.profile.displayName} · ${session.profile.device}` : ""}
                <div>{session?.status}</div>
              </div>
              <Separator />
              <Menu.Item className={itemClass} disabled={!signedIn} onClick={sync}>
                รีเฟรชแชท
              </Menu.Item>
              <Menu.CheckboxItem className={itemClass} checked={dev} onCheckedChange={(v) => setUi({ developerMode: v })}>
                <span className="w-4 text-center">{dev ? "✓" : ""}</span>
                เครื่องมือนักพัฒนา
              </Menu.CheckboxItem>
              {dev && (
                <Menu.Item className={itemClass} onClick={() => api.openLiffDevtools().catch((e) => notice(errorText(e)))}>
                  LIFF DevTools
                  <span className="ml-auto text-secondary">⌥⌘L</span>
                </Menu.Item>
              )}
              <Menu.Item className={itemClass} onClick={clearCache}>
                ล้าง cache
                {cacheBytes != null && <span className="ml-auto text-secondary">{megabytes(cacheBytes)}</span>}
              </Menu.Item>
              <Separator />
              {signedIn && (
                <Menu.Item className={itemClass} onClick={() => api.logout()}>
                  ออกจากระบบ
                </Menu.Item>
              )}
              <Menu.Item className={itemClass} onClick={() => api.quit()}>
                ออก
              </Menu.Item>
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
    </div>
  );
}
