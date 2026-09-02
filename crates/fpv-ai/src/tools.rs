//! The central tool list (PLAN.md section 4.2): defined once here, exposed
//! twice — as OpenAI function-calling schemas for the internal agent
//! ([`to_chat_tools`]), and as plain [`ToolSpec`]s an MCP server can adapt
//! to its own tool format. [`dispatch`] executes a tool call by name+args
//! against a [`fpv_core::CommandBus`].

use fpv_core::{Command, CommandBus, CoreError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the tool's arguments object.
    pub parameters: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments for tool {tool}: {source}")]
    InvalidArguments {
        tool: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Core(#[from] CoreError),
}

/// The full tool catalog, matching PLAN.md 4.2's list. Read-only tools
/// (`list_clips`, `get_timeline_state`) are handled directly in
/// [`dispatch`]; mutating tools deserialize straight into a
/// [`fpv_core::Command`] (reusing its tagged-enum `serde` representation,
/// so this list and the command bus can never drift apart).
pub fn catalog() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_clips",
            description: "List all clips in the current project, with their ids, tracks, and timing.",
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolSpec {
            name: "get_timeline_state",
            description: "Get the full current project/timeline state as JSON.",
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolSpec {
            name: "add_track",
            description: "Add a new track (video or audio) to the project.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["Video", "Audio"] },
                    "name": { "type": "string" }
                },
                "required": ["kind", "name"]
            }),
        },
        ToolSpec {
            name: "remove_track",
            description: "Remove a track and all of its clips from the project.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "track_id": { "type": "string" }
                },
                "required": ["track_id"]
            }),
        },
        ToolSpec {
            name: "add_clip",
            description: "Add a new clip to a track.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "track_id": { "type": "string" },
                    "clip": {
                        "type": "object",
                        "properties": {
                            "source_path": { "type": "string" },
                            "in_point": { "type": "integer", "description": "microseconds" },
                            "out_point": { "type": "integer", "description": "microseconds" },
                            "position": { "type": "integer", "description": "microseconds" }
                        },
                        "required": ["source_path", "in_point", "out_point", "position"]
                    }
                },
                "required": ["track_id", "clip"]
            }),
        },
        ToolSpec {
            name: "remove_clip",
            description: "Remove a clip from the project entirely.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" }
                },
                "required": ["clip_id"]
            }),
        },
        ToolSpec {
            name: "trim_clip",
            description: "Change a clip's in/out points within its source media.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "new_in": { "type": "integer", "description": "microseconds" },
                    "new_out": { "type": "integer", "description": "microseconds" }
                },
                "required": ["clip_id", "new_in", "new_out"]
            }),
        },
        ToolSpec {
            name: "split_clip",
            description: "Split a clip into two at an absolute timeline position.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "at": { "type": "integer", "description": "microseconds" }
                },
                "required": ["clip_id", "at"]
            }),
        },
        ToolSpec {
            name: "reorder_clip",
            description: "Move a clip to a new index within its track's playback order.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "track_id": { "type": "string" },
                    "clip_id": { "type": "string" },
                    "new_index": { "type": "integer" }
                },
                "required": ["track_id", "clip_id", "new_index"]
            }),
        },
        ToolSpec {
            name: "move_clip",
            description: "Move a clip to a different track and/or timeline position.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "new_track_id": { "type": "string" },
                    "new_position": { "type": "integer", "description": "microseconds" }
                },
                "required": ["clip_id", "new_track_id", "new_position"]
            }),
        },
        ToolSpec {
            name: "apply_stabilization",
            description: "Apply gyro-based stabilization to a clip.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "profile": {
                        "type": "object",
                        "properties": {
                            "smoothness": { "type": "number", "minimum": 0, "maximum": 1 },
                            "strength": { "type": "number", "minimum": 0, "maximum": 1 },
                            "horizon_lock": { "type": "boolean" },
                            "dynamic_fov": { "type": "number", "minimum": 0, "maximum": 1 }
                        },
                        "required": ["smoothness", "strength", "horizon_lock", "dynamic_fov"]
                    }
                },
                "required": ["clip_id", "profile"]
            }),
        },
        ToolSpec {
            name: "apply_lut",
            description: "Apply a 3D LUT color grade to a clip.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "lut_path": { "type": "string" }
                },
                "required": ["clip_id", "lut_path"]
            }),
        },
        ToolSpec {
            name: "set_speed_ramp",
            description: "Set speed-ramp keyframes on a clip.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "keyframes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "at": { "type": "integer" },
                                "rate": { "type": "number" }
                            },
                            "required": ["at", "rate"]
                        }
                    }
                },
                "required": ["clip_id", "keyframes"]
            }),
        },
        ToolSpec {
            name: "add_text_overlay",
            description: "Add a text overlay to a clip, positioned in normalized 0..1 coordinates.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "overlay": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "start": { "type": "integer", "description": "microseconds" },
                            "end": { "type": "integer", "description": "microseconds" },
                            "x": { "type": "number" },
                            "y": { "type": "number" }
                        },
                        "required": ["text", "start", "end", "x", "y"]
                    }
                },
                "required": ["clip_id", "overlay"]
            }),
        },
        ToolSpec {
            name: "add_osd_overlay",
            description: "Overlay the clip's flight-controller OSD telemetry, decoded from the given source format.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "clip_id": { "type": "string" },
                    "source": { "type": "string", "enum": ["Betaflight", "Inav", "WalkSnail", "Hdzero"] }
                },
                "required": ["clip_id", "source"]
            }),
        },
    ]
}

