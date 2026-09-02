<p align="center">
  <img src="frontend/src/assets/fpv-editor-logo.svg" width="112" alt="FPV Editor logo">
</p>

<h1 align="center">FPV Editor</h1>

<p align="center">A native, open-source video editor built for FPV drone pilots.</p>

<p align="center">
  <a href="#getting-started">Get started</a> ·
  <a href="#features">Features</a> ·
  <a href="docs/README.md">Docs</a> ·
  <a href="#contributing">Contributing</a> ·
  <a href="LICENSE">MIT License</a>
</p>

![FPV Editor empty project workspace](docs/images/editor-overview.png)

FPV Editor brings the FPV post-flight workflow into one native desktop application:
import your flight footage, build a multi-track cut, tune stabilization, apply LUTs, and
deliver an edit. The Rust core is shared by the desktop UI, CLI, AI copilot, and MCP
server, so every timeline change uses the same command bus with undo/redo support.

## Features

- Native Tauri desktop app with a focused flight-deck editing interface
- Multi-track timeline, trims, splits, project open/save, and undo/redo
- FFmpeg-powered probing, proxy creation, and clip export
- FPV stabilization tools: gyro integration, horizon lock, rolling-shutter correction,
  lens-distortion modelling, and dynamic-FOV crop calculation
- `.cube` LUT parsing and GPU/CPU color-processing paths
- Headless CLI for scripted workflows
- Configurable OpenAI-compatible AI copilot and an MCP server for external agents

## Getting started

### Install the desktop app

If you want the visual FPV Editor, download a pre-built installer from the
[latest GitHub release](https://github.com/mroxso/FPVEditor/releases/latest).
You do not need Git, Rust, Cargo, Node.js, or npm.

Choose the file that matches your computer:

| Platform | Download | Install |
| --- | --- | --- |
| macOS on Apple Silicon | `FPV.Editor_*_aarch64.dmg` | Open the DMG, then drag **FPV Editor** to **Applications**. |
| Windows (64-bit) | `FPV.Editor_*_x64_en-US.msi` | Open the MSI file and follow the installer. |
| Ubuntu or Debian (64-bit) | `FPV.Editor_*_amd64.deb` | Run `sudo apt install ./FPV.Editor_*_amd64.deb` from the download directory. |
| Other Linux distributions (64-bit) | `FPV.Editor_*_amd64.AppImage` | Run `chmod +x FPV.Editor_*_amd64.AppImage`, then start it with `./FPV.Editor_*_amd64.AppImage`. |

macOS pre-built releases currently support Apple Silicon only. After
installation, open **FPV Editor** from Applications, the Windows Start menu, or
your Linux application launcher.

### Install the CLI (recommended)

For most users, the standalone CLI is the easiest way to start. It does not
require Git, Rust, or Cargo. The installer downloads the right binary for your
computer, verifies its SHA-256 checksum, and installs the `fpv` command.

On macOS or Linux, open a terminal and run:

```sh
curl -fsSL https://raw.githubusercontent.com/mroxso/FPVEditor/main/scripts/install-fpv-cli.sh | bash
```

On Windows, open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/mroxso/FPVEditor/main/scripts/install-fpv-cli.ps1 | iex
```

The Windows installer adds `fpv` to your user `PATH`. On macOS and Linux, the
installer prints a command to add its installation directory to `PATH` when
needed; open a new terminal after doing so.

`fpv` uses the system-installed `ffmpeg` and `ffprobe` for media probing and
export. Install FFmpeg before working with video files:

```sh
# macOS (Homebrew)
brew install ffmpeg

# Ubuntu or Debian
sudo apt-get install ffmpeg
```

On Windows, run `winget install Gyan.FFmpeg` in PowerShell.

### Create your first project

Check that the CLI is available, then create an empty project:

```sh
fpv --help
fpv new project.fpv.json --name "My FPV Edit"
```

Add a video track and inspect the project file to find the generated track ID:

```sh
fpv add-track project.fpv.json --kind video --name V1
fpv show project.fpv.json
```

### Other CLI installation options

You can also [download a release archive](https://github.com/mroxso/FPVEditor/releases/latest)
manually. Download the `fpv-cli-*` archive matching your system and
`fpv-cli-checksums.txt`, verify the archive checksum, extract it, and place the
`fpv` executable on your `PATH`.

Run the installation script again to upgrade. To install a specific release or
choose another installation directory on macOS/Linux, set `FPV_VERSION` or
`FPV_INSTALL_DIR` before running it. To uninstall, remove `fpv` from
`~/.local/bin` on macOS/Linux or `%LOCALAPPDATA%\\FPVEditor\\bin` on Windows.

### Use the CLI

```sh
fpv add-clip project.fpv.json --track <track-id> \
  --source run.mp4 --in 0 --out 12
fpv stabilize project.fpv.json --clip <clip-id> \
  --smoothness 0.6 --strength 1.0 --horizon-lock --dynamic-fov 0.15
fpv export project.fpv.json --clip <clip-id> --output out.mp4
```

To expose a project to an external MCP-capable agent:

```sh
fpv mcp-serve project.fpv.json
```

### Install from source

Install from source if you want to develop FPV Editor, run the desktop app, or
make changes to the CLI. You need Rust stable (edition 2021), Node.js and npm,
and `ffmpeg` and `ffprobe` on your `PATH`. GPU tests additionally require a
usable `wgpu` adapter (Metal, Vulkan, or DX12).

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

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
cd frontend && npm run build
```

Media and GPU tests skip gracefully when their required tools or hardware are unavailable.

## Project status

The core editing model, command bus, media wrapper, GPU color path, CLI, AI/MCP
integration, and desktop shell are implemented and covered by workspace tests. The
stabilization math is implemented, but frame-by-frame stabilization rendering is not yet
wired into clip export; current exports reserve the dynamic-FOV crop without applying
the final de-shake warp. See [PLAN.md](PLAN.md) for the architecture and remaining work.

## Architecture

The workspace is organized around a shared Rust core:

| Crate | Responsibility |
| --- | --- |
| `fpv-core` | Project model, commands, undo/redo, JSON project files |
| `fpv-media` | FFmpeg/FFprobe integration, proxies, and exports |
| `fpv-stabilize` / `fpv-gpu` | Stabilization and image-processing primitives |
| `fpv-ai` / `fpv-mcp` | AI tool catalog and MCP server |
| `fpv-cli` / `fpv-app` | Headless and desktop entry points |

See the [docs](docs/README.md) for the full documentation set, including a CLI reference,
stabilization details, AI/MCP integration, and the frontend architecture.

## Contributing

Contributions are welcome. Please open an issue to discuss significant changes, then
send a focused pull request with tests and a clear description of the behavior change.
Use conventional commit messages and run the checks in [Development](#development)
before submitting. For visible UI changes, include a screenshot in the pull request.

Do not commit API keys, provider credentials, or custom authorization headers.

## License

FPV Editor is released under the [MIT License](LICENSE).
