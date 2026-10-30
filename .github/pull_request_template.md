<!--
The title becomes a line in the release notes, so write it for someone who
uses LAAB: "Send an expired session to the login page", not "fix auth bug".
-->

## What changed

<!-- What does this change for someone using LAAB, and why? Link the issue if there is one. -->

## Screenshots

<!--
For UI changes, before and after. A short video helps for motion or drag and drop.
Use demo mode (LAAB_DEMO=1) or blur names, faces and messages. Don't post real chats.
-->

## How I tested it

<!-- What you clicked through, and on which macOS version. Delete the lines that don't apply. -->

- [ ] `pnpm test` in `desktop/`
- [ ] `cargo test` in `desktop/src-tauri/`
- [ ] Demo mode: `LAAB_DEMO=1 pnpm tauri dev`
- [ ] A real LINE account (say what you tried)
- [ ] A packaged build: `./scripts/release.sh <version> --dry-run`

## Checklist

- [ ] No tokens, keys, real chat data or personal LINE IDs in the code, tests, logs or screenshots
- [ ] Docs updated if behavior, setup or release steps changed (`README.md`, `desktop/README.md`, `AGENTS.md`)
