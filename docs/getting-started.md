# Getting Started

FPV Editor is a native video editor for FPV drone pilots. This guide covers four ways
to get it running: the pre-built desktop app, the standalone CLI, the CLI Docker image,
and building from source. See [docs/README.md](README.md) for the rest of the
documentation set.

## Option 1: Install the desktop app

Download a pre-built installer from the
[latest GitHub release](https://github.com/mroxso/FPVEditor/releases/latest). You do not
need Git, Rust, Cargo, Node.js, or npm.

| Platform | Download | Install |
| --- | --- | --- |
| macOS on Apple Silicon | `FPV.Editor_*_aarch64.dmg` | Open the DMG, then drag **FPV Editor** to **Applications**. |
| Windows (64-bit) | `FPV.Editor_*_x64_en-US.msi` | Open the MSI file and follow the installer. |
| Ubuntu or Debian (64-bit) | `FPV.Editor_*_amd64.deb` | Run `sudo apt install ./FPV.Editor_*_amd64.deb` from the download directory. |
| Other Linux distributions (64-bit) | `FPV.Editor_*_amd64.AppImage` | Run `chmod +x FPV.Editor_*_amd64.AppImage`, then start it with `./FPV.Editor_*_amd64.AppImage`. |

macOS pre-built releases currently support Apple Silicon only. After installation, open
**FPV Editor** from Applications, the Windows Start menu, or your Linux application
launcher.

## Option 2: Install the CLI

The standalone CLI (`fpv`) is the easiest way to script the editor. It does not require
Git, Rust, or Cargo. The installer downloads the right binary for your computer, verifies
its SHA-256 checksum, and installs the `fpv` command.

On macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/mroxso/FPVEditor/main/scripts/install-fpv-cli.sh | bash
```

On Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/mroxso/FPVEditor/main/scripts/install-fpv-cli.ps1 | iex
```

The Windows installer adds `fpv` to your user `PATH`. On macOS and Linux, the installer
prints a command to add its installation directory to `PATH` when needed; open a new
terminal after doing so.

`fpv` uses the system-installed `ffmpeg` and `ffprobe` for media probing and export.
Install FFmpeg before working with video files:

```sh
# macOS (Homebrew)
brew install ffmpeg

# Ubuntu or Debian
sudo apt-get install ffmpeg
```

On Windows, run `winget install Gyan.FFmpeg` in PowerShell.

### Other installation options

You can also
[download a release archive](https://github.com/mroxso/FPVEditor/releases/latest)
manually. Download the `fpv-cli-*` archive matching your system and
`fpv-cli-checksums.txt`, verify the archive checksum, extract it, and place the `fpv`
executable on your `PATH`.

Run the installation script again to upgrade. To install a specific release or choose
another installation directory on macOS/Linux, set the `FPV_VERSION` or
`FPV_INSTALL_DIR` environment variables before running the script. To uninstall, remove
`fpv` from `~/.local/bin` on macOS/Linux or `%LOCALAPPDATA%\FPVEditor\bin` on Windows.

## Option 3: Run the CLI with Docker

A self-contained image with `fpv`, FFmpeg, and FFprobe already installed is published to
GHCR. No Rust, FFmpeg, or other local dependencies are needed.

```sh
docker pull ghcr.io/mroxso/fpveditor:latest
docker run --rm ghcr.io/mroxso/fpveditor:latest --help
```

The entrypoint runs `fpv` directly, and the image's working directory is `/work`. Mount a
local directory there to operate on your own project files:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$(pwd)":/work ghcr.io/mroxso/fpveditor:latest \
  new project.fpv.json --name "My FPV Edit"
docker run --rm --user "$(id -u):$(id -g)" -v "$(pwd)":/work ghcr.io/mroxso/fpveditor:latest \
  export project.fpv.json --clip <clip-id> --output out.mp4
```

Images are tagged per release (for example `0.10.1`), and `latest` tracks the most
recent release.

## Option 4: Build from source

Building from source is required if you want to develop FPV Editor, run the desktop app,
or make changes to the CLI. You need:

- Rust stable (edition 2021)
- Node.js and npm
- `ffmpeg` and `ffprobe` on your `PATH`
- For GPU-accelerated color/warp tests: a usable `wgpu` adapter (Metal, Vulkan, or DX12)

```sh
git clone https://github.com/mroxso/FPVEditor.git
cd FPVEditor
cd frontend && npm ci && cd ..
cargo run -p fpv-app
```

To run the CLI from the source checkout without installing it globally:

```sh
cargo run -p fpv-cli -- --help
cargo run -p fpv-cli -- new project.fpv.json --name "My Edit"
```

The frontend development server is available at `http://127.0.0.1:1420`:

```sh
cd frontend
npm run dev
```

See [Development](development.md) for the full set of build, test, and lint commands.

## Your first project

Whichever way you installed `fpv`, check that it's available and create an empty
project:

```sh
fpv --help
fpv new project.fpv.json --name "My FPV Edit"
```

Add a video track, then inspect the project file to find the generated track ID:

```sh
fpv add-track project.fpv.json --kind video --name V1
fpv show project.fpv.json
```

Copy the `id` field of the track from the `show` output, then add a clip to it, trim it,
stabilize it, and export it:

```sh
fpv add-clip project.fpv.json --track <track-id> \
  --source run.mp4 --in 0 --out 12

fpv trim-clip project.fpv.json --clip <clip-id> --in 1 --out 8

fpv stabilize project.fpv.json --clip <clip-id> \
  --smoothness 0.6 --strength 1.0 --horizon-lock --dynamic-fov 0.15

fpv export project.fpv.json --clip <clip-id> --output out.mp4
```

Each command loads the project file, applies one edit through the same command bus the
GUI and AI agent use, and saves the result back — see
[Architecture](architecture.md#the-command-bus) for why this matters. For every
subcommand and flag, see the [CLI Reference](cli-reference.md).

To let an external agent (such as Claude Code) drive this project directly:

```sh
fpv mcp-serve project.fpv.json
```

See [AI & MCP](ai-and-mcp.md) for how to connect an agent to this server.
