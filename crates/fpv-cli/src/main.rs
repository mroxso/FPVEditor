//! `fpv`: headless CLI for scripting the editor (PLAN.md section 2 and
//! roadmap phase 6). Each invocation loads a project file, applies zero or
//! one edits through the same [`fpv_core::CommandBus`] the GUI and AI agent
//! use, and saves the result back — so shell scripts and CI pipelines can
//! drive the exact same editing surface.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fpv_core::{
    ClipId, Command as CoreCommand, NewClip, StabilizationProfile, Timecode, TrackId, TrackKind,
};

mod commands;
use commands::*;

#[derive(Parser)]
#[command(name = "fpv", version, about = "Headless CLI for the FPV Editor")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new, empty project file.
    New {
        project: PathBuf,
        #[arg(long, default_value = "Untitled")]
        name: String,
    },
    /// Add a track (video or audio) to a project.
    AddTrack {
        project: PathBuf,
        #[arg(long, value_enum)]
        kind: TrackKindArg,
        #[arg(long)]
        name: String,
    },
    /// Add a clip to a track.
    AddClip {
        project: PathBuf,
        #[arg(long)]
        track: String,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        r#in: f64,
        #[arg(long)]
        out: f64,
        #[arg(long, default_value_t = 0.0)]
        position: f64,
    },
    /// Change a clip's in/out points.
    TrimClip {
        project: PathBuf,
        #[arg(long)]
        clip: String,
        #[arg(long)]
        r#in: f64,
        #[arg(long)]
        out: f64,
    },
    /// Split a clip at an absolute timeline position (seconds).
    SplitClip {
        project: PathBuf,
        #[arg(long)]
        clip: String,
        #[arg(long)]
        at: f64,
    },
    /// Apply gyro-based stabilization to a clip.
    Stabilize {
        project: PathBuf,
        #[arg(long)]
        clip: String,
        #[arg(long, default_value_t = 0.5)]
        smoothness: f32,
        #[arg(long, default_value_t = 1.0)]
        strength: f32,
        #[arg(long, default_value_t = false)]
        horizon_lock: bool,
        #[arg(long, default_value_t = 0.1)]
        dynamic_fov: f32,
    },
    /// Apply a 3D LUT color grade to a clip.
    ApplyLut {
        project: PathBuf,
        #[arg(long)]
        clip: String,
        #[arg(long)]
        lut: PathBuf,
    },
    /// List clips in the project as JSON.
    List { project: PathBuf },
    /// Print the full project state as JSON.
    Show { project: PathBuf },
    /// Probe a media file's duration/resolution/fps via ffprobe.
    Probe { source: PathBuf },
    /// Render a single clip to a file via ffmpeg.
    Export {
        project: PathBuf,
        #[arg(long)]
        clip: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 1920)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        #[arg(long, default_value_t = 60.0)]
        fps: f64,
    },
    /// Run an MCP server over stdio for external agents (Claude Code, etc.),
    /// per PLAN.md section 4.4. Blocks until the peer disconnects, then
    /// saves whatever the agent did back to the project file.
    McpServe { project: PathBuf },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum TrackKindArg {
    Video,
    Audio,
}

impl From<TrackKindArg> for TrackKind {
    fn from(v: TrackKindArg) -> Self {
        match v {
            TrackKindArg::Video => TrackKind::Video,
            TrackKindArg::Audio => TrackKind::Audio,
        }
    }
}

fn parse_clip_id(s: &str) -> Result<ClipId> {
    Ok(ClipId(uuid::Uuid::parse_str(s).context("invalid clip id")?))
}

fn parse_track_id(s: &str) -> Result<TrackId> {
    Ok(TrackId(uuid::Uuid::parse_str(s).context("invalid track id")?))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::New { project, name } => cmd_new(&project, &name),
        Cmd::AddTrack { project, kind, name } => {
            with_bus(&project, |bus| {
                bus.execute(CoreCommand::AddTrack {
                    kind: kind.into(),
                    name,
                })?;
                Ok(())
            })
        }
        Cmd::AddClip {
            project,
            track,
            source,
            r#in,
            out,
            position,
        } => with_bus(&project, |bus| {
            bus.execute(CoreCommand::AddClip {
                track_id: parse_track_id(&track)?,
                clip: NewClip {
                    source_path: source,
                    in_point: Timecode::from_seconds(r#in),
                    out_point: Timecode::from_seconds(out),
                    position: Timecode::from_seconds(position),
                },
            })?;
            Ok(())
        }),
        Cmd::TrimClip { project, clip, r#in, out } => with_bus(&project, |bus| {
            bus.execute(CoreCommand::TrimClip {
                clip_id: parse_clip_id(&clip)?,
                new_in: Timecode::from_seconds(r#in),
                new_out: Timecode::from_seconds(out),
            })?;
            Ok(())
        }),
        Cmd::SplitClip { project, clip, at } => with_bus(&project, |bus| {
            bus.execute(CoreCommand::SplitClip {
                clip_id: parse_clip_id(&clip)?,
                at: Timecode::from_seconds(at),
            })?;
            Ok(())
        }),
        Cmd::Stabilize {
            project,
            clip,
            smoothness,
            strength,
            horizon_lock,
            dynamic_fov,
        } => with_bus(&project, |bus| {
            bus.execute(CoreCommand::ApplyStabilization {
                clip_id: parse_clip_id(&clip)?,
                profile: StabilizationProfile {
                    smoothness,
                    strength,
                    horizon_lock,
                    dynamic_fov,
                },
            })?;
            Ok(())
        }),
        Cmd::ApplyLut { project, clip, lut } => with_bus(&project, |bus| {
            bus.execute(CoreCommand::ApplyLut {
                clip_id: parse_clip_id(&clip)?,
                lut_path: lut,
            })?;
            Ok(())
        }),
        Cmd::List { project } => cmd_list(&project),
        Cmd::Show { project } => cmd_show(&project),
        Cmd::Probe { source } => cmd_probe(&source),
        Cmd::Export {
            project,
            clip,
            output,
            width,
            height,
            fps,
        } => cmd_export(&project, &parse_clip_id(&clip)?, &output, width, height, fps),
        Cmd::McpServe { project } => cmd_mcp_serve(&project).await,
    }
}
