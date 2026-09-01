use std::path::Path;

use crate::error::{CoreError, CoreResult};
use crate::model::{Project, PROJECT_FORMAT_VERSION};

/// Serialize a project to pretty-printed, human-diffable JSON.
pub fn to_json(project: &Project) -> CoreResult<String> {
    serde_json::to_string_pretty(project).map_err(|e| CoreError::Serde(e.to_string()))
}

pub fn from_json(data: &str) -> CoreResult<Project> {
    let project: Project =
        serde_json::from_str(data).map_err(|e| CoreError::Serde(e.to_string()))?;
    if project.format_version > PROJECT_FORMAT_VERSION {
        return Err(CoreError::Serde(format!(
            "project format version {} is newer than supported version {}",
            project.format_version, PROJECT_FORMAT_VERSION
        )));
    }
    Ok(project)
}

pub fn save(project: &Project, path: &Path) -> CoreResult<()> {
    let json = to_json(project)?;
    std::fs::write(path, json).map_err(|e| CoreError::Serde(e.to_string()))
}

pub fn load(path: &Path) -> CoreResult<Project> {
    let data = std::fs::read_to_string(path).map_err(|e| CoreError::Serde(e.to_string()))?;
    from_json(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, NewClip};
    use crate::bus::CommandBus;
    use crate::model::{Timecode, TrackKind};

    #[test]
    fn round_trips_a_project_with_clips_through_json() {
        let mut bus = CommandBus::new(Project::new("Roundtrip"));
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        let track_id = bus.project().tracks[0].id;
        bus.execute(Command::AddClip {
            track_id,
            clip: NewClip {
                source_path: "clip.mp4".into(),
                in_point: Timecode::ZERO,
                out_point: Timecode::from_seconds(3.5),
                position: Timecode::ZERO,
            },
        })
        .unwrap();

        let json = to_json(bus.project()).unwrap();
        let reloaded = from_json(&json).unwrap();
        assert_eq!(&reloaded, bus.project());
    }

    #[test]
    fn save_then_load_round_trips_via_filesystem() {
        let dir = std::env::temp_dir().join(format!("fpv-core-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.fpv.json");

        let project = Project::new("On disk");
        save(&project, &path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, project);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_project_file_from_a_newer_format_version() {
        let mut project = Project::new("future");
        project.format_version = PROJECT_FORMAT_VERSION + 1;
        let json = serde_json::to_string(&project).unwrap();
        let err = from_json(&json).unwrap_err();
        assert!(matches!(err, CoreError::Serde(_)));
    }
}
