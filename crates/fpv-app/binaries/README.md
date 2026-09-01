# Bundled FFmpeg binaries

Release builds place `ffmpeg` and `ffprobe` (with `.exe` on Windows) in this
directory before Tauri packages the application. The files are intentionally
not committed: they are built from the pinned upstream source by
[`scripts/build-ffmpeg.sh`](../../../scripts/build-ffmpeg.sh).

The Tauri bundle includes this directory as resources. At application startup
`fpv-app` configures `fpv-media` to use these executables before checking the
user override or `PATH`.
