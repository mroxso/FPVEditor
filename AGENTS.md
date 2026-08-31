# Repository Guidelines

## Project Structure & Module Organization

This Rust 2021 Cargo workspace keeps domain code under `crates/`: `fpv-core` owns the model and command bus, `fpv-media` wraps FFmpeg, `fpv-stabilize` and `fpv-gpu` handle image processing, and `fpv-ai`/`fpv-mcp` expose agent integrations. User-facing entry points are `fpv-cli` and the Tauri app in `fpv-app`. The React/TypeScript UI is in `frontend/src`; reusable controls are under `frontend/src/components/ui`. See `PLAN.md` for architecture and `README.md` for current status.

## Build, Test, and Development Commands

- `cargo test --workspace`: run all Rust unit and integration tests.
- `cargo clippy --workspace --all-targets`: lint every crate and target.
- `cargo fmt --all -- --check`: verify Rust formatting without modifying files.
- `cargo run -p fpv-app`: launch the desktop app after installing frontend dependencies.
- `cargo run -p fpv-cli -- --help`: inspect the headless editor commands.
- `cd frontend && npm install`: install pinned frontend dependencies.
- `cd frontend && npm run dev`: run Vite on `127.0.0.1:1420`.
- `cd frontend && npm run build`: type-check with `tsc` and create a production bundle.

Full media tests require `ffmpeg` and `ffprobe` on `PATH`; GPU tests require a usable `wgpu` adapter. Tests skip gracefully when these are unavailable.

## Coding Style & Naming Conventions

Use standard `rustfmt` output (four-space indentation), `snake_case` for Rust functions/modules, and `PascalCase` for types. Keep timeline mutations flowing through `fpv-core::CommandBus`; AI, MCP, CLI, and GUI paths should not implement separate mutation logic. TypeScript is strict and uses the `@/` alias for `frontend/src`. Name React components in `PascalCase`, helpers in `camelCase`, and preserve the existing two-space frontend indentation.

## Testing Guidelines

Place Rust unit tests beside implementation code in `#[cfg(test)]` modules. Put public-interface coverage in crate-level `tests/` directories, following `crates/fpv-cli/tests/cli.rs`. Use descriptive `snake_case` names and add regression coverage for bug fixes. There is no frontend test runner; `npm run build` is the required UI validation.

## Commit & Pull Request Guidelines

Recent commits use short, imperative subjects such as `Implement Tauri FPV editor frontend` and `Fix stabilization ... bugs`. Keep each commit scoped to one coherent change. Pull requests should explain the behavior change, list verification commands, link relevant issues or `PLAN.md` sections, and include screenshots for visible UI changes. Call out dependencies on FFmpeg, GPU hardware, API providers, or project-file compatibility.

## Security & Configuration

Never commit API keys or provider credentials. Keep OpenAI-compatible endpoint settings local, and avoid logging secrets or custom authorization headers.
