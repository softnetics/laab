# LAAB desktop (Tauri)

The macOS app.

- **Frontend** (`src/`): React 19, TanStack Query, Router, Virtual, Hotkeys and Store, Base UI, Tailwind CSS 4, Motion.
- **Backend** (`src-tauri/`): Rust on [line-rs](https://github.com/softnetics/line-rs).

## Where data goes

Data lives in `~/Library/Application Support/Laab/`.

- `logs/laab.log` (and `laab.1.log`) is the app's log, kept under 2 MB. `crashes/` holds panic reports until the next launch sends or deletes them.
- `laab.sqlite` is encrypted with SQLCipher. The key is 32 random bytes in the Keychain (`tech.softnetics.laab` / `database-key`, see `src-tauri/src/keychain.rs`). The LINE session is in its `session` and `kv` tables.
- Files in `cache/media` and `cache/avatars` are encrypted with AES-256-GCM in 64 KB chunks (`src-tauri/src/vault.rs`), so `media://` can decrypt just the range a `<video>` asks for. Files left over from before encryption get encrypted in the background at launch. `cache/stickers` isn't encrypted, and neither is anything in demo mode.
- `cache/media` is capped at 2 GB (`src-tauri/src/cache.rs`). Past that, LAAB deletes the full files used least recently, before any thumbnail, until it's down to 80%, and clears the database paths that pointed at them. Signing out, or **ล้าง cache** (Clear cache) in the ⋯ menu, deletes the whole cache.
- An unencrypted database from an older build is encrypted on first launch, and the old session files (`linejs-rs-storage.json`, `auth-token.txt`, `auth-device.txt`) are moved into it and deleted. If the Keychain key can't open the database, LAAB renames it to `laab.sqlite.unreadable-<time>` and starts fresh, and you have to sign in again.

## Usage statistics and problem reports

LAAB asks once, after you sign in, whether to send anonymous usage statistics and crash reports to PostHog. Nothing is sent until you agree, and **ส่งสถิติการใช้งาน** (Send usage statistics) in the ⋯ menu turns it off again. Everything that can be sent is defined in Rust (`src-tauri/src/analytics.rs`, `usage.rs`, `crash.rs`), and the webview's CSP blocks PostHog.

- **Events**: `app_opened`, `app_updated` (with the previous version), `signed_in` (QR or password), `signed_out`, and once a day `daily_usage`: how many times each feature was used (`n_sent_text`, `n_opened_liff`, …) and the number of chats as a range. Each event carries the app version, the macOS version and the CPU architecture.
- **Errors**: `login_failed`, `session_ended`, `sync_failed`, `send_failed` and `database_unreadable`, each with a category (`network`, `auth`, `rate_limited`, `server`, `other`) worked out on your Mac from the error text, which isn't sent, plus the names of your last 20 actions (breadcrumbs).
- **Crashes** (`$exception`): Rust panics, saved to `crashes/` and sent at the next launch; native crashes from macOS's `~/Library/Logs/DiagnosticReports/laab-*.ips` since the last launch; and uncaught webview errors. They carry the error type, function names, source files and line numbers, never the error or panic message.
- Events go out under a random id that isn't tied to your LINE account. Signing out replaces it, and turning statistics off deletes it. PostHog is asked not to create a person profile or look up a location.
- Messages, names, LINE ids, chat lists, links and files are never sent automatically.

**รายงานปัญหา…** (Report a problem), in the ⋯ menu and a message's right-click menu, sends a report you write. It shows everything it will send in a box you can edit. A message is attached as its shape (field names and value types) unless you tick its content; the last 200 lines of the local log (`logs/laab.log`, never sent otherwise) only if you tick that. Reports can be sent with statistics off.

Release builds send only if `LAAB_POSTHOG_KEY` (and optionally `LAAB_POSTHOG_HOST`) was set at build time; otherwise the menu items don't show. Debug builds log events instead of sending them, also in demo mode, so the whole flow can be tried with `LAAB_DEMO=1`.

## Developing

You need Rust and pnpm.

```bash
cd desktop
pnpm install
LAAB_DEMO=1 pnpm tauri dev      # made-up data, no LINE connection
pnpm tauri dev                  # real LINE account
LAAB_DATA_DIR=/tmp/laab-test pnpm tauri dev   # a different data folder

pnpm test                       # vitest (UI logic)
cd src-tauri && cargo test      # store, message conversion, LIFF, notification rules
```

`LAAB_DATA_DIR` still uses the same Keychain key as your real data, so expect a Keychain prompt when the build is signed differently from the one you normally run. Demo mode doesn't touch the Keychain.

To build the `.app`, run `./scripts/build-desktop.sh` (set `VERSION=` if you want). It writes `dist/LAAB.app`, signed with the Developer ID certificate in your Keychain if there is one and ad hoc otherwise.

## iPhone

The same code builds as an iPhone app (Tauri 2 iOS; the Xcode project is in `src-tauri/gen/apple`). The UI follows LINE for iPhone and is designed for the iPhone Duo first:

- iOS draws the tab bar with UIKit (the `src-tauri/plugins/native-tabbar` plugin), so it gets Liquid Glass on iOS 26 and later. On screens where iOS 27.1 has a vertical bar (the iPhone Duo's cover screen, and the inner screen in landscape), the clock sits in a column beside the screen, so the plugin puts the tabs and the back, search, notes, ⋯ and new chat buttons there as glass groups, the way Apple's apps do, and the header keeps only the chat's name. Other screens get a `UITabBar` at the bottom that content scrolls under (`--tabbar-h`). The bar hides itself while a dialog or the viewer is open. Android and the phone preview on the desktop use an HTML tab bar.
- Cover screen (narrower than 600 pt): a tab bar at the bottom (friends, chats, add friends, settings). A chat slides in over the list; swipe from the left edge to go back.
- Inner screen (unfolded) and landscape: two panes, the list on the left and the chat on the right. Folding or unfolding keeps the same chat open.
- Long-press instead of right-click. A message's menu starts with a row of reactions. The composer has send, photo and file buttons (Return makes a new line). Photos, videos and PDFs open in the app.
- Albums and notes, and photos, files and links, open as pages in the app instead of separate windows.
- QR sign-in: on a phone that has LINE, **เปิดแอป LINE บนเครื่องนี้** (Open LINE on this device) opens the QR's link in LINE to confirm.

```bash
cd desktop
pnpm tauri ios dev "iPhone Duo"          # simulator (needs the iOS 27.1 runtime)
SIMCTL_CHILD_LAAB_DEMO=1 xcrun simctl launch --terminate-running-process booted app.laab.ios   # relaunch in demo mode
xcrun simctl openurl booted "laab://chat/c_team"   # open a chat by deep link (iOS asks first)
SIMCTL_CHILD_LAAB_DEMO=1 SIMCTL_CHILD_LAAB_DEMO_OPEN="laab://chat/c_team" \
  xcrun simctl launch --terminate-running-process booted app.laab.ios   # demo that opens a chat at launch, no tapping
```

To see the phone layout in the desktop app, set `localStorage.laabPhone = "1"` in the devtools and reload.

To build the app, run `./scripts/build-ios.sh` (simulator) or `APPLE_DEVELOPMENT_TEAM=<Team ID> ./scripts/build-ios.sh --device` (an `.ipa`).

Limits on iPhone:

- iOS suspends LAAB in the background, so notifications only arrive while the app is open (notifications while it's closed need APNs and a relay). The app syncs again when it comes back.
- LIFF opens in Safari, so there's no LINE user agent, `window._liff` or shareTargetPicker.
- Nothing moves over from the Mac; sign in again on the phone.
- The updater, usage statistics, crash reports and LIFF DevTools are for the Mac only.

## Features

- A resizable chat list (double-click the edge to reset it), LINE-style bubble tails, "อ่านแล้ว" (Read) above the time, Shift-Return for a new line
- Syncs only the chats that changed (`chats.synced`), and shows names of people who aren't friends (`users` and the `people` view)
- Deep links: `laab://chat/<id>?message=<id>`, used by the Raycast extension in `raycast/`
- LIFF links open the way they do in the LINE app (token in the fragment, LINE's user agent, `window._liff`, always asking first, shareTargetPicker)
- Flex messages as cards (bubble and carousel), with uri, message, postback and clipboard buttons
- In a group, click the reader avatars to see everyone who read a message
- Escape closes the newest thing first: a popover, then the mention picker, in-chat search, the sticker panel, and finally the chat
- In-chat search with ⌘F, ⌘G and ⇧⌘G, with highlighted results
- A keyboard-driven mention picker; mentions show in blue in the composer and in bubbles, and Backspace deletes a whole tag
- ⌥↑ and ⌥↓ switch chats, and so do ↑ and ↓ when the list has focus; a chat loads after you've rested on it for 150 ms
- Rows for unsent messages; silent unsends stay hidden, and the cached files of unsent messages are deleted
- Reactions: pick, change, remove, and see who reacted
- LINE-style call history rows and group event rows (`C_MI`)
- A developer tool that shows a message's raw data
- Drag one chat onto another to make a folder and name it, Discord-style; drag a chat out to take it out of the folder
- When the session expires or LINE signs you out, the whole window switches to the sign-in screen

From line-rs: replies, unsending, editing, translating, saving to Keep Memo, announcements (set and remove), hiding chats, deleting chat history, blocking, adding friends by LINE ID or link, joining groups by invite link, signing in with email or phone number, pins synced with LINE, group notes (read, post, delete) and albums (view, create, rename, delete). Right-click a message or a chat to find them. Editing, translating, server-side pins and album writes come from line-rs's reverse engineering and haven't been tried against the live server.

## Not yet tested with a real LINE account

Everything above has only been tested with unit tests and in demo mode. These still need a real account:

- QR login with `ANDROIDSECONDARY`
- Paginated `getMessageBoxes` (`minChatId`), and `getChats` 50 groups at a time
- Decrypting E2EE history, and downloading and sending photos and files (line-rs says its E2EE media metadata is still a guess)
- The parameters of reaction, unsend and read events (line-rs says they're unconfirmed)
- LIFF consent, shareTargetPicker and postbacks
- Muting and unmuting group notifications
