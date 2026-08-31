# FPV Editor — Project Plan

A native video editor for FPV drone pilots, written in Rust: stabilization (gyro-based, like
Gyroflow), classic cutting/timeline editing, a modern UI, and end-to-end AI capability — both
remote-controllable by external agents and equipped with a built-in AI assistant that talks to a
configurable OpenAI-compatible endpoint.

## 1. Goals & Non-Goals

**Goals**
- Video stabilization tailored to FPV footage (gyro/blackbox data, lens correction, horizon lock)
- Full editing capabilities: multi-track timeline, trim/cut/ripple, transitions, speed ramping, color correction/LUTs
- FPV-specific extras: OSD/telemetry overlay, blackbox log import, camera/lens profiles
- AI accessible from two sides:
  1. **External**: agents (e.g. Claude Code, custom scripts) can drive the editor headlessly or alongside the GUI
  2. **Internal**: a chat/agent panel inside the app itself that calls the same editing functions as tools
- Configurable OpenAI-compatible endpoint (base URL + API key + model name) → works with OpenAI, Azure OpenAI, Ollama, LM Studio, vLLM, etc.
- Modern, native-feeling UI (no dated look)

**Non-Goals (v1)**
- No cloud rendering/collaboration backend
- No mobile app
- No custom codec — rely on ffmpeg/GPU pipelines instead of reinventing an encoder

## 2. Architecture Overview

Core idea: a **single command API** is the source of truth for all timeline mutations (add clip,
cut, stabilize, apply LUT, export, …). The GUI, the internal AI agent, and external agents all call
the same commands — this gives consistency, undo/redo, and scriptability for free.

```
                     ┌─────────────────────┐
                     │   Command / Event    │   (fpv-core)
                     │   Bus + Undo/Redo     │
                     └──────────▲───────────┘
           ┌─────────────┬──────┼──────┬─────────────────┐
           │             │      │      │                 │
     ┌─────┴────┐  ┌─────┴───┐ │ ┌────┴─────┐     ┌──────┴──────┐
     │ Tauri UI │  │ internal │ │ │ MCP server│     │  CLI (fpv)  │
     │(Frontend)│  │ AI agent │ │ │(external) │     │ (scripting) │
     └──────────┘  └─────────┘ │ └───────────┘     └─────────────┘
                                │
                    external agents (Claude Code, etc.)
```

### Crate Layout (Cargo Workspace)

| Crate | Purpose |
|---|---|
| `fpv-core` | Project/timeline data model, command bus, undo/redo, serialization (project file) |
| `fpv-media` | Decode/encode via ffmpeg, frame pipeline, proxy generation |
| `fpv-stabilize` | Gyro/blackbox-based stabilization, lens profiles, horizon lock, optical fallback |
| `fpv-gpu` | wgpu-based rendering: compositing, color grading/LUTs, warp for stabilization |
| `fpv-ai` | OpenAI-compatible client, agent loop with tool-calling against the command API |
| `fpv-mcp` | MCP server: exposes editor functions as tools for external agents |
| `fpv-cli` | Headless CLI for automation/scripting (uses the same core API) |
| `fpv-app` | Tauri shell, IPC commands, wires all crates together |
| `frontend/` | UI (Svelte/React + Tailwind), timeline canvas, preview player |

## 3. Tech Stack

- **Language/runtime**: Rust, `tokio` for async
- **UI shell**: Tauri 2 (Rust backend, webview frontend) — the fastest path to a genuinely modern
  look (Tailwind, shadcn-style components, clean animations), video preview via GPU
  texture/canvas streaming
  - *Alternative if a pure-Rust UI is preferred*: `iced` (wgpu-based, no webview) — less design
    flexibility, but a single Rust toolchain with no webview dependency
