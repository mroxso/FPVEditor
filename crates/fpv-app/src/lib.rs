//! `fpv-app`: the application service layer that wires every engine crate
//! together (PLAN.md section 2's "wires all crates together"). Each method
//! on [`AppState`] is what a Tauri `#[tauri::command]` IPC handler would
//! call directly — kept as plain, independently-testable async functions
//! here so the wiring is verified without needing a windowing/webview
//! runtime, which this environment doesn't have.
//!
//! The internal AI agent panel drives the *same* [`fpv_core::CommandBus`]
//! instance held here, so an edit it makes is immediately visible to the
//! GUI and vice versa. An external MCP server (`fpv mcp-serve`, see
//! `fpv-mcp`/`fpv-cli`) is a **separate process** with its own `CommandBus`
//! loaded from the project file — it does not share this instance, so
//! running the GUI and `mcp-serve` against the same project file
//! concurrently can silently overwrite one side's edits on save.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use fpv_ai::{AiClient, ProviderConfig};
use fpv_core::{Command, CommandBus, NewClip, Project, Timecode, TrackKind};
use serde::Serialize;
use tokio::sync::Mutex;

pub struct AppState {
    bus: Arc<Mutex<CommandBus>>,
    ai_config: Arc<Mutex<Option<ProviderConfig>>>,
    project_path: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(Project::new("Untitled"))
    }
}

