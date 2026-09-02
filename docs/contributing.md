# Contributing

Contributions are welcome. This page covers the contribution workflow; see
[Development](development.md) for build/test/lint commands and coding conventions.

## Before you start

For significant changes, open an issue first to discuss the approach. This avoids
spending effort on a design that doesn't fit the project's goals — see
[PLAN.md](../PLAN.md) for the project's goals, non-goals, and architecture, and the
[README's project status](../README.md#project-status) for what is and isn't implemented
yet (in particular, note the current gap between stabilization math and export described
in [Stabilization](stabilization.md#current-limitation-stabilization-is-not-yet-applied-during-export)).

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/) for every commit:

| Type | Use for |
| --- | --- |
| `feat:` | User-visible features |
| `fix:` | Bug fixes |
| `docs:` | Documentation-only changes |
| `test:` | Test-only changes |
| `ci:` | CI/workflow changes |
| `build:` | Build-system/dependency changes |
| `refactor:` | Non-behavior-changing restructuring |
| `chore:` | Everything else (housekeeping, tooling) |

Use `!` after the type (e.g. `feat!:`) or a `BREAKING CHANGE:` footer for incompatible
changes. These types and markers directly drive the automated release process — see
[RELEASES.md](../RELEASES.md) and [Development's Releases section](development.md#releases).

Keep each commit scoped to one coherent change.

## Pull requests

A pull request should:

- Explain the behavior change and why it's needed.
- List the verification commands you ran (e.g. `cargo test --workspace`, `cargo clippy
  --workspace --all-targets`, `cargo fmt --all -- --check`, `npm run build`) — see
  [Development](development.md#build-test-and-lint).
- Link relevant issues or the [PLAN.md](../PLAN.md) section the change relates to.
- Include a screenshot for any visible UI change.
- Call out any new dependency on FFmpeg, GPU hardware, a specific AI provider, or a
  project-file format change, so reviewers and CI maintainers know what environment is
  needed to verify it.

Run the full check suite from [Development](development.md#build-test-and-lint) before
opening the pull request.

## Architectural rule that reviewers will enforce

Every timeline mutation must go through `fpv_core::CommandBus` by adding or reusing a
`Command` variant (see [Architecture](architecture.md#the-command-bus)). A change that
mutates a `Project` directly from the GUI, the AI agent, the MCP server, or the CLI
without going through the command bus will not preserve undo/redo, will not be reachable
by the AI tool catalog (which is derived directly from `Command`, see
[AI & MCP](ai-and-mcp.md#the-shared-tool-catalog)), and should be restructured before
merge.

## Security and configuration

Never commit API keys, provider credentials, or custom authorization headers. See
[Security](security.md) for how these are meant to be stored and handled.
