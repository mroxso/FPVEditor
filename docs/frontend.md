# Frontend

The FPV Editor desktop UI lives in `frontend/` and runs inside the Tauri app's webview.
It is a React + TypeScript single-page app built with Vite and styled with Tailwind CSS
v4 and `shadcn`-style components.

## Structure

```
frontend/
  src/
    main.tsx              entire application: App shell + every workspace/panel component
    styles.css             Tailwind entry point
    assets/                static assets (logo, etc.)
    components/ui/         reusable, shadcn-style primitives (button, card, dialog, input, ...)
    lib/utils.ts            shared helpers (currently the `cn()` class-merging helper)
  index.html
  vite.config.ts
  tsconfig.json
  components.json          shadcn component generator config
```

Unlike a typical multi-file React app, `frontend/src/main.tsx` currently contains the
whole application — the root `App` component plus every top-level panel (project
launcher, import workspace, export workspace, monitor/preview, clip inspector, timeline,
Copilot chat panel, and the settings dialog) as sibling functions in one file, wired
together with local component state and a handful of `useEffect`/`useCallback` hooks. The
reusable primitives under `components/ui/` (button, card, dialog, input, label,
separator, slider, switch, tabs, textarea, toggle, toggle-group, tooltip, badge) are the
only pieces split into their own files.

TypeScript is strict (`tsconfig.json`: `"strict": true`) and uses the `@/` path alias for
`frontend/src` (e.g. `@/components/ui/button`). Name React components in `PascalCase`,
helpers in `camelCase`, and keep the existing two-space indentation.

## Key panels (in `main.tsx`)

| Component | Role |
| --- | --- |
| `App` | Root shell: owns the current `Project`, undo/redo state, active phase (import/cut/export), and wires keyboard shortcuts (undo/redo, etc.) |
| `ProjectLauncher` | New/open/recent-project screen shown before a project is loaded |
| `ImportWorkspace` / `ImportMediaCard` | Media import UI backed by `import_media` |
| `Preview` | The monitor: renders either a single clip or the composited timeline via `render_preview`, switching between `"clip"` and `"timeline"` preview modes |
| `Timeline` | Multi-track timeline: clip selection, trim, split, drag-reorder, driving `execute` |
| `ClipInspector` | Per-clip property panel (trim, stabilization, LUT, speed ramp, overlays) |
| `ExportWorkspace` | Export settings UI backed by `export_capabilities` and `export_timeline`, with a cancel button wired to `cancel_export` |
| `Copilot` | The internal AI chat panel, backed by the `chat` command (see [AI & MCP](ai-and-mcp.md#the-internal-agent)) |
| `SettingsDialog` | AI provider configuration (`configure_ai`, `test_ai_connection`, `ai_config`), FFmpeg diagnostics (`media_diagnostics`), and update checks (`check_for_updates`, `download_update`) |

## Talking to the backend

The frontend never touches the filesystem or FFmpeg directly. Every mutation and every
piece of state comes from the Rust backend via Tauri's `invoke()` (from
`@tauri-apps/api/core`), calling the `#[tauri::command]` handlers registered in
`crates/fpv-app/src/main.rs` (see
[Architecture](architecture.md#application-service-layer-fpv-app)):

| Frontend call | Backend command | Purpose |
| --- | --- | --- |
| `invoke("timeline")` | `timeline` | Fetch the current project state |
| `invoke("execute", { command })` | `execute` | Apply a `Command` (add/trim/split/stabilize/...) through the shared `CommandBus` |
| `invoke("undo")` / `invoke("redo")` | `undo` / `redo` | Undo/redo the last command |
| `invoke("new_project", { name })` | `new_project` | Start a new, empty project |
| `invoke("load_project", { path })` | `load_project` | Load a project file from disk |
| `invoke("save_project", { path })` | `save_project` | Save the current project to disk |
| `invoke("import_media", { paths, targetTrackId })` | `import_media` | Import source media files (or a directory) as clips, linking their original paths |
| `invoke("media_diagnostics")` | `media_diagnostics` | Report whether `ffmpeg`/`ffprobe` are available |
| `invoke("render_preview", { clipId, start })` | `render_preview` | Render a clip or timeline-window preview for the monitor |
| `invoke("export_capabilities")` | `export_capabilities` | Report which containers/codecs the local FFmpeg build supports |
| `invoke("export_timeline", { settings })` | `export_timeline` | Render the full timeline to a file, streaming `"export-progress"` events |
| `invoke("cancel_export")` | `cancel_export` | Cancel an in-progress timeline export |
| `invoke("configure_ai", { config })` | `configure_ai` | Save AI provider settings |
| `invoke("ai_config")` | `ai_config` | Read back the current AI provider configuration |
| `invoke("test_ai_connection")` | `test_ai_connection` | Verify the configured AI endpoint is reachable |
| `invoke("chat", { prompt })` | `chat` | Send a message to the internal AI agent |
| `invoke("check_for_updates")` | `check_for_updates` | Check GitHub releases for a newer version |
| `invoke("download_update", { downloadUrl, assetName })` | `download_update` | Download and open an update installer |

`export_timeline` reports progress via a Tauri event (`app.emit("export-progress", ...)`
on the Rust side), consumed with `@tauri-apps/api/event`'s `listen`. File paths returned
for previews/imports are converted to webview-loadable URLs with `convertFileSrc` and
must be allow-listed via the Tauri asset protocol scope — the corresponding
`#[tauri::command]` handlers do this (`app.asset_protocol_scope().allow_file(...)`)
before returning a path to the frontend.

## Development workflow

```sh
cd frontend
npm install       # or: npm ci, for a reproducible install from package-lock.json
npm run dev        # Vite dev server on http://127.0.0.1:1420
npm run build       # tsc --noEmit type-check, then a production build
npm run preview      # preview a production build locally
```

Running `cargo run -p fpv-app` launches the full desktop app (Tauri shell + this
frontend); see [Getting Started](getting-started.md#option-3-build-from-source). There is
no frontend test runner — `npm run build`'s type-check is the required UI validation (see
[Development](development.md)).