#[derive(Debug, Serialize)]
pub struct ExecuteOutcome {
    pub project: Project,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Result of importing source media. Files are never copied: every added
/// `Clip` retains the absolute path that was selected or discovered.
#[derive(Debug, Serialize)]
pub struct MediaImportOutcome {
    pub project: Project,
    pub imported_paths: Vec<PathBuf>,
    pub skipped_paths: Vec<PathBuf>,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl AppState {
    pub fn new(project: Project) -> Self {
        Self {
            bus: Arc::new(Mutex::new(CommandBus::new(project))),
            ai_config: Arc::new(Mutex::new(None)),
            project_path: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn load_project(&self, path: &Path) -> Result<Project> {
        let project = fpv_core::project_file::load(path)
            .with_context(|| format!("failed to load project at {}", path.display()))?;
        let mut bus = self.bus.lock().await;
        *bus = CommandBus::new(project.clone());
        *self.project_path.lock().await = Some(path.to_path_buf());
        Ok(project)
    }

    pub async fn save_project(&self, path: &Path) -> Result<()> {
        let bus = self.bus.lock().await;
        fpv_core::project_file::save(bus.project(), path)
            .with_context(|| format!("failed to save project at {}", path.display()))?;
        *self.project_path.lock().await = Some(path.to_path_buf());
        Ok(())
    }

    pub async fn get_timeline_state(&self) -> Project {
        self.bus.lock().await.project().clone()
    }

    /// Apply one edit through the shared command bus — the single mutation
    /// path for the GUI, the internal AI agent, and (via `fpv-mcp`, which
    /// wraps its own bus the same way) external agents.
    pub async fn execute_command(&self, command: Command) -> Result<ExecuteOutcome> {
        let mut bus = self.bus.lock().await;
        bus.execute(command)?;
        Ok(ExecuteOutcome {
            project: bus.project().clone(),
            can_undo: bus.can_undo(),
            can_redo: bus.can_redo(),
        })
    }

    pub async fn undo(&self) -> Result<ExecuteOutcome> {
        let mut bus = self.bus.lock().await;
        bus.undo()?;
        Ok(ExecuteOutcome {
            project: bus.project().clone(),
            can_undo: bus.can_undo(),
            can_redo: bus.can_redo(),
        })
    }

    pub async fn redo(&self) -> Result<ExecuteOutcome> {
        let mut bus = self.bus.lock().await;
        bus.redo()?;
        Ok(ExecuteOutcome {
            project: bus.project().clone(),
            can_undo: bus.can_undo(),
            can_redo: bus.can_redo(),
        })
    }

    pub fn probe_media(&self, path: &Path) -> Result<fpv_media::MediaInfo> {
        fpv_media::probe(path).context("ffprobe failed")
    }

    /// Import files or recursively scan folders for video sources. This only
    /// reads metadata through ffprobe; originals (including network shares)
    /// remain in place and project clips reference their source paths.
    pub async fn import_media_paths(&self, paths: Vec<PathBuf>) -> Result<MediaImportOutcome> {
        let candidates = collect_media_files(&paths)?;
        if candidates.is_empty() {
            anyhow::bail!("no supported video files were found")
        }

        let mut readable = Vec::new();
        let mut skipped_paths = Vec::new();
        for path in candidates {
            match fpv_media::probe(&path) {
                Ok(info) if info.duration_us > 0 => readable.push((path, info.duration_us)),
                _ => skipped_paths.push(path),
            }
        }

        let mut bus = self.bus.lock().await;
        let existing_sources: std::collections::HashSet<PathBuf> = bus
            .project()
            .clips
            .values()
            .map(|clip| clip.source_path.clone())
            .collect();
        readable.retain(|(path, _)| {
            if existing_sources.contains(path) {
                skipped_paths.push(path.clone());
                false
            } else {
                true
            }
        });
        if readable.is_empty() {
            anyhow::bail!("no readable video files could be imported; ensure ffprobe is installed and the source is available")
        }

        let video_track = match bus
            .project()
            .tracks
            .iter()
            .find(|track| track.kind == TrackKind::Video)
        {
            Some(track) => track.id,
            None => {
                bus.execute(Command::AddTrack {
                    kind: TrackKind::Video,
                    name: "V1".into(),
                })?;
                bus.project().tracks.last().expect("new track exists").id
            }
        };

        let mut imported_paths = Vec::new();
        let mut next_position = bus.project().duration();
        for (path, duration_us) in readable {
            bus.execute(Command::AddClip {
                track_id: video_track,
                clip: NewClip {
                    source_path: path.clone(),
                    in_point: Timecode::ZERO,
                    out_point: Timecode(duration_us),
                    position: next_position,
                },
            })?;
            next_position = Timecode(next_position.0 + duration_us);
            imported_paths.push(path);
        }
        Ok(MediaImportOutcome {
            project: bus.project().clone(),
            imported_paths,
            skipped_paths,
            can_undo: bus.can_undo(),
            can_redo: bus.can_redo(),
        })
    }

    pub fn export_clip(
        &self,
        clip: &fpv_core::Clip,
        settings: &fpv_media::ExportSettings,
    ) -> Result<()> {
        fpv_media::export_clip(clip, settings).context("ffmpeg export failed")
    }

    /// PLAN.md section 4.1: configure the AI provider (base URL/key/model).
    pub async fn configure_ai(&self, config: ProviderConfig) {
        *self.ai_config.lock().await = Some(config);
    }

    pub async fn test_ai_connection(&self) -> Result<()> {
        let config = self
            .ai_config
            .lock()
            .await
            .clone()
            .context("AI provider is not configured")?;
        AiClient::new(&config).test_connection().await?;
        Ok(())
    }

    /// PLAN.md section 4.3: the internal agent panel — sees the current
    /// timeline and can edit it through the same tool catalog as the MCP
    /// server exposes to external agents.
    pub async fn chat(&self, prompt: &str) -> Result<String> {
        let config = self
            .ai_config
            .lock()
            .await
            .clone()
            .context("AI provider is not configured")?;
        let client = AiClient::new(&config);
        let mut bus = self.bus.lock().await;
        let reply = fpv_ai::run_turn(&client, &mut bus, prompt).await?;
        Ok(reply)
    }
}

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "m4v", "avi", "webm", "mts", "m2ts"];

fn collect_media_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        collect_media_path(path, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_media_path(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot access media source at {}", path.display()))?;
    if metadata.is_file() {
        if is_supported_video(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .with_context(|| format!("cannot read media folder at {}", path.display()))?
        {
            collect_media_path(&entry?.path(), files)?;
        }
    }
    Ok(())
}

fn is_supported_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_core::TrackKind;
    use wiremock::matchers::{method, path as wpath};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn media_collection_recurses_and_keeps_only_supported_video_paths() {
        let root =
            std::env::temp_dir().join(format!("fpv-media-collection-{}", std::process::id()));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let source = root.join("flight.MP4");
        let nested_source = nested.join("goggles.mov");
        std::fs::write(&source, []).unwrap();
        std::fs::write(&nested_source, []).unwrap();
        std::fs::write(root.join("notes.txt"), []).unwrap();

        let found = collect_media_files(&[root.clone()]).unwrap();

        assert_eq!(found, vec![source, nested_source]);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn media_import_links_the_original_source_path_without_copying_it() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("skipping: ffmpeg is not available on PATH");
            return;
        }
        let root = std::env::temp_dir().join(format!("fpv-app-import-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("network-source.mp4");
        let generated = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x120:rate=24:duration=1",
                "-pix_fmt",
                "yuv420p",
                source.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(generated.status.success());

        let outcome = AppState::default()
            .import_media_paths(vec![source.clone()])
            .await
            .unwrap();

        assert_eq!(outcome.imported_paths, vec![source.clone()]);
        assert_eq!(outcome.project.clips.len(), 1);
        assert_eq!(
            outcome.project.clips.values().next().unwrap().source_path,
            source
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn execute_command_mutates_shared_state_visible_to_get_timeline_state() {
        let state = AppState::default();
        state
            .execute_command(Command::AddTrack {
                kind: TrackKind::Video,
                name: "V1".into(),
            })
            .await
            .unwrap();
        let project = state.get_timeline_state().await;
        assert_eq!(project.tracks.len(), 1);
    }

    #[tokio::test]
    async fn undo_after_add_track_restores_empty_project() {
        let state = AppState::default();
        let outcome = state
            .execute_command(Command::AddTrack {
                kind: TrackKind::Video,
                name: "V1".into(),
            })
            .await
            .unwrap();
        assert!(outcome.can_undo);
        assert!(!outcome.can_redo);

        let after_undo = state.undo().await.unwrap();
        assert!(after_undo.project.tracks.is_empty());
        assert!(after_undo.can_redo);
    }

    #[tokio::test]
    async fn save_then_load_round_trips_through_a_real_file() {
        let dir = std::env::temp_dir().join(format!("fpv-app-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proj.fpv.json");

        let state = AppState::default();
        state
            .execute_command(Command::AddTrack {
                kind: TrackKind::Audio,
                name: "A1".into(),
            })
            .await
            .unwrap();
        state.save_project(&path).await.unwrap();

        let fresh_state = AppState::default();
        let loaded = fresh_state.load_project(&path).await.unwrap();
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.tracks[0].kind, TrackKind::Audio);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn chat_without_ai_configured_returns_a_clear_error_not_a_panic() {
        let state = AppState::default();
        let err = state.chat("do something").await.unwrap_err();
        assert!(err.to_string().contains("not configured"));
    }

    #[tokio::test]
    async fn chat_drives_the_same_command_bus_the_gui_uses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wpath("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "c1", "object": "chat.completion", "created": 1, "model": "m",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "done" },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
            })))
            .mount(&server)
            .await;

        let state = AppState::default();
        state
            .configure_ai(ProviderConfig::new(server.uri(), "test-model"))
            .await;

        let reply = state.chat("hello").await.unwrap();
        assert_eq!(reply, "done");
    }

    #[tokio::test]
    async fn a_clip_added_by_chat_is_visible_via_get_timeline_state() {
        let server = MockServer::start().await;
        let state = AppState::default();
        let outcome = state
            .execute_command(Command::AddTrack {
                kind: TrackKind::Video,
                name: "V1".into(),
            })
            .await
            .unwrap();
        let track_id = outcome.project.tracks[0].id;

        let tool_args = serde_json::json!({
            "track_id": track_id,
            "clip": { "source_path": "run.mp4", "in_point": 0, "out_point": 1_000_000, "position": 0 }
        });
        Mock::given(method("POST"))
            .and(wpath("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "c1", "object": "chat.completion", "created": 1, "model": "m",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": { "name": "add_clip", "arguments": tool_args.to_string() }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(wpath("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "c2", "object": "chat.completion", "created": 1, "model": "m",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "added it" },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
            })))
            .mount(&server)
            .await;

        state
            .configure_ai(ProviderConfig::new(server.uri(), "test-model"))
            .await;
        let reply = state.chat("add my run.mp4 clip").await.unwrap();
        assert_eq!(reply, "added it");

        // Read through the GUI-facing accessor, not the tool-call result,
        // to prove the agent's edit landed in the shared command bus.
        let project = state.get_timeline_state().await;
        assert_eq!(project.clips.len(), 1);
    }
}