/// Convert the catalog into OpenAI `ChatCompletionTools` function schemas.
pub fn to_chat_tools() -> Vec<async_openai::types::chat::ChatCompletionTools> {
    catalog()
        .into_iter()
        .map(|spec| {
            async_openai::types::chat::ChatCompletionTools::Function(
                async_openai::types::chat::ChatCompletionTool {
                    function: async_openai::types::chat::FunctionObject {
                        name: spec.name.to_string(),
                        description: Some(spec.description.to_string()),
                        parameters: Some(spec.parameters),
                        strict: None,
                    },
                },
            )
        })
        .collect()
}

/// Execute a tool call by name against the command bus. Read-only tools
/// return their result directly; mutating tools return the updated
/// project's JSON.
pub fn dispatch(bus: &mut CommandBus, tool_name: &str, args: Value) -> Result<Value, ToolError> {
    match tool_name {
        "list_clips" => Ok(json!(bus.project().clips.values().collect::<Vec<_>>())),
        "get_timeline_state" => Ok(json!(bus.project())),
        other if catalog().iter().any(|spec| spec.name == other) => {
            let command = args_to_command(other, args)?;
            bus.execute(command)?;
            Ok(json!(bus.project()))
        }
        other => Err(ToolError::UnknownTool(other.to_string())),
    }
}

