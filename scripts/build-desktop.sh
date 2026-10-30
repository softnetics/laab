#!/usr/bin/env bash
# Build the Tauri app (desktop/) into dist/LAAB.app.
# VERSION sets the app version (default 0.1.0). Needs macOS, Rust and pnpm.
#
# Signing: APPLE_SIGNING_IDENTITY if set, else the first "Developer ID
# Application" certificate in the keychain (hardened runtime, timestamped),
# else ad hoc. release.sh notarizes Developer ID builds.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$(pwd)
version=${VERSION:-0.1.0}
identity=${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning |
  sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)}
identity=${identity:--}

cd desktop
pnpm install --frozen-lockfile
pnpm tauri build --bundles app \
  --config "{\"version\":\"$version\",\"bundle\":{\"macOS\":{\"signingIdentity\":\"$identity\"}}}"

bundle="src-tauri/target/release/bundle/macos/LAAB.app"
app="$root/dist/LAAB.app"
mkdir -p "$root/dist"
# Replace instead of overwrite: macOS caches code signatures per inode.
rm -rf "$app"
ditto "$bundle" "$app"
if [[ "$identity" == - ]]; then
  # Ad hoc signature, or macOS drops the app's notifications.
  codesign --force --deep --sign - "$app"
  echo "Built $app ($version, ad hoc signature)"
else
  codesign --verify --strict --deep "$app"
  echo "Built $app ($version, signed by $identity)"
fi
