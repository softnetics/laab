import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import { notice, setUi, useUi } from "../lib/ui";
import { buttonClass, Sheet } from "./Sheet";

export function FolderDialog() {
  const state = useUi((s) => s.folderDialog);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const rename = state?.rename;
  useEffect(() => setName(state?.rename?.name ?? ""), [state]);
  const close = () => setUi({ folderDialog: null });
  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      if (rename) {
        await api.renameFolder(rename.id, name);
      } else {
        const w = await api.addWorkspace(name);
        if (state?.chatId) await api.setPinned(state.chatId, w.id, true);
        setUi({ filter: `folder:${w.id}` });
      }
      close();
    } catch (e) {
      notice(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Sheet open={!!state} onClose={close} className="w-[360px]" title={rename ? "เปลี่ยนชื่อโฟลเดอร์" : "สร้างโฟลเดอร์แชท"}>
      <div className="flex flex-col gap-4 p-5">
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.nativeEvent.isComposing && create()}
          placeholder="ชื่อโฟลเดอร์"
          aria-label="ชื่อโฟลเดอร์"
          className="selectable h-9 rounded-md bg-field px-3 text-[14px] outline-none"
        />
        <div className="flex justify-end gap-2">
          <button className={buttonClass()} onClick={close}>
            ยกเลิก
          </button>
          <button className={buttonClass(true)} disabled={!name.trim() || busy} onClick={create}>
            {rename ? "บันทึก" : "สร้าง"}
          </button>
        </div>
      </div>
    </Sheet>
  );
}
