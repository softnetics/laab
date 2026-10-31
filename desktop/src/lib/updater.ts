import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createStore, useStore } from "@tanstack/react-store";

const CHECK_EVERY_MS = 30 * 60 * 1000;

/** A newer release that has finished downloading and waits for a restart. */
const ready = createStore<Update | null>(null);
let started = false;
let lastCheck = 0;
let running: Promise<void> | null = null;
/** Told the version as soon as a check finds one, before it downloads. */
let onFound: ((version: string) => void) | null = null;

async function checkOnce() {
  if (ready.state) return;
  const update = await check();
  if (!update) return;
  onFound?.(update.version);
  await update.download();
  ready.setState(() => update);
}

/** One check at a time: a click during a background check waits for it. */
function checkShared() {
  lastCheck = Date.now();
  running ??= checkOnce().finally(() => {
    running = null;
  });
  return running;
}

/** Checks GitHub Releases at launch, every 30 minutes, and when the window
 *  comes back into focus after that long (timers stop while the Mac sleeps),
 *  and downloads a newer version in the background. Failures (offline,
 *  GitHub down) wait for the next check. Dev builds don't check. */
export function startUpdateChecks() {
  if (import.meta.env.DEV || started) return;
  started = true;
  const run = () => checkShared().catch(() => {});
  run();
  setInterval(run, CHECK_EVERY_MS);
  window.addEventListener("focus", () => {
    if (Date.now() - lastCheck >= CHECK_EVERY_MS) run();
  });
}

/** "Check for updates" in the menu. `say` gets each line for the status bar.
 *  A newer version shows up in the update bar once it has downloaded. */
export async function checkNow(say: (line: string) => void) {
  if (import.meta.env.DEV) return say("รุ่นพัฒนาไม่ตรวจหาอัปเดต");
  say("กำลังตรวจหาอัปเดต…");
  onFound = (v) => say(`กำลังดาวน์โหลด LAAB ${v}…`);
  try {
    await checkShared();
  } catch (e) {
    return say(`ตรวจหาอัปเดตไม่ได้: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    onFound = null;
  }
  const update = ready.state;
  say(update ? `LAAB ${update.version} พร้อมติดตั้งแล้ว` : `LAAB ${await getVersion()} เป็นเวอร์ชันล่าสุดแล้ว`);
}

export const useReadyVersion = () => useStore(ready, (u) => u?.version ?? null);

export async function installAndRestart() {
  await ready.state?.install();
  await relaunch();
}
