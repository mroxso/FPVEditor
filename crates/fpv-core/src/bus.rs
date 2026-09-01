use crate::command::Command;
use crate::error::{CoreError, CoreResult};
use crate::model::Project;

/// Executes [`Command`]s against a [`Project`] and maintains undo/redo
/// history. This is the single mutation path shared by the GUI, the
/// internal AI agent, the MCP server, and the CLI (PLAN.md section 2).
pub struct CommandBus {
    project: Project,
    undo_stack: Vec<(Command, Project)>,
    redo_stack: Vec<(Command, Project)>,
}

impl CommandBus {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Execute a command. On success it becomes undoable and the redo stack
    /// is cleared (standard editor semantics: a new edit invalidates future
    /// redo history). On failure the project is left untouched.
    pub fn execute(&mut self, command: Command) -> CoreResult<()> {
        let pre_state = self.project.clone();
        command.apply(&mut self.project)?;
        self.undo_stack.push((command, pre_state));
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> CoreResult<Command> {
        let (command, pre_state) = self.undo_stack.pop().ok_or(CoreError::NothingToUndo)?;
        let post_state = std::mem::replace(&mut self.project, pre_state);
        self.redo_stack.push((command.clone(), post_state));
        Ok(command)
    }

    pub fn redo(&mut self) -> CoreResult<Command> {
        let (command, post_state) = self.redo_stack.pop().ok_or(CoreError::NothingToRedo)?;
        let pre_state = std::mem::replace(&mut self.project, post_state);
        self.undo_stack.push((command.clone(), pre_state));
        Ok(command)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Descriptions of executed commands, oldest first — for an undo-history panel.
    pub fn history(&self) -> Vec<String> {
        self.undo_stack.iter().map(|(c, _)| c.describe()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::NewClip;
    use crate::model::{Timecode, TrackKind};

    fn bus_with_video_track() -> (CommandBus, crate::model::TrackId) {
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
    fn execute_add_track_then_undo_restores_empty_project() {
        let mut bus = CommandBus::new(Project::new("test"));
        assert!(bus.project().tracks.is_empty());
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        assert_eq!(bus.project().tracks.len(), 1);
        bus.undo().unwrap();
        assert!(bus.project().tracks.is_empty());
        assert!(!bus.can_undo());
    }

    #[test]
    fn redo_replays_an_undone_command() {
        let (mut bus, track_id) = bus_with_video_track();
        bus.execute(Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "a.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(2.0),
                position: Timecode::ZERO,
            },
        })
        .unwrap();
        assert_eq!(bus.project().clips.len(), 1);
        bus.undo().unwrap();
        assert_eq!(bus.project().clips.len(), 0);
        bus.redo().unwrap();
        assert_eq!(bus.project().clips.len(), 1);
    }

    #[test]
    fn a_new_command_clears_redo_history() {
        let (mut bus, track_id) = bus_with_video_track();
        bus.execute(Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "a.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(2.0),
                position: Timecode::ZERO,
            },
        })
        .unwrap();
        bus.undo().unwrap();
        assert!(bus.can_redo());
        bus.execute(Command::AddTrack {
            kind: TrackKind::Audio,
            name: "A1".into(),
        })
        .unwrap();
        assert!(!bus.can_redo());
    }

    #[test]
    fn failed_command_does_not_pollute_undo_stack_or_mutate_state() {
        let (mut bus, _track_id) = bus_with_video_track();
        let before = bus.project().clone();
        let history_len_before = bus.history().len();
        let bogus_track = crate::model::TrackId::new();
        let err = bus
            .execute(Command::AddClip {
                track_id: bogus_track,
                clip: NewClip {
                    source_path: "a.mp4".into(),
                    in_point: Timecode::ZERO,
                    out_point: Timecode::from_seconds(2.0),
                    position: Timecode::ZERO,
                },
            })
            .unwrap_err();
        assert_eq!(err, CoreError::TrackNotFound(bogus_track));
        assert_eq!(*bus.project(), before);
        assert_eq!(bus.history().len(), history_len_before);
    }

    #[test]
    fn undo_on_empty_history_errors() {
        let mut bus = CommandBus::new(Project::new("test"));
        assert_eq!(bus.undo().unwrap_err(), CoreError::NothingToUndo);
    }

    #[test]
    fn split_clip_then_undo_restores_original_single_clip() {
        let (mut bus, track_id) = bus_with_video_track();
        bus.execute(Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "a.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(10.0),
                position: Timecode::ZERO,
            },
        })
        .unwrap();
        let clip_id = bus.project().tracks[0].clip_order[0];
        bus.execute(Command::SplitClip {
            clip_id,
            at: Timecode::from_seconds(4.0),
        })
        .unwrap();
        assert_eq!(bus.project().clips.len(), 2);
        assert_eq!(bus.project().tracks[0].clip_order.len(), 2);
        bus.undo().unwrap();
        assert_eq!(bus.project().clips.len(), 1);
        let clip = bus.project().clip(clip_id).unwrap();
        assert_eq!(clip.out_point, Timecode::from_seconds(10.0));
    }

    #[test]
    fn trim_clip_start_moves_its_timeline_edge_and_is_undoable() {
        let (mut bus, track_id) = bus_with_video_track();
        bus.execute(Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "a.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(10.0),
                position: Timecode::from_seconds(2.0),
            },
        })
        .unwrap();
        let clip_id = bus.project().tracks[0].clip_order[0];

        bus.execute(Command::TrimClipStart {
            clip_id,
            new_in: Timecode::from_seconds(3.0),
            new_position: Timecode::from_seconds(5.0),
        })
        .unwrap();
        let clip = bus.project().clip(clip_id).unwrap();
        assert_eq!(clip.in_point, Timecode::from_seconds(3.0));
        assert_eq!(clip.position, Timecode::from_seconds(5.0));

        bus.undo().unwrap();
        let clip = bus.project().clip(clip_id).unwrap();
        assert_eq!(clip.in_point, Timecode::ZERO);
        assert_eq!(clip.position, Timecode::from_seconds(2.0));
    }

    #[test]
    fn history_reports_descriptions_in_order() {
        let (mut bus, _track_id) = bus_with_video_track();
        bus.execute(Command::AddTrack {
            kind: TrackKind::Audio,
            name: "A1".into(),
        })
        .unwrap();
        assert_eq!(bus.history().len(), 2);
        assert!(bus.history()[0].contains("Video"));
        assert!(bus.history()[1].contains("Audio"));
    }
}