- **Video I/O**: `ffmpeg-next` (bindings) for decode/encode/muxing
- **GPU pipeline**: `wgpu` for color grading, LUT application, stabilization warp, compositing
- **Stabilization**: custom implementation following Gyroflow's approach (gyro integration,
  rolling-shutter correction, lens profile database) — possibly adopt concepts/formats from
  `gyroflow-core` (check license: Gyroflow is GPL-3.0 → reusing its code would make this project
  GPL-3.0-encumbered; alternatively reimplement independently from the algorithm/paper to stay
  license-free)
- **AI client**: `async-openai` (speaks the OpenAI API dialect, works against any OpenAI-compatible
  endpoint via a configurable `base_url`)
- **External agent interface**: MCP server (Rust SDK, e.g. `rmcp`) — directly compatible with
  Claude Code & friends, plus an optional plain REST interface for non-MCP clients
- **Project file**: JSON or RON, versionable, human-readable (for debugging/diffing)

## 4. AI Integration in Detail

### 4.1 Settings
- "AI Provider" settings panel: base URL, API key, model name, optional headers
- Presets for common local servers (Ollama, LM Studio) plus "Custom"
- Connection test button

### 4.2 Tool Definitions (shared between internal agent & MCP)
A central tool list, defined once and exposed twice (internally as a function-calling schema,
externally as MCP tools), e.g.:
- `list_clips`, `get_timeline_state`
- `trim_clip`, `split_clip`, `add_clip`, `reorder_clips`
- `apply_stabilization(clip_id, profile)`
- `apply_lut(clip_id, lut_path)`, `set_speed_ramp(clip_id, keyframes)`
- `add_text_overlay`, `add_osd_overlay(telemetry_source)`
- `render_export(preset)`

### 4.3 Internal Agent
- A chat panel in the app that "sees" the timeline (context = current project state)
- Uses the same tools as the MCP server → a prompt like "cut out all clips under 2 seconds and
  stabilize the rest" can be executed directly

### 4.4 External Control
- The MCP server optionally runs in the background of the app (or headless via `fpv-cli mcp-serve`)
- External agents can automate entire editing workflows without any GUI interaction

## 5. FPV-Specific Features

- **Blackbox/gyro import**: Betaflight/INAV logs, embedded gyro metadata (GoPro-style, if present), camera-latency sync
- **Lens/camera profiles**: database for common FPV cams (Caddx, RunCam, DJI O3/O4, etc.), FOV/distortion correction
- **Horizon lock & dynamic FOV** (like Gyroflow's "stabilization strength" + "smoothness")
- **OSD overlay**: rendering Betaflight OSD/WalkSnail/HDZero telemetry data over the footage
- **Music sync/speed ramping**: beat detection (optionally AI-assisted) for freestyle edits

## 6. Roadmap (Phases)

1. **Foundation**: workspace setup, tech spikes (ffmpeg decode + preview, wgpu render loop, Tauri shell)
2. **Core editing**: data model, timeline (cut/trim/reorder), preview playback
3. **Stabilization**: gyro import, stabilization algorithm, lens profiles
4. **Color & audio**: LUTs/color correction, audio track, OSD overlay
5. **AI internal**: provider configuration, tool layer, chat agent panel
6. **AI external**: MCP server, CLI automation
7. **Export & performance**: render pipeline, presets, proxy workflow for 4K/60 footage
8. **Polish**: UI/UX refinement, theming, onboarding, plugin/effect interface for later

## 7. Open Questions / Decisions

- Tauri vs. pure-Rust UI (iced) — recommendation: Tauri, for UI modernity
- Adopt Gyroflow code (GPL-3.0 encumbrance) vs. independent reimplementation (more effort, license-free)
- The app's own license (open source? if so, which one, compatible with the ffmpeg build variant GPL/LGPL)
- v1 target platforms: macOS + Windows first, Linux later?

## 8. Next Concrete Steps

1. Set up the Cargo workspace with the crates from section 2 (empty skeletons)
2. Tech spike: play video in a Tauri window via a wgpu texture
3. Build a minimal command bus in `fpv-core` (add/trim/remove clip) including undo/redo
4. Test a first `async-openai` connection against a configurable endpoint (simple chat, no tools yet)
