import { save } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "motion/react";
import { useEffect } from "react";
import { api, errorText, mediaUrl } from "../lib/api";
import { notice, pushDismiss, setUi, useUi } from "../lib/ui";
import { CloseIcon, DocIcon } from "./Icons";

const IMAGE = /\.(jpe?g|png|gif|webp|heic|bmp)$/i;
const VIDEO = /\.(mp4|mov|m4v)$/i;
const DOC = /\.(pdf|txt)$/i;

/** The phone's viewer for attachments, which the desktop hands to the
 *  default app. Photos, videos and PDFs show in LAAB; other files can be
 *  saved to Files. */
export function FileViewer() {
  const v = useUi((s) => s.viewer);
  const close = () => setUi({ viewer: null });
  useEffect(() => (v ? pushDismiss(close) : undefined), [v]);
  const name = v?.name ?? v?.path.split("/").pop() ?? "";
  const src = v ? mediaUrl.local(v.path) : "";
  const saveFile = async () => {
    if (!v?.chatId || !v.messageId) return;
    try {
      const dest = await save({ defaultPath: name });
      if (dest) {
        await api.saveAttachment(v.chatId, v.messageId, dest);
        notice("บันทึกแล้ว");
      }
    } catch (e) {
      notice(errorText(e));
    }
  };
  return (
    <AnimatePresence>
      {v && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
          className="fixed inset-0 z-[60] flex flex-col bg-black text-white"
          style={{ colorScheme: "dark" }}
        >
          <div className="safe-top safe-x shrink-0">
            <div className="flex h-[52px] items-center gap-2 px-2">
              <button onClick={close} title="ปิด" className="grid h-11 w-11 place-items-center">
                <CloseIcon size={22} />
              </button>
              <span className="min-w-0 flex-1 truncate text-center text-[15px]">{name}</span>
              <button
                onClick={saveFile}
                disabled={!v.chatId}
                className="h-11 px-3 text-[16px] font-medium text-line-green disabled:opacity-0"
              >
                บันทึก
              </button>
            </div>
          </div>
          <div className="safe-bottom safe-x flex min-h-0 flex-1 items-center justify-center">
            {IMAGE.test(v.path) ? (
              <motion.img
                initial={{ scale: 0.96 }}
                animate={{ scale: 1 }}
                src={src}
                alt=""
                draggable={false}
                className="max-h-full max-w-full object-contain"
                onClick={close}
              />
            ) : VIDEO.test(v.path) ? (
              <video src={src} controls autoPlay playsInline className="max-h-full max-w-full" />
            ) : DOC.test(v.path) ? (
              <iframe src={src} title={name} className="h-full w-full bg-white" />
            ) : (
              <div className="flex flex-col items-center gap-3 px-8 text-center">
                <DocIcon size={56} className="text-white/60" />
                <div className="text-[16px] break-all">{name}</div>
                <p className="text-[13px] text-white/60">LAAB เปิดไฟล์ชนิดนี้ไม่ได้ กดบันทึกเพื่อเก็บไว้ในแอปไฟล์</p>
              </div>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
