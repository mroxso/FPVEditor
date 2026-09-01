#!/usr/bin/env bash
# Build the LGPL-only FFmpeg command-line tools used by desktop release builds.
set -euo pipefail

readonly FFMPEG_VERSION="7.1.1"
readonly FFMPEG_SHA256="733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1"
readonly SOURCE_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
readonly BUILD_ROOT="${RUNNER_TEMP:-/tmp}/fpv-editor-ffmpeg"
readonly OUTPUT_DIR="crates/fpv-app/binaries"

rm -rf "$BUILD_ROOT"
mkdir -p "$BUILD_ROOT" "$OUTPUT_DIR"
curl --fail --location --retry 3 --output "$BUILD_ROOT/ffmpeg.tar.xz" "$SOURCE_URL"
if command -v sha256sum >/dev/null; then
  actual_sha256="$(sha256sum "$BUILD_ROOT/ffmpeg.tar.xz" | awk '{print $1}')"
else
  actual_sha256="$(shasum -a 256 "$BUILD_ROOT/ffmpeg.tar.xz" | awk '{print $1}')"
fi
[[ "$actual_sha256" == "$FFMPEG_SHA256" ]]
tar -C "$BUILD_ROOT" -xf "$BUILD_ROOT/ffmpeg.tar.xz"

pushd "$BUILD_ROOT/ffmpeg-${FFMPEG_VERSION}" >/dev/null
./configure \
  --disable-autodetect \
  --disable-gpl \
  --disable-nonfree \
  --disable-debug \
  --disable-doc \
  --enable-small \
  --prefix="$BUILD_ROOT/install"
make -j"${FFMPEG_JOBS:-2}"
make install
popd >/dev/null

extension=""
if [[ "${OSTYPE:-}" == msys* || "${OSTYPE:-}" == cygwin* ]]; then
  extension=".exe"
fi
install -m 755 "$BUILD_ROOT/install/bin/ffmpeg${extension}" "$OUTPUT_DIR/ffmpeg${extension}"
install -m 755 "$BUILD_ROOT/install/bin/ffprobe${extension}" "$OUTPUT_DIR/ffprobe${extension}"

cp "$BUILD_ROOT/ffmpeg-${FFMPEG_VERSION}/COPYING.LGPLv2.1" "$OUTPUT_DIR/FFMPEG-LGPL-2.1.txt"
printf '%s\n' "$FFMPEG_VERSION" > "$OUTPUT_DIR/FFMPEG-VERSION.txt"
printf '%s\n' \
  "FFmpeg source: $SOURCE_URL" \
  "SHA-256: $FFMPEG_SHA256" \
  "Configure: --disable-autodetect --disable-gpl --disable-nonfree --disable-debug --disable-doc --enable-small" \
  > "$OUTPUT_DIR/FFMPEG-BUILD.txt"
