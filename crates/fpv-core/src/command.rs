use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{CoreError, CoreResult};
use crate::model::{
    Clip, ClipId, OsdSource, Project, SpeedKeyframe, StabilizationProfile, TextOverlay, Timecode,
    Track, TrackId, TrackKind,
};

/// Data needed to create a new clip; distinct from `Clip` so callers don't
/// have to invent an id (the command bus mints one on execution).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewClip {
    pub source_path: PathBuf,
    pub in_point: Timecode,
    pub out_point: Timecode,
    pub position: Timecode,
}

/// The single source of truth for every timeline mutation. The GUI, the
/// internal AI agent, an MCP server and the CLI all funnel through this enum
/// (see PLAN.md section 2 and 4.2 — this doubles as the tool-call schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    AddTrack {
        kind: TrackKind,
        name: String,
    },
    RemoveTrack {
        track_id: TrackId,
    },
    AddClip {
        track_id: TrackId,
        clip: NewClip,
    },
    RemoveClip {
        clip_id: ClipId,
    },
    TrimClip {
        clip_id: ClipId,
        new_in: Timecode,
        new_out: Timecode,
    },
    /// Trim the beginning of a clip while keeping its visible left edge in
    /// sync with the new timeline position. This is deliberately a single
    /// command so one trim gesture is one undoable editor action.
    TrimClipStart {
        clip_id: ClipId,
        new_in: Timecode,
        new_position: Timecode,
    },
    SplitClip {
        clip_id: ClipId,
        /// Absolute timeline position at which to split.
        at: Timecode,
    },
    ReorderClip {
        track_id: TrackId,
        clip_id: ClipId,
        new_index: usize,
    },
    MoveClip {
        clip_id: ClipId,
        new_track_id: TrackId,
        new_position: Timecode,
    },
    ApplyStabilization {
        clip_id: ClipId,
        profile: StabilizationProfile,
    },
    ApplyLut {
        clip_id: ClipId,
        lut_path: PathBuf,
    },
    SetSpeedRamp {
        clip_id: ClipId,
        keyframes: Vec<SpeedKeyframe>,
    },
    AddTextOverlay {
        clip_id: ClipId,
        overlay: TextOverlay,
    },
    AddOsdOverlay {
        clip_id: ClipId,
        source: OsdSource,
    },
}

/// Index at which to insert a clip at `position` into `clip_order` so the
/// track stays sorted by each clip's timeline position (ties are inserted
/// after existing clips at the same position).
fn clip_order_insert_index(
    clip_order: &[ClipId],
    clips: &std::collections::HashMap<ClipId, Clip>,
    position: Timecode,
) -> usize {
    clip_order
        .iter()
        .position(|id| clips.get(id).is_some_and(|c| c.position > position))
        .unwrap_or(clip_order.len())
}

impl Command {
    /// Human-readable summary, e.g. for an undo-history UI or agent transcript.
    pub fn describe(&self) -> String {
        match self {
            Command::AddTrack { kind, name } => format!("Add {kind:?} track '{name}'"),
            Command::RemoveTrack { track_id } => format!("Remove track {track_id}"),
            Command::AddClip { .. } => "Add clip".to_string(),
            Command::RemoveClip { clip_id } => format!("Remove clip {clip_id}"),
            Command::TrimClip { clip_id, .. } => format!("Trim clip {clip_id}"),
            Command::TrimClipStart { clip_id, .. } => format!("Trim clip start {clip_id}"),
            Command::SplitClip { clip_id, .. } => format!("Split clip {clip_id}"),
            Command::ReorderClip { clip_id, .. } => format!("Reorder clip {clip_id}"),
            Command::MoveClip { clip_id, .. } => format!("Move clip {clip_id}"),
            Command::ApplyStabilization { clip_id, .. } => {
                format!("Apply stabilization to {clip_id}")
            }
            Command::ApplyLut { clip_id, .. } => format!("Apply LUT to {clip_id}"),
            Command::SetSpeedRamp { clip_id, .. } => format!("Set speed ramp on {clip_id}"),
            Command::AddTextOverlay { clip_id, .. } => format!("Add text overlay to {clip_id}"),
            Command::AddOsdOverlay { clip_id, .. } => format!("Add OSD overlay to {clip_id}"),
        }
    }

