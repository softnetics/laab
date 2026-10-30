import { ChatView } from "./ChatView";
import { ConfirmDialog, Sheets } from "./Dialogs";
import { ProfileSheet } from "./ProfileSheet";
import { RawInspector } from "./RawInspector";

/** A chat in its own window. Escape only closes popups here. */
export function ChatWindow({ chatId }: { chatId: string }) {
  return (
    <div className="h-full">
      <ChatView chatId={chatId} main={false} />
      <RawInspector chatId={chatId} />
      <ConfirmDialog />
      <Sheets />
      <ProfileSheet main={false} />
    </div>
  );
}
