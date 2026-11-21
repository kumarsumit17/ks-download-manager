#!/bin/sh
# Builds the LGPL ffmpeg + ffprobe that the app bundles (specs/video-playlists.md,
# "Bundled programs"). macOS only for now; Windows is the Release stage.
#
#   scripts/build-ffmpeg.sh <work dir> <arm64|x86_64>
#
# The source tarball is pinned by version and SHA-256 in src-tauri/sidecars.json and
# checked before anything is unpacked. Output: <work dir>/out-<arch>/{ffmpeg,ffprobe}.
#
# Licence: LGPL-2.1-or-later. No --enable-gpl, no --enable-nonfree, no external
# libraries except system ones that are compatible (zlib, Secure Transport for TLS).
# The exact configure line below is the "source and build instructions" we ship.
set -eu

WORK=${1:?work dir}
ARCH=${2:?arm64 or x86_64}
HERE=$(cd "$(dirname "$0")/.." && pwd)
VERSION=$(node -e 'console.log(require(process.argv[1]).ffmpeg.version)' "$HERE/src-tauri/sidecars.json")
SHA=$(node -e 'console.log(require(process.argv[1]).ffmpeg.sha256)' "$HERE/src-tauri/sidecars.json")
TARBALL="$WORK/ffmpeg-$VERSION.tar.xz"

mkdir -p "$WORK"
if [ ! -f "$TARBALL" ]; then
  curl -fsSL -o "$TARBALL" "https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
fi
echo "$SHA  $TARBALL" | shasum -a 256 -c - >/dev/null || { echo "ffmpeg tarball hash mismatch" >&2; exit 1; }

SRC="$WORK/src-$ARCH"
rm -rf "$SRC" && mkdir -p "$SRC"
tar -xf "$TARBALL" -C "$SRC" --strip-components 1
cd "$SRC"

EXTRA=""
if [ "$ARCH" != "$(uname -m)" ]; then EXTRA="--enable-cross-compile"; fi
# No assembler is installed by default on macOS for x86 (nasm); C fallbacks are used.
if [ "$ARCH" = "x86_64" ]; then EXTRA="$EXTRA --disable-x86asm"; fi

./configure \
  --arch="$ARCH" --target-os=darwin --cc="clang -arch $ARCH" \
  --extra-cflags="-mmacosx-version-min=11.0" --extra-ldflags="-mmacosx-version-min=11.0" \
  --disable-autodetect --enable-zlib --enable-securetransport \
  --disable-gpl --disable-nonfree --disable-shared --enable-static \
  --disable-doc --disable-debug --disable-ffplay --enable-ffmpeg --enable-ffprobe \
  $EXTRA >/dev/null

make -j"$(sysctl -n hw.ncpu)" ffmpeg ffprobe >/dev/null
OUT="$WORK/out-$ARCH"
mkdir -p "$OUT"
cp ffmpeg ffprobe "$OUT/"
strip -x "$OUT/ffmpeg" "$OUT/ffprobe"
# Licence texts shipped with the binaries.
cp COPYING.LGPLv2.1 LICENSE.md "$OUT/"
if [ "$ARCH" = "$(uname -m)" ]; then "$OUT/ffmpeg" -hide_banner -version | head -1; fi
