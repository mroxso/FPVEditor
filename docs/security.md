# Security & Configuration

## API keys and provider credentials

Never commit API keys, provider credentials, or custom authorization headers. Keep
OpenAI-compatible endpoint settings local, and avoid logging secrets or custom
authorization headers.

## How the desktop app stores AI provider settings

`fpv-app` (`crates/fpv-app/src/lib.rs`) splits AI provider configuration into two stores
specifically so secrets never land in a plain config file on disk:

- `base_url` and `model` (non-secret) are written as JSON to `ai-provider.json` inside
  the OS-specific Tauri app config directory.
- `api_key` and `extra_headers` (secrets — headers are treated as secrets too, since they
  often carry bearer tokens) are stored in the operating system's credential store via
  the `keyring` crate, under service `com.mroxso.fpveditor`, account
  `ai-provider-settings`.

See [AI & MCP](ai-and-mcp.md#where-settings-are-stored) for the full detail. If you touch
this code path, preserve that split: do not add a code path that writes `api_key` or
`extra_headers` to the plain JSON settings file, and do not log a `ProviderConfig`'s
`api_key` or `extra_headers` fields.

`fpv-app`'s test suite includes a check
(`public_ai_settings_never_include_credentials` in `crates/fpv-app/src/lib.rs`) asserting
that the struct written to the plain settings file has no field capable of holding a
secret — keep that invariant if you change `SavedAiSettings`.

## The CLI and MCP server

`fpv mcp-serve` and the `fpv-ai`/`fpv-mcp` crates take a `ProviderConfig` (or, for
`mcp-serve`, no AI configuration at all — it only exposes the project's editing tools)
constructed by the caller; the CLI and MCP server do not themselves read or write any
credential store. If you build automation around `fpv-ai::AiClient` directly, keep API
keys out of shell history and process listings (e.g. read them from an environment
variable or a local file with restricted permissions, not a bare CLI flag).

## Reporting

If you discover a security issue, please open an issue on the
[GitHub repository](https://github.com/mroxso/FPVEditor) so it can be triaged. Avoid
including live credentials or exploit details in a public issue.
