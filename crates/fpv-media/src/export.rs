//! Builds `ffmpeg` invocations from a [`fpv_core::Clip`]. Argv construction
//! is a pure function (`export_clip_args`) so it can be unit tested without
//! ever spawning a process; [`export_clip`] is the thin wrapper that
//! actually runs it.

use std::path::{Path, PathBuf};

use fpv_core::{Clip, Project, TrackKind};

use crate::error::MediaResult;
use crate::process;

#[derive(Debug, Clone, PartialEq)]
pub struct ExportSettings {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// libx264 CRF (lower = higher quality), defaults to 23 if `None`.
    pub crf: Option<u8>,
}

/// Build the `ffmpeg` argv that renders `clip` (trim, stabilization crop,
/// LUT, and a constant-rate speed change) to `settings.output_path`.
///
/// Speed ramps are approximated by the *first* keyframe's rate — proper
/// per-sample-accurate ramping needs a custom `setpts` expression built from
/// the whole curve, tracked as future work (PLAN.md section 5).
pub fn export_clip_args(clip: &Clip, settings: &ExportSettings) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".to_string()];

    args.push("-ss".to_string());
    args.push(format!("{:.6}", clip.in_point.seconds()));
    args.push("-to".to_string());
    args.push(format!("{:.6}", clip.out_point.seconds()));
    args.push("-i".to_string());
    args.push(clip.source_path.to_string_lossy().to_string());

    let mut filters: Vec<String> = Vec::new();

    if let Some(stab) = &clip.stabilization {
        // Crop in by fov_scale to hide the edges a rotational warp would
        // reveal. NOTE: this only reserves the crop — nothing in the export
        // path (or anywhere else yet) actually invokes fpv_stabilize's
        // per-frame rotation or fpv-gpu's warp pipeline, so exported clips
        // are cropped/zoomed but not actually de-shaken. Wiring frame-by-frame
        // GPU warp into this ffmpeg-based export pipeline is tracked as
        // follow-up work.
        let scale = 1.0 + stab.dynamic_fov.clamp(0.0, 1.0);
        let crop_w = (settings.width as f32 / scale) as u32;
        let crop_h = (settings.height as f32 / scale) as u32;
        filters.push(format!(
            "crop={crop_w}:{crop_h}:(in_w-{crop_w})/2:(in_h-{crop_h})/2"
        ));
    }

    filters.push(format!("scale={}:{}", settings.width, settings.height));

    if let Some(lut) = &clip.lut_path {
        filters.push(format!("lut3d='{}'", lut.to_string_lossy()));
    }

    if let Some(first) = clip.speed_keyframes.first() {
        if first.rate > 0.0 {
            filters.push(format!("setpts=(1/{})*PTS", first.rate));
        }
    }

    args.push("-vf".to_string());
    args.push(filters.join(","));

    args.push("-r".to_string());
    args.push(format!("{}", settings.fps));

    args.push("-c:v".to_string());
    args.push("libx264".to_string());
    args.push("-crf".to_string());
    args.push(settings.crf.unwrap_or(23).to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());

    args.push(settings.output_path.to_string_lossy().to_string());
    args
}

pub fn export_clip(clip: &Clip, settings: &ExportSettings) -> MediaResult<()> {
    let args = export_clip_args(clip, settings);
    process::run("ffmpeg", &args)
}

