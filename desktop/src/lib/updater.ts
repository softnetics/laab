import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createStore, useStore } from "@tanstack/react-store";

const CHECK_EVERY_MS = 30 * 60 * 1000;

/** A newer release that has finished downloading and waits for a restart. */
const ready = createStore<Update | null>(null);
let started = false;
let lastCheck = 0;

async function checkOnce() {
  if (ready.state) return;
  const update = await check();
  if (!update) return;
  await update.download();
  ready.setState(() => update);
}

/** Checks GitHub Releases at launch, every 30 minutes, and when the window
 *  comes back into focus after that long (timers stop while the Mac sleeps),
 *  and downloads a newer version in the background. Failures (offline,
 *  GitHub down) wait for the next check. Dev builds don't check. */
export function startUpdateChecks() {
  if (import.meta.env.DEV || started) return;
  started = true;
  const run = () => {
    lastCheck = Date.now();
    checkOnce().catch(() => {});
  };
  run();
  setInterval(run, CHECK_EVERY_MS);
  window.addEventListener("focus", () => {
    if (Date.now() - lastCheck >= CHECK_EVERY_MS) run();
  });
}

export const useReadyVersion = () => useStore(ready, (u) => u?.version ?? null);

export async function installAndRestart() {
  await ready.state?.install();
  await relaunch();
}
