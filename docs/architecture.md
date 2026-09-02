# Architecture

FPV Editor is a Rust 2021 Cargo workspace built around one idea: every timeline mutation
flows through a single command API, so the desktop UI, the internal AI agent, an external
MCP-connected agent, and the CLI all get undo/redo and consistency for free.

```
                     +----------------------+
                     |   Command / Event    |   (fpv-core)
                     |   Bus + Undo/Redo    |
                     +-----------^----------+
           +-------------+-------+------+-------------------+
           |             |              |                   |
     +-----+----+  +-----+----+   +-----+-----+     +-------+-----+
     | Tauri UI |  | internal |   | MCP server|     |  CLI (fpv)  |
     |(frontend)|  | AI agent |   |(external) |     | (scripting) |
     +----------+  +----------+   +-----------+     +-------------+
                         |              |
                          external agents (Claude Code, etc.)
```

## Crate layout

| Crate | Responsibility |
| --- | --- |
| `fpv-core` | Project/timeline data model, the `Command` enum, `CommandBus` (undo/redo), JSON project-file serialization |
| `fpv-media` | `ffmpeg`/`ffprobe` integration: probing, proxy generation, clip and timeline export |
| `fpv-stabilize` | Gyro/blackbox-based stabilization math: orientation integration, smoothing, horizon lock, rolling-shutter correction, lens distortion |
| `fpv-gpu` | `wgpu`-based color grading/LUT application and stabilization warp/reprojection math |
| `fpv-ai` | OpenAI-compatible client, the shared tool catalog, and the internal agent's tool-calling loop |
| `fpv-mcp` | MCP server that exposes the same tool catalog to external agents |
| `fpv-cli` | Headless `fpv` binary for scripting (`clap`-based) |
| `fpv-app` | The Tauri application service layer (`AppState`) that wires every crate together, plus the Tauri IPC command handlers in `main.rs` |
| `frontend/` | The React/TypeScript UI (Tauri webview) |

Dependency direction is one-way: `fpv-cli`, `fpv-mcp`, `fpv-ai`, and `fpv-app` all depend
on `fpv-core`, `fpv-media`, and (where relevant) `fpv-stabilize`/`fpv-gpu` — never the
reverse. `fpv-mcp` reuses `fpv-ai`'s tool catalog and dispatch logic directly rather than
redefining it, so the MCP tool list and the internal agent's tool list cannot drift apart.

## The command bus

`fpv_core::CommandBus` (`crates/fpv-core/src/bus.rs`) is the single mutation path for a
`Project`. It holds:

- the current `Project`
- an undo stack of `(Command, pre-execution Project snapshot)` pairs
- a redo stack of the same shape

`execute(command)` clones the project, applies the command, and — only on success —
pushes the command and the pre-state onto the undo stack and clears the redo stack
(standard editor semantics: a new edit invalidates future redo history). A failed command
leaves the project completely untouched. `undo()`/`redo()` swap the whole project back
and forth between the two stacks; `history()` returns human-readable descriptions of the
executed commands (`Command::describe`), suitable for an undo-history panel.

Every one of the GUI, the internal AI agent, the MCP server, and the CLI constructs a
`CommandBus` and calls `execute` with a `Command` — none of them contain separate
mutation logic. `fpv-app`'s `AppState` holds one `CommandBus` shared between the GUI and
the internal AI agent (so a chat-driven edit is immediately visible in the timeline);
`fpv mcp-serve` and the GUI each hold their own separate `CommandBus` loaded from the
same project file, so running both against one file concurrently can overwrite one side's
edits on save.

### The `Command` enum

`fpv_core::Command` (`crates/fpv-core/src/command.rs`) is a serde-tagged enum
(`#[serde(tag = "command", rename_all = "snake_case")]`) with one variant per timeline
mutation:

| Variant | Effect |
| --- | --- |
| `AddTrack { kind, name }` | Append a video or audio track |
| `RemoveTrack { track_id }` | Remove a track and all of its clips |
| `AddClip { track_id, clip }` | Insert a new clip into a track, sorted by timeline position |
| `RemoveClip { clip_id }` | Remove a clip from the project |
| `TrimClip { clip_id, new_in, new_out }` | Change a clip's in/out points within its source media |
| `TrimClipStart { clip_id, new_in, new_position }` | Trim the clip's start while keeping its visible left edge in sync with a new timeline position (one undoable action) |
| `SplitClip { clip_id, at }` | Split a clip into two at an absolute timeline position |
| `ReorderClip { track_id, clip_id, new_index }` | Move a clip to a new index within its track's playback order |
| `MoveClip { clip_id, new_track_id, new_position }` | Move a clip to a different track and/or position |
| `ApplyStabilization { clip_id, profile }` | Attach a `StabilizationProfile` to a clip |
| `ApplyLut { clip_id, lut_path }` | Attach a `.cube` LUT path to a clip |
| `SetSpeedRamp { clip_id, keyframes }` | Set speed-ramp keyframes (rejected if empty, out of order, or a non-positive rate) |
| `AddTextOverlay { clip_id, overlay }` | Add a positioned, timed text overlay |
| `AddOsdOverlay { clip_id, source }` | Attach a flight-controller OSD telemetry source (`Betaflight`, `Inav`, `WalkSnail`, `Hdzero`) |

