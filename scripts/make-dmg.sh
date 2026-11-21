#!/bin/sh
# Makes the macOS disk image from an app that is already signed (and, with a Developer ID,
# notarised and stapled). Tauri's own DMG is made before scripts/sign-macos.sh re-signs the
# app, so it would hold the app as it was before; this one is made after. System tools only.
#
#   scripts/make-dmg.sh "<path>/KS Download Manager.app" <output.dmg>
set -eu
APP=${1:?app bundle}
DMG=${2:?output dmg}
NAME=$(basename "$APP" .app)
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
# ditto keeps signatures, extended attributes and symbolic links exactly as they are.
ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "$NAME" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG"
hdiutil verify -quiet "$DMG"
echo "dmg: $DMG"
