# AI & MCP

FPV Editor's AI story has two sides sharing one implementation, per
[PLAN.md](../PLAN.md#4-ai-integration-in-detail): an internal chat panel inside the
desktop app, and an MCP server external agents (such as Claude Code) can connect to. Both
sides call the exact same tool catalog against the exact same kind of `CommandBus`
described in [Architecture](architecture.md#the-command-bus), so nothing an agent does is
a special code path from what the GUI or CLI can do.

## Configuring a provider

FPV Editor speaks the OpenAI chat-completions API dialect via `async-openai`
(`fpv_ai::AiClient`, `crates/fpv-ai/src/client.rs`), configured with a base URL, an
optional API key, a model name, and optional extra headers
(`fpv_ai::ProviderConfig`, `crates/fpv-ai/src/config.rs`). Because it only needs an
OpenAI-compatible `base_url`, this works against OpenAI itself, Azure OpenAI, Ollama, LM
Studio, vLLM, or any other compatible endpoint.

Built-in presets (`fpv_ai::Preset`) fill in a sensible default `base_url`:

| Preset | Default base URL |
| --- | --- |
| `OpenAi` | `https://api.openai.com/v1` |
| `Ollama` | `http://localhost:11434/v1` |
| `LmStudio` | `http://localhost:1234/v1` |
| `Custom` | *(empty — set your own)* |

In the desktop app, configure this in the Settings dialog; it calls the `configure_ai`
Tauri command with a `ProviderConfig`, and `test_ai_connection` sends a minimal one-token
chat request to verify the endpoint is reachable before you rely on it.

### Where settings are stored

`fpv-app`'s `AppState` (`crates/fpv-app/src/lib.rs`) deliberately splits provider
settings into two stores so the app's config file never contains secrets:

- **Non-secret settings** (`base_url`, `model`) are written as pretty-printed JSON to
  `ai-provider.json` inside the OS-specific Tauri app config directory (set up in
  `crates/fpv-app/src/main.rs`'s `setup` hook via `app.path().app_config_dir()`), written
  atomically (write to a `.json.tmp` file, then rename over the target).
- **Secrets** (`api_key` and `extra_headers`, which often carry bearer tokens) are stored
  in the operating system's credential store via the `keyring` crate, under service
  `com.mroxso.fpveditor` and account `ai-provider-settings`. A missing keychain entry
  (e.g. settings copied from another machine, or a cleared keychain) is treated as "no
  secrets configured," not an error.

See [Security](security.md) for the broader policy this implements.

## The shared tool catalog

`fpv_ai::tools::catalog()` (`crates/fpv-ai/src/tools.rs`) is the single list of tools,
defined once and exposed two ways:

- **Internally**, `to_chat_tools()` converts it into OpenAI function-calling
  `ChatCompletionTools` schemas for the internal agent loop.
- **Externally**, `fpv-mcp` converts each `ToolSpec` into an MCP `Tool` (`spec_to_tool` in
  `crates/fpv-mcp/src/lib.rs`) and serves the whole catalog via `list_tools`.

| Tool | Effect |
| --- | --- |
| `list_clips` | Read-only: list all clips with their ids, tracks, and timing |
| `get_timeline_state` | Read-only: the full current project as JSON |
| `add_track` | Add a video or audio track |
| `remove_track` | Remove a track and its clips |
| `add_clip` | Add a clip to a track |
| `remove_clip` | Remove a clip |
| `trim_clip` | Change a clip's in/out points |
| `split_clip` | Split a clip at an absolute timeline position |
| `reorder_clip` | Move a clip within its track's playback order |
| `move_clip` | Move a clip to a different track and/or position |
| `apply_stabilization` | Attach a stabilization profile to a clip (see [Stabilization](stabilization.md)) |
| `apply_lut` | Attach a `.cube` LUT to a clip |
| `set_speed_ramp` | Set speed-ramp keyframes on a clip |
| `add_text_overlay` | Add a positioned, timed text overlay |
| `add_osd_overlay` | Attach an OSD telemetry source (`Betaflight`, `Inav`, `WalkSnail`, `Hdzero`) |

Every mutating tool's JSON Schema mirrors the corresponding `fpv_core::Command` variant's
fields (see [Architecture](architecture.md#the-command-enum)); time values are always
expressed in **microseconds** in tool-call arguments (`fpv_core::Timecode`'s internal
unit), unlike the CLI, which takes seconds as `f32`/`f64`.

`fpv_ai::tools::dispatch(bus, tool_name, args)` executes a tool call by name against a
`CommandBus`: the two read-only tools are handled directly; every other tool name has its
JSON arguments deserialized straight into a `Command` (by injecting the tool name as the
`command` serde tag) and executed via `bus.execute`. An unknown tool name returns
`ToolError::UnknownTool`; malformed arguments return `ToolError::InvalidArguments` rather
than panicking; a command that fails at the `CommandBus` level (e.g. referencing a
missing clip) surfaces as `ToolError::Core`.

## The internal agent

`fpv_ai::agent::run_turn(client, bus, user_prompt)` (`crates/fpv-ai/src/agent.rs`) is the
chat loop behind the desktop app's Copilot panel and the `chat` Tauri command:

1. Send the user's prompt plus the full tool catalog to the configured provider.
2. If the model's response includes tool calls, execute each one via `tools::dispatch`
   against the shared `CommandBus` (so the edit is immediately visible in the GUI's
   timeline), feed each tool's JSON result (or a JSON `{"error": "..."}` payload on
   failure) back to the model as a tool message, and loop.
3. Once the model responds with no tool calls, return its text content as the final
   reply.

The loop caps out after 8 tool-call iterations (`MAX_TOOL_ITERATIONS`), returning
`AgentError::TooManyIterations` rather than looping forever if a model keeps calling
tools without ever producing a final answer. Malformed tool-call JSON from the model is
fed back to it as an error string instead of aborting the turn.

Because this shares the app's single `CommandBus` with the GUI (see
[Architecture](architecture.md#application-service-layer-fpv-app)), a prompt like "cut
out all clips under 2 seconds and stabilize the rest" executes as real, undoable timeline
edits — the same undo/redo history the GUI's undo button walks.

## The MCP server

`fpv-mcp` (`crates/fpv-mcp/src/lib.rs`) wraps the same tool catalog and dispatch logic in
the [Model Context Protocol](https://modelcontextprotocol.io/) via the `rmcp` crate, so
external agents (Claude Code and similar MCP-capable tools) can drive the editor without
any GUI interaction.

Run it over stdio for a given project file:

```sh
fpv mcp-serve project.fpv.json
```

This loads the project (or starts a new `"Untitled"` project if the file does not yet
exist), serves an MCP server over stdio until the connected agent disconnects, then saves
whatever the agent did back to the project file. Connect an MCP-capable agent to this
process the same way you would any other stdio MCP server — point it at the `fpv
mcp-serve <project>` command.

`FpvMcpServer::get_info()` advertises the server's tool-calling capability and an
instructions string telling the connecting agent to drive editing through these tools
instead of a GUI. `list_tools` returns the full catalog; `call_tool` executes one tool
call against the server's shared `CommandBus` (guarded by a `Mutex`) and returns either a
success result (the tool's JSON output as text) or an MCP-level error result — never a
raw protocol error, even for an unknown tool name.

**Important:** `fpv mcp-serve` and a concurrently open GUI session hold *separate*
`CommandBus` instances loaded from the same project file. Running both against the same
file at the same time can silently overwrite one side's edits when either saves — see the
note in [Architecture](architecture.md#the-command-bus).
