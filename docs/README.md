# FPV Editor Documentation

This directory documents FPV Editor's architecture, CLI, AI/MCP integration, frontend,
and contributor workflow in more depth than the top-level [README](../README.md). Start
with [Getting Started](getting-started.md) if you are new to the project, or jump to a
specific topic below.

| Document | Covers |
| --- | --- |
| [Getting Started](getting-started.md) | Installing the desktop app, the CLI, the CLI Docker image, or building from source; creating your first project |
| [Architecture](architecture.md) | Workspace/crate layout, the command bus, data flow, the project file format |
| [CLI Reference](cli-reference.md) | Every `fpv` subcommand, its flags, and examples |
| [Stabilization](stabilization.md) | Gyro-based stabilization, horizon lock, rolling-shutter correction, lens distortion, dynamic FOV, and current limitations |
| [AI & MCP](ai-and-mcp.md) | Configuring an OpenAI-compatible provider, the internal chat agent, the shared tool catalog, and the MCP server for external agents |
| [Frontend](frontend.md) | The React/TypeScript UI, its structure, and how it talks to the Rust backend over Tauri IPC |
| [Development](development.md) | Build/test/lint commands, coding conventions, and testing guidelines |
| [Contributing](contributing.md) | Contribution workflow, commit conventions, and pull request expectations |
| [Security](security.md) | How API keys and credentials are handled |

For the project's goals, non-goals, and roadmap, see [PLAN.md](../PLAN.md). For current
implementation status, see the [README](../README.md#project-status). For the release
process, see [RELEASES.md](../RELEASES.md).