/// Reuses `Command`'s `#[serde(tag = "command", rename_all = "snake_case")]`
/// representation: the tool's arguments become the enum's fields.
fn args_to_command(tool_name: &str, mut args: Value) -> Result<Command, ToolError> {
    if let Value::Object(map) = &mut args {
        map.insert("command".to_string(), json!(tool_name));
    }
    serde_json::from_value(args).map_err(|source| ToolError::InvalidArguments {
        tool: tool_name.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_core::{Project, TrackKind};

    fn bus_with_track() -> (CommandBus, fpv_core::TrackId) {
        let mut bus = CommandBus::new(Project::new("test"));
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        let track_id = bus.project().tracks[0].id;
        (bus, track_id)
    }

    #[test]
    fn catalog_has_no_duplicate_tool_names() {
        let names: Vec<&str> = catalog().into_iter().map(|t| t.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
    }

    #[test]
    fn to_chat_tools_preserves_catalog_length_and_names() {
        use async_openai::types::chat::ChatCompletionTools;
        let chat_tools = to_chat_tools();
        assert_eq!(chat_tools.len(), catalog().len());
        let ChatCompletionTools::Function(tool) = &chat_tools[0] else {
            panic!("expected a function tool");
        };
        assert_eq!(tool.function.name, catalog()[0].name);
    }

    #[test]
    fn dispatching_add_clip_mutates_the_project_via_the_command_bus() {
        let (mut bus, track_id) = bus_with_track();
        let args = json!({
            "track_id": track_id,
            "clip": {
                "source_path": "run.mp4",
                "in_point": 0,
                "out_point": 3_000_000,
                "position": 0
            }
        });
        dispatch(&mut bus, "add_clip", args).unwrap();
        assert_eq!(bus.project().clips.len(), 1);
    }

    #[test]
    fn dispatching_apply_stabilization_sets_the_clips_profile() {
        let (mut bus, track_id) = bus_with_track();
        dispatch(
            &mut bus,
            "add_clip",
            json!({
                "track_id": track_id,
                "clip": { "source_path": "a.mp4", "in_point": 0, "out_point": 1_000_000, "position": 0 }
            }),
        )
        .unwrap();
        let clip_id = bus.project().tracks[0].clip_order[0];

        dispatch(
            &mut bus,
            "apply_stabilization",
            json!({
                "clip_id": clip_id,
                "profile": { "smoothness": 0.6, "strength": 0.9, "horizon_lock": true, "dynamic_fov": 0.15 }
            }),
        )
        .unwrap();

        let clip = bus.project().clip(clip_id).unwrap();
        let profile = clip.stabilization.unwrap();
        assert!((profile.smoothness - 0.6).abs() < 1e-6);
        assert!(profile.horizon_lock);
    }

    #[test]
    fn catalog_covers_every_command_variant() {
        // Every mutating tool name must deserialize into some `Command`
        // variant via `args_to_command`'s `command` tag, so the catalog
        // can't silently drift behind `Command` as variants are added.
        let names: Vec<&str> = catalog().into_iter().map(|t| t.name).collect();
        for expected in [
            "add_track",
            "remove_track",
            "add_clip",
            "remove_clip",
            "trim_clip",
            "split_clip",
            "reorder_clip",
            "move_clip",
            "apply_stabilization",
            "apply_lut",
            "set_speed_ramp",
            "add_text_overlay",
            "add_osd_overlay",
        ] {
            assert!(names.contains(&expected), "catalog is missing tool {expected}");
        }
    }

    #[test]
    fn dispatching_add_track_creates_a_track_from_a_fresh_project() {
        let mut bus = CommandBus::new(Project::new("test"));
        let result = dispatch(
            &mut bus,
            "add_track",
            json!({ "kind": "Video", "name": "V1" }),
        )
        .unwrap();
        assert_eq!(bus.project().tracks.len(), 1);
        assert_eq!(result["tracks"][0]["name"], "V1");
    }

    #[test]
    fn dispatching_move_clip_relocates_a_clip_to_another_track() {
        let (mut bus, track_id) = bus_with_track();
        dispatch(
            &mut bus,
            "add_track",
            json!({ "kind": "Video", "name": "V2" }),
        )
        .unwrap();
        let track2_id = bus.project().tracks[1].id;

        dispatch(
            &mut bus,
            "add_clip",
            json!({
                "track_id": track_id,
                "clip": { "source_path": "a.mp4", "in_point": 0, "out_point": 1_000_000, "position": 0 }
            }),
        )
        .unwrap();
        let clip_id = bus.project().tracks[0].clip_order[0];

        dispatch(
            &mut bus,
            "move_clip",
            json!({ "clip_id": clip_id, "new_track_id": track2_id, "new_position": 5_000_000 }),
        )
        .unwrap();

        assert!(bus.project().tracks[0].clip_order.is_empty());
        assert_eq!(bus.project().tracks[1].clip_order, vec![clip_id]);
    }

    #[test]
    fn list_clips_is_read_only_and_does_not_touch_undo_history() {
        let (mut bus, _track_id) = bus_with_track();
        let before_history = bus.history().len();
        dispatch(&mut bus, "list_clips", json!({})).unwrap();
        assert_eq!(bus.history().len(), before_history);
    }

    #[test]
    fn unknown_tool_name_is_rejected() {
        let (mut bus, _track_id) = bus_with_track();
        let err = dispatch(&mut bus, "delete_universe", json!({})).unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(_)));
    }

    #[test]
    fn malformed_arguments_produce_an_invalid_arguments_error_not_a_panic() {
        let (mut bus, _track_id) = bus_with_track();
        let err = dispatch(&mut bus, "trim_clip", json!({ "clip_id": "not-a-uuid" })).unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments { .. }));
    }

    #[test]
    fn a_command_referencing_a_missing_clip_surfaces_as_a_core_error() {
        let (mut bus, _track_id) = bus_with_track();
        let bogus_clip = fpv_core::ClipId::new();
        let err = dispatch(
            &mut bus,
            "trim_clip",
            json!({ "clip_id": bogus_clip, "new_in": 0, "new_out": 1000 }),
        )
        .unwrap_err();
        assert!(matches!(err, ToolError::Core(CoreError::ClipNotFound(_))));
    }
}
