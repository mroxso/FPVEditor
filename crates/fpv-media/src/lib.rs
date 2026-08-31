//! `fpv-media`: decode/encode via `ffmpeg`/`ffprobe`, media probing, and
//! proxy generation, per PLAN.md section 2.

pub mod error;
pub mod export;
pub mod probe;
pub mod process;

pub use error::{MediaError, MediaResult};
pub use export::{export_clip, export_clip_args, generate_proxy, ExportSettings};
pub use probe::{probe, MediaInfo};
