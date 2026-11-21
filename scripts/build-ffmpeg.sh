#!/bin/sh
# Builds the LGPL ffmpeg + ffprobe that the app bundles (specs/video-playlists.md,
# "Bundled programs").
#
#   scripts/build-ffmpeg.sh <work dir> <arm64|x86_64|windows-x86_64>
#
# arm64 and x86_64 build for macOS, on a Mac. windows-x86_64 cross-builds for Windows on
# Linux with the mingw-w64 toolchain and nasm (Ubuntu: apt install mingw-w64 nasm).
#
# The source tarballs (ffmpeg, and zlib for Windows) are pinned by version and SHA-256 in
# src-tauri/sidecars.json and checked before anything is unpacked.
# Output: <work dir>/out-<target>/{ffmpeg,ffprobe}[.exe] plus the licence texts.
#
# Licence: LGPL-2.1-or-later. No --enable-gpl, no --enable-nonfree, no external
# libraries except compatible ones: zlib (the system copy on macOS, built from the pinned
# source on Windows), and the system TLS (Secure Transport on macOS, Schannel on Windows).
# The exact configure lines below are the "source and build instructions" we ship.
set -eu

WORK=${1:?work dir}
TARGET=${2:?arm64, x86_64 or windows-x86_64}
HERE=$(cd "$(dirname "$0")/.." && pwd)
PINS="$HERE/src-tauri/sidecars.json"
pin() { node -e 'const p=require(process.argv[1]); console.log(p[process.argv[2]][process.argv[3]])' "$PINS" "$1" "$2"; }
jobs() { nproc 2>/dev/null || sysctl -n hw.ncpu; }

# fetch <url> <file> <sha256>: downloads once, then checks the hash every time.
fetch() {
  [ -f "$2" ] || curl -fsSL -o "$2" "$1"
  echo "$3  $2" | shasum -a 256 -c - >/dev/null || { echo "$2: hash mismatch" >&2; rm -f "$2"; exit 1; }
}

VERSION=$(pin ffmpeg version)
mkdir -p "$WORK"
TARBALL="$WORK/ffmpeg-$VERSION.tar.xz"
fetch "$(pin ffmpeg source)" "$TARBALL" "$(pin ffmpeg sha256)"

SRC="$WORK/src-$TARGET"
OUT="$WORK/out-$TARGET"
rm -rf "$SRC" "$OUT" && mkdir -p "$SRC" "$OUT"
tar -xf "$TARBALL" -C "$SRC" --strip-components 1

COMMON="--disable-autodetect --enable-zlib \
  --disable-gpl --disable-nonfree --disable-shared --enable-static \
  --disable-doc --disable-debug --disable-ffplay --enable-ffmpeg --enable-ffprobe"

case "$TARGET" in
  arm64 | x86_64)
    ARCH=$TARGET
    EXTRA=""
    if [ "$ARCH" != "$(uname -m)" ]; then EXTRA="--enable-cross-compile"; fi
    # No assembler is installed by default on macOS for x86 (nasm); C fallbacks are used.
    if [ "$ARCH" = "x86_64" ]; then EXTRA="$EXTRA --disable-x86asm"; fi
    cd "$SRC"
    # shellcheck disable=SC2086
    ./configure \
      --arch="$ARCH" --target-os=darwin --cc="clang -arch $ARCH" \
      --extra-cflags="-mmacosx-version-min=11.0" --extra-ldflags="-mmacosx-version-min=11.0" \
      --enable-securetransport $COMMON $EXTRA >/dev/null
    EXE=""
    STRIP="strip -x"
    ;;
  windows-x86_64)
    # The "-win32" compiler uses Windows' own threads, so nothing links winpthreads.
    CROSS=x86_64-w64-mingw32-
    CC=${CROSS}gcc-win32
    command -v "$CC" >/dev/null || { echo "$CC not found (apt install mingw-w64)" >&2; exit 1; }
    command -v nasm >/dev/null || { echo "nasm not found (apt install nasm)" >&2; exit 1; }
    ZV=$(pin zlib version)
    ZTAR="$WORK/zlib-$ZV.tar.xz"
    fetch "$(pin zlib source)" "$ZTAR" "$(pin zlib sha256)"
    ZDIR="$WORK/zlib-$TARGET"
    rm -rf "$ZDIR" && mkdir -p "$ZDIR"
    tar -xf "$ZTAR" -C "$ZDIR" --strip-components 1
    make -C "$ZDIR" -f win32/Makefile.gcc PREFIX="$CROSS" CC="$CC" libz.a >/dev/null
    cd "$SRC"
    # shellcheck disable=SC2086
    ./configure \
      --arch=x86_64 --target-os=mingw32 --enable-cross-compile \
      --cross-prefix="$CROSS" --cc="$CC" \
      --extra-cflags="-I$ZDIR" --extra-ldflags="-L$ZDIR -static -static-libgcc" \
      --enable-schannel --enable-w32threads $COMMON >/dev/null
    EXE=".exe"
    STRIP="${CROSS}strip"
    ;;
  *)
    echo "unknown target: $TARGET" >&2
    exit 1
    ;;
esac

# The licence policy, checked on the configured build (the binaries may not run here).
for flag in GPL NONFREE; do
  grep -q "^#define CONFIG_$flag 0$" config.h || { echo "CONFIG_$flag is not 0" >&2; exit 1; }
done

make -j"$(jobs)" ffmpeg ffprobe >/dev/null
cp "ffmpeg$EXE" "ffprobe$EXE" "$OUT/"
$STRIP "$OUT/ffmpeg$EXE" "$OUT/ffprobe$EXE"
# Licence texts shipped with the binaries.
cp COPYING.LGPLv2.1 LICENSE.md "$OUT/"

if [ "$TARGET" = windows-x86_64 ]; then
  cp "$ZDIR/LICENSE" "$OUT/zlib-LICENSE.txt"
  # The mingw-w64 start-up code and C runtime glue are linked into the programs.
  cp /usr/share/doc/mingw-w64-x86-64-dev/copyright "$OUT/mingw-w64-runtime-COPYING.txt"
  # Only Windows' own DLLs may be needed: nothing from the toolchain (libgcc, winpthread).
  DLLS=$(${CROSS}objdump -p "$OUT/ffmpeg.exe" "$OUT/ffprobe.exe" | sed -n 's/^\s*DLL Name: //p' | tr 'A-Z' 'a-z' | sort -u)
  echo "$DLLS" | sed 's/^/imports: /'
  if echo "$DLLS" | grep -Eq 'libgcc|libwinpthread|libstdc|libz|zlib'; then
    echo "a toolchain DLL is needed at run time" >&2
    exit 1
  fi
  {
    echo "Windows build: cross-compiled on $(. /etc/os-release && echo "$PRETTY_NAME") with"
    echo "$($CC --version | head -1), $(nasm -v), zlib $ZV ($(pin zlib source), SHA-256 $(pin zlib sha256))."
    echo "Configure line: see the windows-x86_64 case in scripts/build-ffmpeg.sh."
  } > "$OUT/BUILD.txt"
elif [ "$TARGET" = "$(uname -m)" ]; then
  "$OUT/ffmpeg" -hide_banner -version | head -1
fi
