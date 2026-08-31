use std::path::Path;

use anyhow::{Context, Result};
use fpv_core::{ClipId, CommandBus, Project};

/// Load a project, run `f` against its command bus, save the result back to
/// disk, and print the updated project as JSON. This is the shared shape of
/// every mutating subcommand: one edit per CLI invocation.
pub fn with_bus(path: &Path, f: impl FnOnce(&mut CommandBus) -> Result<()>) -> Result<()> {
    let project = fpv_core::project_file::load(path)
        .with_context(|| format!("failed to load project at {}", path.display()))?;
    let mut bus = CommandBus::new(project);
    f(&mut bus)?;
    fpv_core::project_file::save(bus.project(), path)
        .with_context(|| format!("failed to save project at {}", path.display()))?;
    println!("{}", serde_json::to_string_pretty(bus.project())?);
    Ok(())
}

pub fn cmd_new(path: &Path, name: &str) -> Result<()> {
    if path.exists() {
        anyhow::bail!("{} already exists; refusing to overwrite", path.display());
    }
    let project = Project::new(name);
    fpv_core::project_file::save(&project, path)
        .with_context(|| format!("failed to write new project to {}", path.display()))?;
    println!("{}", serde_json::to_string_pretty(&project)?);
    Ok(())
}

pub fn cmd_list(path: &Path) -> Result<()> {
    let project = fpv_core::project_file::load(path)
        .with_context(|| format!("failed to load project at {}", path.display()))?;
    let clips: Vec<_> = project.clips.values().collect();
    println!("{}", serde_json::to_string_pretty(&clips)?);
    Ok(())
}

pub fn cmd_show(path: &Path) -> Result<()> {
    let project = fpv_core::project_file::load(path)
        .with_context(|| format!("failed to load project at {}", path.display()))?;
    println!("{}", serde_json::to_string_pretty(&project)?);
    Ok(())
}

pub fn cmd_probe(source: &Path) -> Result<()> {
    let info = fpv_media::probe(source).context("ffprobe failed")?;
    println!(
        "{}",
        serde_json::json!({
            "duration_us": info.duration_us,
            "width": info.width,
            "height": info.height,
            "fps": info.fps,
            "video_codec": info.video_codec,
            "has_audio": info.has_audio,
        })
    );
    Ok(())
}

pub fn cmd_export(
    project_path: &Path,
    clip_id: &ClipId,
    output: &Path,
    width: u32,
    height: u32,
    fps: f64,
) -> Result<()> {
    let project = fpv_core::project_file::load(project_path)
        .with_context(|| format!("failed to load project at {}", project_path.display()))?;
    let clip = project
        .clip(*clip_id)
        .with_context(|| format!("clip {clip_id} not found in project"))?;
    let settings = fpv_media::ExportSettings {
        output_path: output.to_path_buf(),
        width,
        height,
        fps,
        crf: None,
    };
    fpv_media::export_clip(clip, &settings).context("ffmpeg export failed")?;
    println!(
        "{}",
        serde_json::json!({ "exported": output.to_string_lossy() })
    );
    Ok(())
}

pub async fn cmd_mcp_serve(path: &Path) -> Result<()> {
    let project = if path.exists() {
        fpv_core::project_file::load(path)
            .with_context(|| format!("failed to load project at {}", path.display()))?
    } else {
        Project::new("Untitled")
    };
    let bus = CommandBus::new(project);
    eprintln!("fpv-mcp: serving over stdio for project {}", path.display());
    let final_project = fpv_mcp::serve_stdio(bus).await?;
    fpv_core::project_file::save(&final_project, path)
        .with_context(|| format!("failed to save project at {}", path.display()))?;
    eprintln!("fpv-mcp: session ended, project saved to {}", path.display());
    Ok(())
}
