//! `fpv-core`: the project/timeline data model, the command bus, and
//! undo/redo. This is the single source of truth for all timeline
//! mutations — the GUI, the internal AI agent, the MCP server, and the CLI
//! all funnel through [`bus::CommandBus`] executing [`command::Command`]s.

pub mod bus;
pub mod command;
pub mod error;
pub mod model;
pub mod project_file;

pub use bus::CommandBus;
pub use command::{Command, NewClip};
pub use error::{CoreError, CoreResult};
pub use model::{
    Clip, ClipId, OsdSource, Project, ProjectId, SpeedKeyframe, StabilizationProfile,
    TextOverlay, Timecode, Track, TrackId, TrackKind,
};
