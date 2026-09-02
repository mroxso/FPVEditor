//! `fpv-media`: decode/encode via `ffmpeg`/`ffprobe`, media probing, and
//! proxy generation, per PLAN.md section 2.

pub mod error;
pub mod export;
pub mod probe;
pub mod process;

pub use error::{MediaError, MediaResult};
pub use export::{
    export_capabilities, export_clip, export_clip_args, export_clip_preview, export_timeline,
    export_timeline_preview, export_timeline_preview_range, export_timeline_with_progress,
    export_timeline_with_progress_and_cancel, generate_proxy, preview_dimensions, AudioCodec,
    ExportCapabilities, ExportContainer, ExportSettings, VideoCodec,
};
pub use probe::{probe, MediaInfo};
pub use process::ToolDiagnostic;
