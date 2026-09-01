# FFmpeg distribution notice

FPV Editor release bundles include the `ffmpeg` and `ffprobe` command-line
programs built from **FFmpeg 7.1.1**. They are built with
`--disable-autodetect --disable-gpl --disable-nonfree`; no GPL or
non-redistributable external libraries are enabled.

FFmpeg is licensed under the GNU Lesser General Public License, version 2.1 or
later. FPV Editor is not affiliated with the FFmpeg project.

For every desktop release, the corresponding unmodified FFmpeg source archive
(`ffmpeg-7.1.1.tar.xz`), its SHA-256 digest, and the exact configure command
are published as release assets. The source is also available from the official
[FFmpeg release archive](https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz).

The build recipe is [`scripts/build-ffmpeg.sh`](../../scripts/build-ffmpeg.sh).
This notice and the LGPL text are included in release artifacts.
