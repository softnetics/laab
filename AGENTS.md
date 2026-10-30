# Agent notes

## Building and releasing

The app is the Tauri app in `desktop/`. See `desktop/README.md`.

Building and releasing need macOS (`codesign`, `hdiutil`). On Linux, check script edits with `bash -n` and tell the user the scripts were not run.

- Build `dist/LAAB.app` only through `./scripts/build-desktop.sh`. It handles steps a manual copy misses:
  - It replaces the bundle instead of overwriting it, because macOS caches code signatures per inode.
  - It signs with the first "Developer ID Application" certificate in the keychain (or `APPLE_SIGNING_IDENTITY`), with hardened runtime. Without one, it signs the whole bundle ad hoc. Unsigned, macOS drops the app's notifications.
- The app version comes from the `VERSION` environment variable. There is no `VERSION` file.
- To check packaging, run `./scripts/release.sh <version> --dry-run`. It runs on any branch, packages through `scripts/package.sh` locally, and creates nothing outside `dist/`.
- A real release runs `.github/workflows/release.yml` on GitHub Actions; `release.sh <version>` only starts it. The workflow tests, builds, signs and notarizes, writes the changelog with `scripts/release-notes.sh`, then creates the tag and the GitHub release in one step, so a failed run leaves no tag. Before you start it, get the user's confirmation for that version.
- The workflow reads its signing and notarization credentials from repo secrets (listed at the top of `release.yml`). Do not set or change those secrets unless the user asks.
- Releases ship a `.dmg` by default. Use `--zip` only when the user asks for a zip.
- The app updates itself from `latest.json` on the latest GitHub release (`plugins.updater` in `tauri.conf.json`, `desktop/src/lib/updater.ts`). `package.sh` writes `latest.json` and the signed `.app.tar.gz` when `TAURI_SIGNING_PRIVATE_KEY` (or `_PATH`) and `_PASSWORD` are set. Never print, commit, or regenerate the updater private key: installed apps only accept updates signed with it.
- With a Developer ID certificate, `package.sh` notarizes and staples the app and the `.dmg`. Locally it uses the notarytool keychain profile `laab-notary` (see the top of `package.sh`). Never print, copy into the repo, or commit an App Store Connect `.p8` key.

## Developing (`desktop/`)

- To check it without a LINE account, run `LAAB_DEMO=1 pnpm tauri dev` in `desktop/`. Demo data lives in a temporary folder.
- Do not point a dev build at the user's real data folder unless they ask. Use `LAAB_DATA_DIR` for a scratch folder.
- Run `pnpm test` and `cargo test` (in `desktop/src-tauri`) after changes.
- Usage statistics and crash reports (`src-tauri/src/analytics.rs`, `usage.rs`, `crash.rs`) are opt-in. Never add an event property that holds data from LINE or from the user (text, names, mids, chat ids, URLs, file names, error or panic messages). Add new properties to the test's list in `analytics.rs`. Only a problem report the user previews and confirms (`report.rs`) may carry content.
- `laab.sqlite` is encrypted with SQLCipher; the key is in the login keychain (`tech.softnetics.laab` / `database-key`). Do not read that key or decrypt the user's real database unless they ask. Reading it from a shell opens a keychain prompt on their screen.
