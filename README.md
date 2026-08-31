# FPV Editor

A native video editor for FPV drone pilots, written in Rust. See [`PLAN.md`](PLAN.md)
for the full design; this file tracks what's actually implemented on this branch.

## Status

All timeline-mutation, stabilization, media, and AI-agent logic described in PLAN.md
is implemented as a Cargo workspace with real, passing tests (99 tests, no `#[ignore]`s;
a few skip themselves at runtime with a printed message if `ffmpeg`/a GPU adapter isn't
available in the environment, but both are exercised for real in this repo's dev setup).

| Crate | Status | What's real |
|---|---|---|
| `fpv-core` | ✅ | Project/timeline data model, `Command` enum, `CommandBus` with undo/redo, JSON project files |
| `fpv-media` | ✅ | `ffprobe`/`ffmpeg` CLI wrapper: probing, clip export (trim/LUT/crop/speed filters), proxy generation |
| `fpv-stabilize` | ✅ | Quaternion math, gyro-log integration, EMA smoothing, horizon lock, rolling-shutter correction, lens distortion model |
| `fpv-gpu` | ✅ | `.cube` LUT parsing + trilinear sampling, color grading math, stabilization reprojection math, a real `wgpu` compute pipeline (Metal-backed here) checked against the CPU reference |
| `fpv-ai` | ✅ | Configurable OpenAI-compatible client (`async-openai`), the shared tool catalog (PLAN.md §4.2), and the tool-calling agent loop — tested against a local mock HTTP server, no network/API key needed |
| `fpv-mcp` | ✅ | A real MCP server (`rmcp`) exposing the same tool catalog to external agents (e.g. Claude Code) over stdio; tested with a real MCP client round-tripping over an in-memory pipe |
| `fpv-cli` | ✅ | Headless `fpv` binary: `new/add-track/add-clip/trim-clip/split-clip/stabilize/apply-lut/list/show/probe/export/mcp-serve`, tested by driving the built binary as a subprocess |
| `fpv-app` | ⚠️ partial | The application service layer that wires every crate above together behind one `AppState` (what a GUI's IPC handlers would call) — implemented and tested. The actual Tauri window shell and `frontend/` UI (PLAN.md's "modern UI" and roadmap phase 8) are **not** built: this sandbox has no display/webview runtime to build or verify a GUI against, so wiring a real window here would be unverifiable busywork. `fpv-app`'s public API is the seam a Tauri shell wires into directly. |

Everything one layer below the GUI — the command bus, undo/redo, stabilization math,
media pipeline, LUT/color GPU path, and both the internal-agent and external-agent (MCP)
AI integrations — is implemented and tested, not stubbed.

## Building & testing

```sh
cargo test --workspace     # 99 tests
cargo clippy --workspace --all-targets
```

Requires `ffmpeg`/`ffprobe` on `PATH` for the full `fpv-media`/`fpv-cli` test coverage
(installed via `brew install ffmpeg` in this repo's dev environment); those tests print
a skip message and pass trivially if it's absent. The `fpv-gpu` GPU tests need a working
`wgpu` adapter (Metal/Vulkan/DX12); same skip-if-absent behavior.

## Using the CLI

```sh
cargo run -p fpv-cli -- new project.fpv.json --name "My Edit"
cargo run -p fpv-cli -- add-track project.fpv.json --kind video --name V1
cargo run -p fpv-cli -- add-clip project.fpv.json --track <track-id> \
    --source run.mp4 --in 0 --out 12
cargo run -p fpv-cli -- stabilize project.fpv.json --clip <clip-id> \
    --smoothness 0.6 --strength 1.0 --horizon-lock --dynamic-fov 0.15
cargo run -p fpv-cli -- export project.fpv.json --clip <clip-id> --output out.mp4

# Let an external agent (e.g. Claude Code) drive the same project over MCP:
cargo run -p fpv-cli -- mcp-serve project.fpv.json
```

## Open questions from PLAN.md §7

Still open, not decided by this implementation pass:

- **License**: `Cargo.toml` currently sets `MIT` as a placeholder so the workspace has a
  valid manifest field; PLAN.md explicitly leaves the project's actual license as an open
  decision (interacting with the GPL/LGPL choice of ffmpeg build). Revisit before any
  public release.
- **Tauri vs. `iced`**: not decided here either, since no UI shell was built this pass.
- **Adopting vs. reimplementing Gyroflow's algorithms**: `fpv-stabilize` here is an
  independent implementation (quaternion integration + EMA smoothing + horizon lock),
  not ported from Gyroflow, so this is moot for what exists — but the broader question
  (target parity with Gyroflow's specific algorithm) is still open.

## Architecture

Matches PLAN.md section 2: `fpv-core`'s `CommandBus` is the single mutation path.
`fpv-ai::tools::dispatch` and `fpv-mcp`'s `call_tool` both execute the *same*
`fpv_core::Command`s against it — the tool catalog (PLAN.md §4.2) is defined once in
`fpv-ai::tools::catalog()` and consumed both as OpenAI function-calling schemas (for the
internal agent) and as MCP `Tool` definitions (for external agents), so they can never
drift apart.
