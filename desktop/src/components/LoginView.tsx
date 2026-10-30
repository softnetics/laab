import { motion } from "motion/react";
import QRCode from "qrcode";
import { useEffect, useState } from "react";
import { api, errorText, useSession } from "../lib/api";
import { notice } from "../lib/ui";
import { BubbleIcon } from "./Icons";
import { Spinner } from "./kit";

/** Email or phone number and password. LINE asks the phone to confirm with
 *  a PIN, shown below like the QR flow's. */
function PasswordLogin({ busy }: { busy: boolean }) {
  const [id, setId] = useState("");
  const [password, setPassword] = useState("");
  const [sending, setSending] = useState(false);
  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!id.trim() || !password || busy || sending) return;
    setSending(true);
    api
      .loginPassword(id, password)
      .catch((e) => notice(errorText(e)))
      .finally(() => setSending(false));
  };
  const field = "selectable h-10 w-64 rounded-lg bg-field px-3 text-[14px] outline-none placeholder:text-secondary";
  return (
    // A form, so Return submits from either field and password managers fill it.
    <form className="flex flex-col items-center gap-2" onSubmit={submit}>
      <input
        value={id}
        onChange={(e) => setId(e.target.value)}
        placeholder="อีเมล หรือเบอร์โทร (+66…)"
        aria-label="อีเมล หรือเบอร์โทร"
        autoComplete="username"
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        className={field}
      />
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="รหัสผ่าน"
        aria-label="รหัสผ่าน"
        autoComplete="current-password"
        className={field}
      />
      <button
        type="submit"
        disabled={busy || sending || !id.trim() || !password}
        className="mt-1 flex h-10 w-64 items-center justify-center gap-2 rounded-lg bg-line-green text-[14px] font-semibold text-white disabled:opacity-50"
      >
        {(busy || sending) && <Spinner />}
        เข้าสู่ระบบ
      </button>
      <p className="max-w-64 text-center text-[11px] leading-relaxed text-secondary">
        ใช้แบบ LINE for Windows จะทำให้ LINE บน Windows ที่ล็อกอินอยู่ถูกออกจากระบบ (LINE for Mac ไม่กระทบ)
      </p>
    </form>
  );
}

export function LoginView() {
  const s = useSession();
  const [qr, setQr] = useState<string | null>(null);
  const [method, setMethod] = useState<"qr" | "password">("qr");
  useEffect(() => {
    if (!s?.qrUrl) return setQr(null);
    QRCode.toDataURL(s.qrUrl, { errorCorrectionLevel: "M", margin: 1, width: 440 }).then(setQr);
  }, [s?.qrUrl]);
  const busy = s?.state === "logging_in" || s?.state === "starting";
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 px-8 text-center">
      <BubbleIcon size={52} className="text-line-green" />
      <h1 className="text-[22px] font-bold">เข้าสู่ระบบ LINE</h1>
      <p className="text-[13px] text-secondary">{s?.status}</p>
      {!qr && !s?.pin && method === "password" && <PasswordLogin busy={busy} />}
      {!qr && !s?.pin && method === "qr" && (
        <motion.button
          whileTap={{ scale: 0.97 }}
          disabled={busy}
          onClick={() => api.loginQr().catch((e) => notice(errorText(e)))}
          className="flex h-11 w-60 items-center justify-center gap-2 rounded-lg bg-line-green text-[14px] font-semibold text-white disabled:opacity-60"
        >
          {busy && <Spinner />}
          เข้าสู่ระบบด้วย QR Code
        </motion.button>
      )}
      {qr && !s?.pin && (
        <motion.div
          initial={{ opacity: 0, scale: 0.96 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.2, ease: [0.32, 0.72, 0, 1] }}
          className="flex flex-col items-center gap-3">
          <div className="rounded-2xl bg-white p-3 shadow">
            <img src={qr} width={220} height={220} alt="QR" />
          </div>
          <p className="text-[13px] text-secondary">เปิด LINE บนโทรศัพท์ แล้วสแกน QR นี้</p>
        </motion.div>
      )}
      {s?.pin && (
        <motion.div
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: [0.32, 0.72, 0, 1] }}
          className="flex flex-col items-center gap-2">
          <div className="font-mono text-[30px] font-bold tracking-[0.2em]">{s.pin}</div>
          <p className="text-[13px] text-secondary">ใส่รหัสนี้ใน LINE บนโทรศัพท์</p>
        </motion.div>
      )}
      {!qr && !s?.pin && (
        <button
          className="text-[12.5px] text-secondary underline hover:text-primary"
          onClick={() => setMethod(method === "qr" ? "password" : "qr")}
        >
          {method === "qr" ? "เข้าสู่ระบบด้วยอีเมลหรือเบอร์โทร" : "เข้าสู่ระบบด้วย QR Code"}
        </button>
      )}
      <p className="mt-6 max-w-sm text-[11px] leading-relaxed text-secondary">
        ไคลเอนต์นี้ใช้ line-rs (LINEJS เวอร์ชัน Rust) ซึ่งไม่ใช่ของ LINE อย่างเป็นทางการ การใช้งานอาจขัดกับเงื่อนไขบริการของ LINE
      </p>
    </div>
  );
}
