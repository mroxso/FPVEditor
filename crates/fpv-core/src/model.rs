use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use uuid::Uuid;

macro_rules! def_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

def_id!(ClipId);
def_id!(TrackId);
def_id!(ProjectId);

/// A point in time / duration, stored as microseconds for drift-free arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timecode(pub i64);

impl Timecode {
    pub const ZERO: Timecode = Timecode(0);

    pub fn from_seconds(seconds: f64) -> Self {
        Timecode((seconds * 1_000_000.0).round() as i64)
    }

    pub fn seconds(self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    pub fn checked_sub(self, rhs: Timecode) -> Option<Timecode> {
        self.0.checked_sub(rhs.0).map(Timecode)
    }
}

impl std::ops::Add for Timecode {
    type Output = Timecode;
    fn add(self, rhs: Timecode) -> Timecode {
        Timecode(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Timecode {
    type Output = Timecode;
    fn sub(self, rhs: Timecode) -> Timecode {
        Timecode(self.0 - rhs.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    /// Clip ids in playback order along this track.
    pub clip_order: Vec<ClipId>,
}

impl Track {
    pub fn new(kind: TrackKind, name: impl Into<String>) -> Self {
        Self {
            id: TrackId::new(),
            kind,
            name: name.into(),
            clip_order: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StabilizationProfile {
    /// 0.0 (off) .. 1.0 (max smoothing)
    pub smoothness: f32,
    /// 0.0 (no correction) .. 1.0 (full correction)
    pub strength: f32,
    pub horizon_lock: bool,
    /// Extra crop applied to hide warped edges, 0.0..1.0 of frame size.
    pub dynamic_fov: f32,
}

impl Default for StabilizationProfile {
    fn default() -> Self {
        Self {
            smoothness: 0.5,
            strength: 1.0,
            horizon_lock: false,
            dynamic_fov: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextOverlay {
    pub text: String,
    pub start: Timecode,
    pub end: Timecode,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsdSource {
    Betaflight,
    Inav,
    WalkSnail,
    Hdzero,
}

/// A single keyframe in a speed ramp: at this source-clip time, play at this rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedKeyframe {
    pub at: Timecode,
    pub rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub source_path: PathBuf,
    /// In/out points within the *source* media.
    pub in_point: Timecode,
    pub out_point: Timecode,
    /// Position on the track's timeline.
    pub position: Timecode,
    pub stabilization: Option<StabilizationProfile>,
    pub lut_path: Option<PathBuf>,
    pub speed_keyframes: Vec<SpeedKeyframe>,
    pub text_overlays: Vec<TextOverlay>,
    pub osd_source: Option<OsdSource>,
}

impl Clip {
    pub fn new(source_path: impl Into<PathBuf>, in_point: Timecode, out_point: Timecode) -> Self {
        Self {
            id: ClipId::new(),
            source_path: source_path.into(),
            in_point,
            out_point,
            position: Timecode::ZERO,
            stabilization: None,
            lut_path: None,
            speed_keyframes: Vec::new(),
            text_overlays: Vec::new(),
            osd_source: None,
        }
    }

    pub fn source_duration(&self) -> Timecode {
        self.out_point - self.in_point
    }
}

pub const PROJECT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub id: ProjectId,
    pub name: String,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub tracks: Vec<Track>,
    pub clips: HashMap<ClipId, Clip>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            id: ProjectId::new(),
            name: name.into(),
            fps: 60.0,
            width: 1920,
            height: 1080,
            tracks: Vec::new(),
            clips: HashMap::new(),
        }
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.get(&id)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.clips.get_mut(&id)
    }

    /// Track that currently owns this clip, if any.
    pub fn track_of_clip(&self, clip_id: ClipId) -> Option<TrackId> {
        self.tracks
            .iter()
            .find(|t| t.clip_order.contains(&clip_id))
            .map(|t| t.id)
    }

    /// Total timeline duration = latest clip end across all tracks.
    pub fn duration(&self) -> Timecode {
        self.clips
            .values()
            .map(|c| c.position + c.source_duration())
            .max()
            .unwrap_or(Timecode::ZERO)
    }
}
