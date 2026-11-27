#!/bin/sh
# Installs the Windows installer silently, checks what it installed, uninstalls it and
# checks what it removed (docs/release-plan.md, section 4.2). For a CI runner or a test
# PC: it installs into the current user's account and pretends Chrome and Edge have run once.
#
#   sh scripts/test-windows-installer.sh <setup.exe>        (Git Bash on Windows)
set -eu
export MSYS_NO_PATHCONV=1 # keep /S and /ve as they are, not as paths (so Windows programs get cygpath -w paths)

SETUP=${1:?setup.exe}
# KSDM_DEV=1: the DEV build (tauri.dev.conf.json), with its own identity everywhere.
if [ -n "${KSDM_DEV:-}" ]; then
  ID=com.ks.downloadmanager.dev NAME="KS Download Manager DEV" EXT=mhgmalnfheamcgcongjgdmepagonmmbc
else
  ID=com.ks.downloadmanager NAME="KS Download Manager" EXT=ikplfdbdfikcokpegkapjdedlgifmkbe
fi
KEY="HKCU\Software\Google\Chrome\NativeMessagingHosts\$ID"
EDGE_KEY="HKCU\Software\Microsoft\Edge\NativeMessagingHosts\$ID"
LAD=$(cygpath -u "$LOCALAPPDATA")
DIR="$LAD/$NAME"
fail() { echo "::error::$*"; exit 1; }

# The app registers with a browser only once that browser's own folder exists.
mkdir -p "$LAD/Google/Chrome" "$LAD/Microsoft/Edge"

echo "install: $SETUP /S"
"$SETUP" /S
[ -d "$DIR" ] || fail "nothing installed in $DIR"
ls "$DIR"
EXE=$(find "$DIR" -maxdepth 1 -iname '*.exe' ! -iname 'uninstall.exe' ! -iname 'ffmpeg.exe' ! -iname 'ffprobe.exe' ! -iname 'deno.exe' | head -1)
[ -n "$EXE" ] || fail "no app program in $DIR"

# Licences and bundled programs.
for f in yt-dlp-LICENSE.txt ffmpeg-COPYING.LGPLv2.1 ffmpeg-LICENSE.md ffmpeg-SOURCE.txt deno-LICENSE.md deno-THIRD-PARTY.txt zlib-LICENSE.txt mingw-w64-runtime-COPYING.txt; do
  [ -s "$DIR/licenses/$f" ] || fail "missing licence file: $f"
done
[ -s "$DIR/yt-dlp/_internal/THIRD_PARTY_LICENSES.txt" ] || fail "missing yt-dlp THIRD_PARTY_LICENSES.txt"
"$DIR/ffmpeg.exe" -hide_banner -version | head -1
"$DIR/ffprobe.exe" -hide_banner -version | head -1
"$DIR/deno.exe" --version | head -1
"$DIR/yt-dlp/yt-dlp.exe" --version

# The installer's hook registered the host with Chrome and Edge for this copy.
MANIFEST="$DIR/$ID.json"
[ -s "$MANIFEST" ] || fail "no host manifest next to the app"
node -e '
  const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  const [want, id, ext] = process.argv.slice(2);
  if (m.path.toLowerCase() !== want.toLowerCase()) throw new Error(`path ${m.path}, expected ${want}`);
  if (JSON.stringify(m.allowed_origins) !== JSON.stringify([`chrome-extension://${ext}/`])) throw new Error("allowed_origins");
  if (m.name !== id || m.type !== "stdio") throw new Error("name/type");
' "$(cygpath -w "$MANIFEST")" "$(cygpath -w "$EXE")" "$ID" "$EXT"
reg query "$KEY" /ve | tr -d '\r' | grep -iF "$(cygpath -w "$MANIFEST")" || fail "registry key does not name the manifest"
reg query "$EDGE_KEY" /ve | tr -d '\r' | grep -iF "$(cygpath -w "$MANIFEST")" || fail "Edge's registry key does not name the manifest"

echo "uninstall: uninstall.exe /S"
"$DIR/uninstall.exe" /S
# The uninstaller copies itself to a temp folder and returns at once: wait for it.
for _ in $(seq 60); do [ -e "$EXE" ] || break; sleep 1; done
[ ! -e "$EXE" ] || fail "the app is still there after uninstalling"
[ ! -e "$MANIFEST" ] || fail "the host manifest is still there"
if reg query "$KEY" >/dev/null 2>&1; then fail "the registry key is still there"; fi
if reg query "$EDGE_KEY" >/dev/null 2>&1; then fail "Edge's registry key is still there"; fi
echo "installer test passed"