    /// Apply this command to the project in place.
    pub(crate) fn apply(&self, project: &mut Project) -> CoreResult<()> {
        match self {
            Command::AddTrack { kind, name } => {
                project.tracks.push(Track::new(*kind, name.clone()));
                Ok(())
            }
            Command::RemoveTrack { track_id } => {
                let track = project
                    .track(*track_id)
                    .ok_or(CoreError::TrackNotFound(*track_id))?;
                let clip_ids = track.clip_order.clone();
                project.tracks.retain(|t| t.id != *track_id);
                for clip_id in clip_ids {
                    project.clips.remove(&clip_id);
                }
                Ok(())
            }
            Command::AddClip { track_id, clip } => {
                if clip.in_point < Timecode::ZERO || clip.out_point <= clip.in_point {
                    return Err(CoreError::InvalidTrim {
                        clip: ClipId::new(),
                        in_point: clip.in_point.0,
                        out_point: clip.out_point.0,
                        source_duration: 0,
                    });
                }
                let mut new_clip =
                    Clip::new(clip.source_path.clone(), clip.in_point, clip.out_point);
                new_clip.position = clip.position;
                let track_idx = project
                    .tracks
                    .iter()
                    .position(|t| t.id == *track_id)
                    .ok_or(CoreError::TrackNotFound(*track_id))?;
                let idx = clip_order_insert_index(
                    &project.tracks[track_idx].clip_order,
                    &project.clips,
                    new_clip.position,
                );
                project.tracks[track_idx]
                    .clip_order
                    .insert(idx, new_clip.id);
                project.clips.insert(new_clip.id, new_clip);
                Ok(())
            }
            Command::RemoveClip { clip_id } => {
                if !project.clips.contains_key(clip_id) {
                    return Err(CoreError::ClipNotFound(*clip_id));
                }
                for track in &mut project.tracks {
                    track.clip_order.retain(|c| c != clip_id);
                }
                project.clips.remove(clip_id);
                Ok(())
            }
            Command::TrimClip {
                clip_id,
                new_in,
                new_out,
            } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                if *new_in < Timecode::ZERO || *new_out <= *new_in {
                    return Err(CoreError::InvalidTrim {
                        clip: *clip_id,
                        in_point: new_in.0,
                        out_point: new_out.0,
                        source_duration: clip.source_duration().0,
                    });
                }
                clip.in_point = *new_in;
                clip.out_point = *new_out;
                Ok(())
            }
            Command::TrimClipStart {
                clip_id,
                new_in,
                new_position,
            } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                if *new_in < Timecode::ZERO
                    || *new_position < Timecode::ZERO
                    || *new_in >= clip.out_point
                {
                    return Err(CoreError::InvalidTrim {
                        clip: *clip_id,
                        in_point: new_in.0,
                        out_point: clip.out_point.0,
                        source_duration: clip.source_duration().0,
                    });
                }
                clip.in_point = *new_in;
                clip.position = *new_position;
                Ok(())
            }
            Command::SplitClip { clip_id, at } => {
                let track_id = project
                    .track_of_clip(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                let (start, end, source_path, in_point) = {
                    let clip = project.clip(*clip_id).unwrap();
                    (
                        clip.position,
                        clip.position + clip.source_duration(),
                        clip.source_path.clone(),
                        clip.in_point,
                    )
                };
                if *at <= start || *at >= end {
                    return Err(CoreError::InvalidSplit {
                        clip: *clip_id,
                        at: at.0,
                        start: start.0,
                        end: end.0,
                    });
                }
                let offset_into_clip = *at - start;
                let split_source_point = in_point + offset_into_clip;

                // Shrink the original clip to end at the split point.
                {
                    let clip = project.clip_mut(*clip_id).unwrap();
                    clip.out_point = split_source_point;
                }

                // Create the tail as a new clip immediately after it.
                let mut tail = Clip::new(source_path, split_source_point, in_point + (end - start));
                tail.position = *at;
                let tail_id = tail.id;
                project.clips.insert(tail_id, tail);

                let track = project.track_mut(track_id).unwrap();
                let idx = track
                    .clip_order
                    .iter()
                    .position(|c| c == clip_id)
                    .expect("clip indexed on its own track");
                track.clip_order.insert(idx + 1, tail_id);
                Ok(())
            }
            Command::ReorderClip {
                track_id,
                clip_id,
                new_index,
            } => {
                let track = project
                    .track_mut(*track_id)
                    .ok_or(CoreError::TrackNotFound(*track_id))?;
                let cur = track
                    .clip_order
                    .iter()
                    .position(|c| c == clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                let clip = track.clip_order.remove(cur);
                let idx = (*new_index).min(track.clip_order.len());
                track.clip_order.insert(idx, clip);
                Ok(())
            }
            Command::MoveClip {
                clip_id,
                new_track_id,
                new_position,
            } => {
                if !project.clips.contains_key(clip_id) {
                    return Err(CoreError::ClipNotFound(*clip_id));
                }
                if project.track(*new_track_id).is_none() {
                    return Err(CoreError::TrackNotFound(*new_track_id));
                }
                for track in &mut project.tracks {
                    track.clip_order.retain(|c| c != clip_id);
                }
                let track_idx = project
                    .tracks
                    .iter()
                    .position(|t| t.id == *new_track_id)
                    .unwrap();
                let idx = clip_order_insert_index(
                    &project.tracks[track_idx].clip_order,
                    &project.clips,
                    *new_position,
                );
                project.tracks[track_idx].clip_order.insert(idx, *clip_id);
                project.clip_mut(*clip_id).unwrap().position = *new_position;
                Ok(())
            }
            Command::ApplyStabilization { clip_id, profile } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                clip.stabilization = Some(*profile);
                Ok(())
            }
            Command::ApplyLut { clip_id, lut_path } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                clip.lut_path = Some(lut_path.clone());
                Ok(())
            }
            Command::SetSpeedRamp { clip_id, keyframes } => {
                if keyframes.is_empty()
                    || !keyframes.windows(2).all(|w| w[0].at < w[1].at)
                    || keyframes.iter().any(|k| k.rate <= 0.0)
                {
                    return Err(CoreError::InvalidSpeedRamp);
                }
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                clip.speed_keyframes = keyframes.clone();
                Ok(())
            }
            Command::AddTextOverlay { clip_id, overlay } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                clip.text_overlays.push(overlay.clone());
                Ok(())
            }
            Command::AddOsdOverlay { clip_id, source } => {
                let clip = project
                    .clip_mut(*clip_id)
                    .ok_or(CoreError::ClipNotFound(*clip_id))?;
                clip.osd_source = Some(*source);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackKind;

    fn project_with_track() -> (Project, TrackId) {
        let mut project = Project::new("test");
        Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        }
        .apply(&mut project)
        .unwrap();
        let track_id = project.tracks[0].id;
        (project, track_id)
    }

    fn add_clip_at(project: &mut Project, track_id: TrackId, position_secs: f64) -> ClipId {
        let before: std::collections::HashSet<ClipId> = project.clips.keys().copied().collect();
        Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "a.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(1.0),
                position: Timecode::from_seconds(position_secs),
            },
        }
        .apply(project)
        .unwrap();
        *project
            .clips
            .keys()
            .find(|id| !before.contains(id))
            .unwrap()
    }

    #[test]
    fn add_clip_inserts_into_clip_order_sorted_by_position_not_call_order() {
        let (mut project, track_id) = project_with_track();
        let first = add_clip_at(&mut project, track_id, 10.0);
        let second = add_clip_at(&mut project, track_id, 0.0);
        assert_eq!(project.tracks[0].clip_order, vec![second, first]);
    }

    #[test]
    fn remove_track_removes_its_clips() {
        let (mut project, track_id) = project_with_track();
        let clip_id = add_clip_at(&mut project, track_id, 0.0);

        Command::RemoveTrack { track_id }
            .apply(&mut project)
            .unwrap();

        assert!(project.tracks.is_empty());
        assert!(!project.clips.contains_key(&clip_id));
    }

    #[test]
    fn move_clip_inserts_into_the_destination_tracks_clip_order_by_position() {
        let (mut project, track_id) = project_with_track();
        let early = add_clip_at(&mut project, track_id, 0.0);
        let late = add_clip_at(&mut project, track_id, 10.0);

        Command::AddTrack {
            kind: TrackKind::Video,
            name: "V2".into(),
        }
        .apply(&mut project)
        .unwrap();
        let track2_id = project.tracks[1].id;
        let middle = add_clip_at(&mut project, track2_id, 5.0);

        Command::MoveClip {
            clip_id: middle,
            new_track_id: track_id,
            new_position: Timecode::from_seconds(5.0),
        }
        .apply(&mut project)
        .unwrap();

        assert_eq!(project.tracks[0].clip_order, vec![early, middle, late]);
        assert!(project.tracks[1].clip_order.is_empty());
    }

    #[test]
    fn speed_ramp_with_nonpositive_rate_is_rejected() {
        let (mut project, track_id) = project_with_track();
        let clip_id = add_clip_at(&mut project, track_id, 0.0);
        let err = Command::SetSpeedRamp {
            clip_id,
            keyframes: vec![SpeedKeyframe {
                at: Timecode::ZERO,
                rate: 0.0,
            }],
        }
        .apply(&mut project)
        .unwrap_err();
        assert_eq!(err, CoreError::InvalidSpeedRamp);
    }

    #[test]
    fn tail_trim_updates_the_project_duration_to_the_new_clip_end() {
        let (mut project, track_id) = project_with_track();
        let clip_id = add_clip_at(&mut project, track_id, 2.0);
        let clip = project.clip_mut(clip_id).unwrap();
        clip.out_point = Timecode::from_seconds(10.0);
        assert_eq!(project.duration(), Timecode::from_seconds(12.0));

        Command::TrimClip {
            clip_id,
            new_in: Timecode::ZERO,
            new_out: Timecode::from_seconds(3.0),
        }
        .apply(&mut project)
        .unwrap();

        assert_eq!(project.duration(), Timecode::from_seconds(5.0));
    }

    #[test]
    fn clips_cannot_be_trimmed_before_the_start_of_the_source() {
        let (mut project, track_id) = project_with_track();
        let clip_id = add_clip_at(&mut project, track_id, 0.0);

        let error = Command::TrimClip {
            clip_id,
            new_in: Timecode::from_seconds(-1.0),
            new_out: Timecode::from_seconds(1.0),
        }
        .apply(&mut project)
        .unwrap_err();

        assert!(matches!(error, CoreError::InvalidTrim { .. }));
    }
}
