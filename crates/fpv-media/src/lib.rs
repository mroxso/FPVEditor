//! `fpv-media`: decode/encode via `ffmpeg`/`ffprobe`, media probing, and
//! proxy generation, per PLAN.md section 2.

pub mod error;
pub mod export;
pub mod probe;
pub mod process;

pub use error::{MediaError, MediaResult};
pub use export::{
    export_clip, export_clip_args, export_clip_preview, export_timeline_preview,
    export_timeline_preview_range, generate_proxy, preview_dimensions, ExportSettings,
};
pub use probe::{probe, MediaInfo};
