# CLI Reference

`fpv` is the headless CLI for FPV Editor (`crates/fpv-cli`). Each invocation loads a
project file, applies zero or one edits through the same `fpv_core::CommandBus` the GUI
and AI agent use, and — for mutating commands — saves the result back to disk. This makes
`fpv` safe to drive from shell scripts or CI pipelines: one command, one atomic edit.

Every command prints its result as pretty-printed JSON on stdout. A failed command
leaves the project file untouched (see [Architecture](architecture.md#the-command-bus)).

```
fpv --help
fpv <command> --help
```

Track and clip identifiers are UUIDs; get them from the output of `new`, `add-track`,
`add-clip`, `list`, or `show`.

## `fpv new`

Create a new, empty project file. Refuses to overwrite an existing file.

```sh
fpv new <PROJECT> [--name <NAME>]
```

| Argument | Description |
| --- | --- |
| `PROJECT` (positional) | Path to the project file to create |
| `--name <NAME>` | Project name (default: `Untitled`) |

```sh
fpv new project.fpv.json --name "My FPV Edit"
```

## `fpv add-track`

Add a video or audio track to a project.

```sh
fpv add-track <PROJECT> --kind <video|audio> --name <NAME>
```

| Argument | Description |
| --- | --- |
| `PROJECT` (positional) | Path to the project file |
| `--kind <video\|audio>` | Track kind (required) |
| `--name <NAME>` | Track name (required) |

```sh
fpv add-track project.fpv.json --kind video --name V1
```

## `fpv add-clip`

Add a clip to a track. `--in`/`--out` are in/out points within the source media file,
in seconds; `--position` is where the clip lands on the track's timeline, in seconds.

```sh
fpv add-clip <PROJECT> --track <TRACK_ID> --source <SOURCE> --in <SECONDS> --out <SECONDS> [--position <SECONDS>]
```

| Argument | Description |
| --- | --- |
| `PROJECT` (positional) | Path to the project file |
| `--track <TRACK_ID>` | Target track's UUID (required) |
| `--source <SOURCE>` | Path to the source media file (required) |
| `--in <SECONDS>` | In point within the source, in seconds (required) |
| `--out <SECONDS>` | Out point within the source, in seconds (required) |
| `--position <SECONDS>` | Timeline position, in seconds (default: `0.0`) |

```sh
fpv add-clip project.fpv.json --track <track-id> \
  --source run.mp4 --in 0 --out 12
```

## `fpv trim-clip`

Change a clip's in/out points within its source media.

```sh
fpv trim-clip <PROJECT> --clip <CLIP_ID> --in <SECONDS> --out <SECONDS>
```

```sh
fpv trim-clip project.fpv.json --clip <clip-id> --in 1 --out 8
```

## `fpv split-clip`

Split a clip into two at an absolute timeline position (seconds), not a position within
the source media. The position must fall strictly inside the clip's visible range on the
timeline.

```sh
fpv split-clip <PROJECT> --clip <CLIP_ID> --at <SECONDS>
```

```sh
fpv split-clip project.fpv.json --clip <clip-id> --at 4
```

## `fpv stabilize`

Apply a gyro-based stabilization profile to a clip. See
[Stabilization](stabilization.md) for what each flag does and current limitations.

```sh
fpv stabilize <PROJECT> --clip <CLIP_ID> \
  [--smoothness <0.0-1.0>] [--strength <0.0-1.0>] \
  [--horizon-lock] [--dynamic-fov <0.0-1.0>]
```

| Argument | Description | Default |
| --- | --- | --- |
| `--clip <CLIP_ID>` | Target clip's UUID (required) | — |
| `--smoothness <FLOAT>` | 0.0 (off) .. 1.0 (max smoothing) | `0.5` |
| `--strength <FLOAT>` | 0.0 (no correction) .. 1.0 (full correction) | `1.0` |
| `--horizon-lock` | Keep the horizon level (flag, no value) | off |
| `--dynamic-fov <FLOAT>` | Extra crop/zoom to hide warped edges, 0.0..1.0 of frame size | `0.1` |

```sh
fpv stabilize project.fpv.json --clip <clip-id> \
  --smoothness 0.6 --strength 1.0 --horizon-lock --dynamic-fov 0.15
```

## `fpv apply-lut`

Apply a 3D `.cube` LUT color grade to a clip.

```sh
fpv apply-lut <PROJECT> --clip <CLIP_ID> --lut <LUT_PATH>
```

```sh
fpv apply-lut project.fpv.json --clip <clip-id> --lut warm.cube
```

## `fpv list`

List all clips in the project as JSON (a JSON array).

```sh
fpv list <PROJECT>
```

## `fpv show`

Print the full project state (tracks and clips) as JSON.

```sh
fpv show <PROJECT>
```

## `fpv probe`

Probe a media file's duration, resolution, frame rate, video codec, and audio presence
via `ffprobe`. Does not touch a project file.

```sh
fpv probe <SOURCE>
```

```sh
fpv probe run.mp4
```

Output fields: `duration_us`, `width`, `height`, `fps`, `video_codec`, `has_audio`.

## `fpv export`

Render a single clip to a file via `ffmpeg`. Applies the clip's trim, stabilization crop
(see the note in [Stabilization](stabilization.md)), LUT, and speed-ramp approximation.

```sh
fpv export <PROJECT> --clip <CLIP_ID> --output <OUTPUT> \
  [--width <PIXELS>] [--height <PIXELS>] [--fps <FPS>]
```

| Argument | Description | Default |
| --- | --- | --- |
| `--clip <CLIP_ID>` | Clip to export (required) | — |
| `--output <OUTPUT>` | Output file path (required) | — |
| `--width <PIXELS>` | Output width | `1920` |
| `--height <PIXELS>` | Output height | `1080` |
| `--fps <FPS>` | Output frame rate | `60.0` |

```sh
fpv export project.fpv.json --clip <clip-id> --output out.mp4
```

The CLI's `export` command always encodes H.264/AAC into an MP4 container with `crf`
unset (defaults to 23); other containers/codecs are only exposed via the Tauri app's
`export_timeline` command (`fpv_media::ExportSettings`), not yet as CLI flags.

## `fpv mcp-serve`

Run an MCP server over stdio for external agents (Claude Code, etc.), exposing the same
tool catalog documented in [AI & MCP](ai-and-mcp.md). Blocks until the peer disconnects,
then saves whatever the agent did back to the project file. If the project file does not
yet exist, starts from a new, empty `"Untitled"` project.

```sh
fpv mcp-serve <PROJECT>
```

```sh
fpv mcp-serve project.fpv.json
```
