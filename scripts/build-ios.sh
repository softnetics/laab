#!/usr/bin/env bash
# Build the Tauri app (desktop/) for iPhone.
#
#   ./scripts/build-ios.sh            dist/Laab-ios-simulator.app, for the iOS Simulator
#   ./scripts/build-ios.sh --device   dist/Laab.ipa, for a real iPhone
#
# VERSION sets the app version (default 0.1.0). LAAB_DEMO=1 builds the demo.
# --device needs APPLE_DEVELOPMENT_TEAM (your Team ID) for signing. Needs
# macOS, Xcode, Rust with the iOS targets, and pnpm.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$(pwd)
version=${VERSION:-0.1.0}
device=false
[[ ${1:-} == "--device" ]] && device=true

cd desktop
pnpm install --frozen-lockfile
mkdir -p "$root/dist"

if $device; then
  : "${APPLE_DEVELOPMENT_TEAM:?set APPLE_DEVELOPMENT_TEAM to your Apple Team ID}"
  pnpm tauri ios build --target aarch64 --export-method debugging --config "{\"version\":\"$version\"}"
  ipa=$(find src-tauri/gen/apple/build -name '*.ipa' -newer package.json | head -1)
  [[ -n $ipa ]] || { echo "No .ipa was built" >&2; exit 1; }
  cp "$ipa" "$root/dist/Laab.ipa"
  echo "Built $root/dist/Laab.ipa ($version)"
else
  pnpm tauri ios build --target aarch64-sim --debug --config "{\"version\":\"$version\"}"
  app=$(find src-tauri/gen/apple/build \( -path '*iphonesimulator*' -o -path '*-sim/*' \) -iname 'Laab.app' -type d | head -1)
  [[ -n $app ]] || { echo "No simulator app was built" >&2; exit 1; }
  rm -rf "$root/dist/Laab-ios-simulator.app"
  cp -R "$app" "$root/dist/Laab-ios-simulator.app"
  echo "Built $root/dist/Laab-ios-simulator.app ($version)"
  echo "Install: xcrun simctl install booted dist/Laab-ios-simulator.app"
fi