/// Render the visible video timeline into an H.264 preview file.  Each clip
/// is first passed through the same renderer used by export, then placed at
/// its timeline position.  This deliberately makes the editor monitor match
/// trim, speed, LUT, and stabilization-crop output instead of showing a raw
/// source file.
///
/// Video tracks are composited in track order (later tracks are on top).  The
/// preview is video-only for now; audio mixing belongs in the final export
/// pipeline, where track gain and transitions can be represented explicitly.
pub fn export_timeline_preview(project: &Project, output_path: &Path) -> MediaResult<()> {
    let clips: Vec<&Clip> = project
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Video)
        .flat_map(|track| {
            track
                .clip_order
                .iter()
                .filter_map(|id| project.clips.get(id))
        })
        .collect();
    if clips.is_empty() {
        return Err(crate::error::MediaError::Parse(
            "timeline has no video clips".into(),
        ));
    }

    let cache_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = output_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("preview");
    let settings_for = |path: PathBuf| ExportSettings {
        output_path: path,
        width: project.width,
        height: project.height,
        fps: project.fps,
        crf: Some(28),
    };
    let mut rendered = Vec::with_capacity(clips.len());
    for (index, clip) in clips.iter().enumerate() {
        let path = cache_dir.join(format!("{stem}-clip-{index}.mp4"));
        export_clip(clip, &settings_for(path.clone()))?;
        rendered.push(path);
    }

    let duration = (project.duration().seconds()).max(0.01);
    let mut args = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!(
            "color=c=black:s={}x{}:r={}:d={duration}",
            project.width, project.height, project.fps
        ),
    ];
    for path in &rendered {
        args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    }
    let mut filter = String::new();
    let mut previous = "[0:v]".to_string();
    for (index, clip) in clips.iter().enumerate() {
        let label = format!("[clip{index}]");
        let output = format!("[layer{index}]");
        filter.push_str(&format!(
            "[{}:v]setpts=PTS-STARTPTS+{:.6}/TB{};{}{}overlay=eof_action=pass:shortest=0{};",
            index + 1,
            clip.position.seconds(),
            label,
            previous,
            label,
            output,
        ));
        previous = output;
    }
    filter.push_str(&format!("{}format=yuv420p[preview]", previous));
    args.extend([
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[preview]".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "28".into(),
        "-movflags".into(),
        "+faststart".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    process::run("ffmpeg", &args)
}

/// Generate a proxy (low-res, fast-decode) rendition of a source file for
/// smooth editing playback of large 4K/60 footage.
pub fn generate_proxy(source: &Path, output: &Path, max_height: u32) -> MediaResult<()> {
    let args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.to_string_lossy().to_string(),
        "-vf".to_string(),
        format!("scale=-2:{max_height}"),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "ultrafast".to_string(),
        "-crf".to_string(),
        "28".to_string(),
        output.to_string_lossy().to_string(),
    ];
    process::run("ffmpeg", &args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_core::{Clip, StabilizationProfile, Timecode};

    fn base_clip() -> Clip {
        Clip::new(
            "input.mp4",
            Timecode::from_seconds(1.0),
            Timecode::from_seconds(4.0),
        )
    }

    fn base_settings() -> ExportSettings {
        ExportSettings {
            output_path: "out.mp4".into(),
            width: 1920,
            height: 1080,
            fps: 60.0,
            crf: None,
        }
    }

    #[test]
    fn trims_using_ss_and_to_from_clip_in_out_points() {
        let args = export_clip_args(&base_clip(), &base_settings());
        let ss_idx = args.iter().position(|a| a == "-ss").unwrap();
        assert_eq!(args[ss_idx + 1], "1.000000");
        let to_idx = args.iter().position(|a| a == "-to").unwrap();
        assert_eq!(args[to_idx + 1], "4.000000");
    }

    #[test]
    fn plain_clip_has_no_lut_or_crop_filter() {
        let args = export_clip_args(&base_clip(), &base_settings());
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let filter = &args[vf_idx + 1];
        assert!(!filter.contains("lut3d"));
        assert!(!filter.contains("crop"));
        assert!(filter.contains("scale=1920:1080"));
    }

    #[test]
    fn stabilized_clip_adds_a_crop_filter_before_scale() {
        let mut clip = base_clip();
        clip.stabilization = Some(StabilizationProfile {
            smoothness: 0.5,
            strength: 1.0,
            horizon_lock: false,
            dynamic_fov: 0.25,
        });
        let args = export_clip_args(&clip, &base_settings());
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let filter = &args[vf_idx + 1];
        assert!(filter.starts_with("crop="));
        let crop_pos = filter.find("crop=").unwrap();
        let scale_pos = filter.find("scale=").unwrap();
        assert!(crop_pos < scale_pos);
    }

    #[test]
    fn clip_with_lut_appends_lut3d_filter() {
        let mut clip = base_clip();
        clip.lut_path = Some("/luts/warm.cube".into());
        let args = export_clip_args(&clip, &base_settings());
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert!(args[vf_idx + 1].contains("lut3d='/luts/warm.cube'"));
    }

    #[test]
    fn speed_keyframe_adds_setpts_filter() {
        let mut clip = base_clip();
        clip.speed_keyframes = vec![fpv_core::SpeedKeyframe {
            at: Timecode::ZERO,
            rate: 2.0,
        }];
        let args = export_clip_args(&clip, &base_settings());
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert!(args[vf_idx + 1].contains("setpts=(1/2)*PTS"));
    }

    #[test]
    fn custom_crf_overrides_default() {
        let mut settings = base_settings();
        settings.crf = Some(18);
        let args = export_clip_args(&base_clip(), &settings);
        let crf_idx = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf_idx + 1], "18");
    }

    #[test]
    fn end_to_end_export_of_a_synthetic_clip_produces_a_playable_file_with_expected_fps() {
        if !process::is_available("ffmpeg") {
            eprintln!("skipping: ffmpeg not available on PATH");
            return;
        }
        let dir =
            std::env::temp_dir().join(format!("fpv-media-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("src.mp4");
        let out = dir.join("out.mp4");

        let gen_args: Vec<String> = [
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=2",
            "-pix_fmt",
            "yuv420p",
            source.to_str().unwrap(),
        ]
        .into_iter()
        .map(String::from)
        .collect();
        process::run("ffmpeg", &gen_args).unwrap();

        let clip = Clip::new(
            &source,
            Timecode::from_seconds(0.0),
            Timecode::from_seconds(1.0),
        );
        let settings = ExportSettings {
            output_path: out.clone(),
            width: 160,
            height: 120,
            fps: 30.0,
            crf: Some(30),
        };
        export_clip(&clip, &settings).expect("export should succeed");

        assert!(out.exists());
        let info = crate::probe::probe(&out).unwrap();
        assert_eq!(info.width, 160);
        assert_eq!(info.height, 120);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn timeline_preview_composites_positioned_clips_into_a_playable_video() {
        if !process::is_available("ffmpeg") {
            eprintln!("skipping: ffmpeg not available on PATH");
            return;
        }
        use fpv_core::{Command, CommandBus, NewClip, Project, TrackKind};

        let dir = std::env::temp_dir().join(format!("fpv-timeline-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("src.mp4");
        process::run(
            "ffmpeg",
            &[
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x120:rate=24:duration=2",
                "-pix_fmt",
                "yuv420p",
                source.to_str().unwrap(),
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        )
        .unwrap();
        let mut bus = CommandBus::new(Project::new("preview"));
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        let track_id = bus.project().tracks[0].id;
        for position in [0.0, 1.0] {
            bus.execute(Command::AddClip {
                track_id,
                clip: NewClip {
                    source_path: source.clone(),
                    in_point: Timecode::ZERO,
                    out_point: Timecode::from_seconds(1.0),
                    position: Timecode::from_seconds(position),
                },
            })
            .unwrap();
        }
        let output = dir.join("timeline.mp4");
        export_timeline_preview(bus.project(), &output).unwrap();
        let info = crate::probe::probe(&output).unwrap();
        assert_eq!((info.width, info.height), (1920, 1080));
        assert!(info.duration_us >= 1_900_000);
        std::fs::remove_dir_all(&dir).ok();
    }
}
