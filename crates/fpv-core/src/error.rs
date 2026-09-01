use crate::model::{ClipId, TrackId};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("clip {0} not found")]
    ClipNotFound(ClipId),
    #[error("track {0} not found")]
    TrackNotFound(TrackId),
    #[error("invalid trim: in={in_point} out={out_point} for clip {clip} (source duration {source_duration})")]
    InvalidTrim {
        clip: ClipId,
        in_point: i64,
        out_point: i64,
        source_duration: i64,
    },
    #[error("invalid split point {at} for clip {clip} (bounds [{start}, {end}])")]
    InvalidSplit {
        clip: ClipId,
        at: i64,
        start: i64,
        end: i64,
    },
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("track {0} is not empty")]
    TrackNotEmpty(TrackId),
    #[error("invalid speed ramp: keyframes must be non-empty and time-ordered")]
    InvalidSpeedRamp,
    #[error("serialization error: {0}")]
    Serde(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
