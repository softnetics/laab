#!/usr/bin/env bash
# Build dist/LAAB.app and package it as dist/LAAB-<version>-macos-<arch>.dmg
# (or .zip) with a .sha256 next to it. Used by release.sh --dry-run and by
# the Release workflow.
#
#   ./scripts/package.sh 0.2.0          # .dmg
#   ./scripts/package.sh 0.2.0 --zip    # .zip
#
# With a "Developer ID Application" certificate in the keychain (or
# APPLE_SIGNING_IDENTITY), the app and the .dmg are signed, notarized and
# stapled. Notarization uses the notarytool keychain profile
# LAAB_NOTARY_PROFILE (default laab-notary); create it once with
#   xcrun notarytool store-credentials laab-notary \
#     --key AuthKey_<id>.p8 --key-id <id> --issuer <issuer id>
# Without a certificate the build is ad hoc signed and not notarized.
#
# With TAURI_SIGNING_PRIVATE_KEY (or _PATH) and _PASSWORD, it also writes the
# in-app updater's files: LAAB-<version>-macos-<arch>.app.tar.gz, its .sig and
# latest.json, which points at the release download URL
# (LAAB_UPDATE_BASE_URL, default the GitHub release for v<version>).
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { echo "usage: $0 <version> [--zip]" >&2; exit 2; }
die() { echo "package: $*" >&2; exit 1; }

version=""
format=dmg
for arg in "$@"; do
  case "$arg" in
    --zip) format=zip ;;
    -*) usage ;;
    *) [[ -z "$version" ]] || usage; version="${arg#v}" ;;
  esac
done
[[ -n "$version" ]] || usage

[[ "$(uname)" == Darwin ]] || die "must run on macOS"

identity=${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning |
  sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)}
notary_profile=${LAAB_NOTARY_PROFILE:-laab-notary}
if [[ -n "$identity" ]]; then
  xcrun notarytool history --keychain-profile "$notary_profile" >/dev/null 2>&1 ||
    die "notarytool profile $notary_profile is missing or invalid (see the top of this script)"
fi

# Submit to Apple's notary service and wait; print Apple's log on failure.
notarize() {
  local file=$1 out status id
  out=$(mktemp)
  xcrun notarytool submit "$file" --keychain-profile "$notary_profile" --wait --output-format json >"$out" || true
  status=$(plutil -extract status raw -o - "$out" 2>/dev/null || true)
  id=$(plutil -extract id raw -o - "$out" 2>/dev/null || true)
  rm -f "$out"
  if [[ "$status" != Accepted ]]; then
    [[ -n "$id" ]] && xcrun notarytool log "$id" --keychain-profile "$notary_profile" >&2
    die "notarizing $(basename "$file") failed (${status:-no status})"
  fi
  echo "Notarized $(basename "$file")"
}

# Apple can take a minute to publish a fresh ticket; stapler fails with
# error 68 until it does.
staple() {
  local attempt
  for attempt in 1 2 3 4 5 6; do
    xcrun stapler staple -q "$1" && return
    echo "Stapling $(basename "$1") failed (attempt $attempt); retrying in 20 s" >&2
    sleep 20
  done
  die "could not staple $(basename "$1")"
}

APPLE_SIGNING_IDENTITY="${identity:--}" VERSION="$version" ./scripts/build-desktop.sh

if [[ -n "$identity" ]]; then
  submission="$(mktemp -d)"
  ditto -c -k --keepParent dist/LAAB.app "$submission/LAAB.zip"
  notarize "$submission/LAAB.zip"
  rm -rf "$submission"
  staple dist/LAAB.app
fi

arch="$(uname -m)"

if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
  update="dist/LAAB-$version-macos-$arch.app.tar.gz"
  rm -f "$update" "$update.sig" dist/latest.json
  # AppleDouble ._ files in the archive would break the bundle's signature.
  COPYFILE_DISABLE=1 tar -czf "$update" -C dist LAAB.app
  (cd desktop && pnpm --silent tauri signer sign --app-version "$version" "../$update" >/dev/null)
  case "$arch" in arm64) platform=darwin-aarch64 ;; x86_64) platform=darwin-x86_64 ;; *) die "no updater platform for $arch" ;; esac
  base_url=${LAAB_UPDATE_BASE_URL:-https://github.com/${GITHUB_REPOSITORY:-softnetics/laab}/releases/download/v$version}
  cat > dist/latest.json <<JSON
{
  "version": "$version",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "$platform": {
      "signature": "$(cat "$update.sig")",
      "url": "$base_url/$(basename "$update")"
    }
  }
}
JSON
  echo "Wrote updater files for $version"
fi

pkg="dist/LAAB-$version-macos-$arch.$format"
rm -f "$pkg" "$pkg.sha256"
if [[ "$format" == dmg ]]; then
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  ditto dist/LAAB.app "$staging/LAAB.app"
  # Drag-to-install target in the mounted window.
  ln -s /Applications "$staging/Applications"
  hdiutil create -quiet -volname "LAAB $version" -srcfolder "$staging" -fs HFS+ -format UDZO "$pkg"
  if [[ -n "$identity" ]]; then
    codesign --sign "$identity" --timestamp "$pkg"
    notarize "$pkg"
    staple "$pkg"
  fi
else
  # ditto keeps the bundle's symlinks, permissions and signature intact; zip(1) does not.
  ditto -c -k --sequesterRsrc --keepParent dist/LAAB.app "$pkg"
fi
(cd dist && shasum -a 256 "$(basename "$pkg")" > "$(basename "$pkg").sha256")
echo "Packaged $pkg"
