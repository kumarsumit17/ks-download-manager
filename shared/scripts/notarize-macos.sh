#!/bin/sh
# Sends one file to Apple's notary service, waits, and staples the ticket to it.
#
#   scripts/notarize-macos.sh <file.app | file.dmg>
#
# Needs APPLE_ID, APPLE_PASSWORD (an app-specific password) and APPLE_TEAM_ID in the
# environment. An app is zipped with ditto for the upload; the ticket is stapled to the app.
# On rejection the notary log is printed and the script exits 1.
set -eu
FILE=${1:?file}
: "${APPLE_ID:?}" "${APPLE_PASSWORD:?}" "${APPLE_TEAM_ID:?}"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
UPLOAD=$FILE
case "$FILE" in
  *.app) UPLOAD="$WORK/upload.zip"; ditto -c -k --keepParent "$FILE" "$UPLOAD" ;;
esac
xcrun notarytool submit "$UPLOAD" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" --wait --timeout 60m --output-format json > "$WORK/result.json" || true
STATUS=$(plutil -extract status raw -o - "$WORK/result.json" 2>/dev/null || echo unknown)
ID=$(plutil -extract id raw -o - "$WORK/result.json" 2>/dev/null || echo "")
echo "notarisation of $(basename "$FILE"): $STATUS ($ID)"
if [ "$STATUS" != "Accepted" ]; then
  cat "$WORK/result.json" >&2 || true
  if [ -n "$ID" ]; then
    xcrun notarytool log "$ID" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" >&2 || true
  fi
  exit 1
fi
xcrun stapler staple "$FILE"
xcrun stapler validate "$FILE"