Because `Command`'s JSON representation tags each variant by name, `fpv-ai`'s tool
catalog (see [AI & MCP](ai-and-mcp.md)) deserializes a tool call's arguments directly into
a `Command` by injecting the tool name as the `command` tag — the tool catalog and the
command bus can never drift apart.

## Data model

`crates/fpv-core/src/model.rs` defines the project data:

- **`Timecode(i64)`** — a point in time or duration stored as microseconds, for
  drift-free arithmetic. `Timecode::from_seconds`/`.seconds()` convert to/from `f64`
  seconds.
- **`ClipId` / `TrackId` / `ProjectId`** — UUID-backed identifiers (`uuid::Uuid`).
- **`Track { id, kind, name, clip_order }`** — `clip_order` is the ordered list of clip
  IDs that play back on this track; `kind` is `Video` or `Audio`.
- **`Clip`** — `source_path`, `in_point`/`out_point` (within the *source* media),
  `position` (on the track's timeline), an optional `StabilizationProfile`, an optional
  `lut_path`, `speed_keyframes`, `text_overlays`, and an optional `osd_source`.
- **`StabilizationProfile { smoothness, strength, horizon_lock, dynamic_fov }`** — see
  [Stabilization](stabilization.md) for what each field does.
- **`Project { format_version, id, name, fps, width, height, tracks, clips }`** —
  `clips` is a `HashMap<ClipId, Clip>`; each track's `clip_order` references into it.
  `Project::duration()` is the latest clip end (`position + source_duration`) across all
  clips.

## Project file format

Project files (`*.fpv.json`) are pretty-printed, human-diffable JSON — a direct
serialization of `Project` via `serde_json`, produced by
`fpv_core::project_file::{save, load, to_json, from_json}`
(`crates/fpv-core/src/project_file.rs`).

Every project carries a `format_version` (currently `1`, `fpv_core::PROJECT_FORMAT_VERSION`).
Loading a file whose `format_version` is *greater* than the version this build supports
is a hard error — this is a forward-compatibility guard, not a migration system; there is
currently no migration path for older-to-newer format changes beyond that the format has
not changed since version 1.

## Media pipeline

`fpv-media` (`crates/fpv-media/src`) shells out to `ffmpeg`/`ffprobe` rather than linking
against them:

- `probe(path)` runs `ffprobe -print_format json -show_format -show_streams` and parses
  duration, resolution, frame rate, video codec, and audio presence.
- `export_clip`/`export_clip_args` build an `ffmpeg` argv for a single clip: trim
  (`-ss`/`-to`), an optional stabilization crop, scale, an optional `lut3d` filter, and an
  approximated speed change (`setpts`, using only the *first* speed keyframe's rate —
  proper per-sample-accurate ramping needs a custom `setpts` expression built from the
  whole curve, tracked as future work).
- `export_timeline`/`export_timeline_with_progress[_and_cancel]` render every video-track
  clip into its timeline position via a single `ffmpeg -filter_complex` graph (overlay
  compositing, audio delay + mix).
- `export_timeline_preview[_range]` renders a fast, low-resolution preview of the visible
  timeline window for the editor's monitor, matching trim/speed/LUT/stabilization-crop
  output rather than showing raw source files. It is video-only; audio mixing stays in
  the final export path.
- `generate_proxy` creates a low-resolution, fast-decode proxy rendition for smooth
  editing playback of large 4K/60 footage.

See [Stabilization](stabilization.md#current-limitation-stabilization-is-not-yet-applied-during-export)
for the current gap between the stabilization math and the export pipeline.

## GPU pipeline

`fpv-gpu` (`crates/fpv-gpu/src`) keeps its color-math and reprojection-math as pure CPU
functions (`color`, `lut`, `warp` modules) so they're testable without a GPU adapter, and
a `wgpu`-based compute pipeline (`pipeline::GpuColorPipeline`) that applies the same
`ColorAdjustments` (exposure, contrast, saturation) on the GPU, checked against the CPU
reference in tests. `GpuColorPipeline::new()` returns `Err(GpuError::NoAdapter)` on
machines without a usable GPU backend — callers (and tests) treat that as "skip the GPU
path," not a hard failure.

## Application service layer (`fpv-app`)

`fpv_app::AppState` (`crates/fpv-app/src/lib.rs`) is the layer the Tauri desktop app is
built on. It holds one shared `CommandBus`, the current AI provider configuration, the
active project path, a preview-render cache/rate-limiter, and export-cancellation state,
and exposes plain async methods (`execute_command`, `undo`, `redo`, `load_project`,
`save_project`, `import_media_paths`, `export_timeline`, `render_preview`, `configure_ai`,
`chat`, `check_for_updates`, ...) that are independently testable without a windowing or
webview runtime. `crates/fpv-app/src/main.rs` wraps each of these in a `#[tauri::command]`
handler and registers them with `tauri::generate_handler!`. See
[Frontend](frontend.md#talking-to-the-backend) for the IPC surface as called from the UI,
and [AI & MCP](ai-and-mcp.md#configuring-a-provider) for how AI provider settings are
persisted.
