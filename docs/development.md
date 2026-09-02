# Development

This page collects the day-to-day commands and conventions for working on FPV Editor.
For install instructions, see [Getting Started](getting-started.md); for the pull request
process, see [Contributing](contributing.md).

## Prerequisites

- Rust stable (edition 2021)
- Node.js and npm
- `ffmpeg` and `ffprobe` on `PATH` — required for the full `fpv-media` test suite and for
  probing/exporting media in the app or CLI
- A usable `wgpu` adapter (Metal, Vulkan, or DX12) — required for the GPU-backed tests in
  `fpv-gpu`

Tests that need FFmpeg or a GPU adapter skip gracefully (with an `eprintln!` explaining
why) when those are unavailable, rather than failing the suite.

## Build, test, and lint

```sh
cargo test --workspace                        # run all Rust unit and integration tests
cargo clippy --workspace --all-targets         # lint every crate and target
cargo fmt --all -- --check                     # verify Rust formatting without modifying files
cargo run -p fpv-app                            # launch the desktop app (after `npm install` in frontend/)
cargo run -p fpv-cli -- --help                   # inspect the headless CLI

cd frontend && npm install                       # install pinned frontend dependencies
cd frontend && npm run dev                        # Vite dev server on 127.0.0.1:1420
cd frontend && npm run build                        # tsc --noEmit type-check + production bundle
```

Run all four Rust checks (`test`, `clippy`, `fmt --check`) plus `npm run build` before
opening a pull request — this is the same set CI runs.

## Coding style and naming

- Standard `rustfmt` output: four-space indentation.
- `snake_case` for Rust functions and modules, `PascalCase` for types.
- Keep every timeline mutation flowing through `fpv_core::CommandBus` (see
  [Architecture](architecture.md#the-command-bus)) — the AI, MCP, CLI, and GUI paths must
  not implement separate mutation logic; add a new `Command` variant instead.
- TypeScript is strict and uses the `@/` alias for `frontend/src`. React components are
  `PascalCase`, helpers are `camelCase`, and the frontend uses two-space indentation (see
  [Frontend](frontend.md)).

## Testing guidelines

- Place Rust unit tests beside implementation code in `#[cfg(test)]` modules (this is
  how the vast majority of the workspace's tests are organized — see any `crates/*/src/*.rs`
  file for examples).
- Put public-interface / end-to-end coverage in crate-level `tests/` directories,
  following `crates/fpv-cli/tests/cli.rs`, which drives the real `fpv` binary as a
  subprocess the way a shell script or CI pipeline would.
- Use descriptive `snake_case` test names that describe the behavior under test (e.g.
  `add_clip_inserts_into_clip_order_sorted_by_position_not_call_order`), and add
  regression coverage for bug fixes.
- There is no frontend test runner. `npm run build`'s type-check is the required UI
  validation; manually exercise the feature in the running app for behavioral
  verification.
- Tests that shell out to `ffmpeg`/`ffprobe` or that need a `wgpu` adapter check
  availability first and skip (printing why) rather than failing when the environment
  doesn't provide them — follow that pattern for new tests with the same dependencies.

## Workspace layout

See [Architecture](architecture.md#crate-layout) for what each crate is responsible for.
In short: domain code lives under `crates/` (`fpv-core` owns the model and command bus;
`fpv-media` wraps FFmpeg; `fpv-stabilize` and `fpv-gpu` handle image processing;
`fpv-ai`/`fpv-mcp` expose agent integrations); user-facing entry points are `fpv-cli` and
the Tauri app in `fpv-app`; the frontend lives in `frontend/src`, with reusable controls
under `frontend/src/components/ui`.

## Releases

Releases use [Release Please](https://github.com/googleapis/release-please) — see
[RELEASES.md](../RELEASES.md) for the full process. In short: conventional commit types
on `main` (`fix:`, `feat:`, `feat!:`/`BREAKING CHANGE:`) drive automatic patch/minor/major
version bumps and a standing release pull request; merging that PR tags and publishes a
release, and GitHub Actions attaches macOS ARM64, Windows x64, and Linux x64 bundles.
