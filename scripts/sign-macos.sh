#!/bin/sh
# Signs the bundled programs inside a built app with the hardened runtime, then seals
# the app again. Tauri signs `externalBin` programs without entitlements and leaves the
# resources (yt-dlp's folder) as they came, so this runs after `tauri build`.
#
#   scripts/sign-macos.sh "<path>/KS Download Manager.app" [identity]
#
# identity: "-" (ad-hoc, the default) or a "Developer ID Application: …" certificate
# name from the keychain. With a real identity a secure timestamp is added, which
# notarisation needs.
set -eu
APP=${1:?app bundle}
ID=${2:--}
HERE=$(cd "$(dirname "$0")/.." && pwd)
TS=""
if [ "$ID" != "-" ]; then TS="--timestamp"; fi
sign() { codesign --force --options runtime $TS --sign "$ID" "$@"; }

YT="$APP/Contents/Resources/yt-dlp"
# Every Mach-O file in it (Python, its extension modules and libraries), each as a plain
# file: PyInstaller's Python.framework is not a well-formed bundle, so it is not signed as
# one. Then the yt-dlp program itself.
LIST=$(mktemp)
find "$YT" -type f ! -name yt-dlp_macos -print > "$LIST"
UNSIGNED=0
while IFS= read -r f; do
  if file -b "$f" | grep -q "^Mach-O"; then
    if ! sign "$f" 2>/dev/null; then
      UNSIGNED=$((UNSIGNED + 1))
      echo "not signed (malformed framework copy, see docs/decisions-stage5.md): $f" >&2
    fi
  fi
done < "$LIST"
rm -f "$LIST"
if [ "$ID" = "-" ]; then
  sign --entitlements "$HERE/src-tauri/yt-dlp-adhoc.entitlements.plist" "$YT/yt-dlp_macos"
else
  sign "$YT/yt-dlp_macos"
fi

# Deno needs JIT; nothing else does.
sign --entitlements "$HERE/src-tauri/deno.entitlements.plist" "$APP/Contents/MacOS/deno"
sign "$APP/Contents/MacOS/ffmpeg" "$APP/Contents/MacOS/ffprobe"

# Seal the app again over the re-signed contents.
sign --entitlements "$HERE/src-tauri/entitlements.plist" "$APP"
codesign --verify --deep --strict "$APP"
echo "signed: $APP ($ID); files that could not be signed: $UNSIGNED"
