#!/bin/sh
# Installs the Windows installer silently, checks what it installed, uninstalls it and
# checks what it removed (docs/release-plan.md, section 4.2). For a CI runner or a test
# PC: it installs into the current user's account and pretends Chrome has run once.
#
#   sh scripts/test-windows-installer.sh <setup.exe>        (Git Bash on Windows)
set -eu
export MSYS_NO_PATHCONV=1 # keep /S and /ve as they are, not as paths (so Windows programs get cygpath -w paths)

SETUP=${1:?setup.exe}
KEY='HKCU\Software\Google\Chrome\NativeMessagingHosts\com.ks.downloadmanager'
LAD=$(cygpath -u "$LOCALAPPDATA")
DIR="$LAD/KS Download Manager"
fail() { echo "::error::$*"; exit 1; }

# The app registers with Chrome only once Chrome's own folder exists.
mkdir -p "$LAD/Google/Chrome"

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

# The installer's hook registered the Chrome host for this copy.
MANIFEST="$DIR/com.ks.downloadmanager.json"
[ -s "$MANIFEST" ] || fail "no host manifest next to the app"
node -e '
  const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  const want = process.argv[2];
  if (m.path.toLowerCase() !== want.toLowerCase()) throw new Error(`path ${m.path}, expected ${want}`);
  if (JSON.stringify(m.allowed_origins) !== JSON.stringify(["chrome-extension://ikplfdbdfikcokpegkapjdedlgifmkbe/"])) throw new Error("allowed_origins");
  if (m.name !== "com.ks.downloadmanager" || m.type !== "stdio") throw new Error("name/type");
' "$(cygpath -w "$MANIFEST")" "$(cygpath -w "$EXE")"
reg query "$KEY" /ve | tr -d '\r' | grep -iF "$(cygpath -w "$MANIFEST")" || fail "registry key does not name the manifest"

echo "uninstall: uninstall.exe /S"
"$DIR/uninstall.exe" /S
# The uninstaller copies itself to a temp folder and returns at once: wait for it.
for _ in $(seq 60); do [ -e "$EXE" ] || break; sleep 1; done
[ ! -e "$EXE" ] || fail "the app is still there after uninstalling"
[ ! -e "$MANIFEST" ] || fail "the host manifest is still there"
if reg query "$KEY" >/dev/null 2>&1; then fail "the registry key is still there"; fi
echo "installer test passed"
