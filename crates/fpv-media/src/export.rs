//! Builds `ffmpeg` invocations from a [`fpv_core::Clip`]. Argv construction
//! is a pure function (`export_clip_args`) so it can be unit tested without
//! ever spawning a process; [`export_clip`] is the thin wrapper that
//! actually runs it.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use fpv_core::{Clip, Project, Timecode, TrackKind};
use serde::{Deserialize, Serialize};

use crate::error::MediaResult;
use crate::process;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportContainer {
    Mp4,
    Mov,
    Webm,
}

impl ExportContainer {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
}

impl VideoCodec {
    fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::H264 => "libx264",
            Self::H265 => "libx265",
            Self::Vp9 => "libvpx-vp9",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    Opus,
}

impl AudioCodec {
    fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Opus => "libopus",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Quality value appropriate for the selected codec (lower = better).
    pub crf: Option<u8>,
    pub container: ExportContainer,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
}

/// The small, tested set of containers/codecs exposed by the app which this
/// local FFmpeg installation can actually encode and mux.
#[derive(Debug, Clone, Serialize)]
pub struct ExportCapabilities {
    pub containers: Vec<ExportContainer>,
    pub video_codecs: Vec<VideoCodec>,
    pub audio_codecs: Vec<AudioCodec>,
}

pub fn export_capabilities() -> MediaResult<ExportCapabilities> {
    let encoders =
        crate::process::run_capture_stdout("ffmpeg", &["-hide_banner".into(), "-encoders".into()])?;
    let muxers =
        crate::process::run_capture_stdout("ffmpeg", &["-hide_banner".into(), "-muxers".into()])?;
    let has = |text: &str, name: &str| {
        text.lines()
            .any(|line| line.split_whitespace().any(|word| word == name))
    };
    Ok(ExportCapabilities {
        containers: [
            ExportContainer::Mp4,
            ExportContainer::Mov,
            ExportContainer::Webm,
        ]
        .into_iter()
        .filter(|container| has(&muxers, container.extension()))
        .collect(),
        video_codecs: [VideoCodec::H264, VideoCodec::H265, VideoCodec::Vp9]
            .into_iter()
            .filter(|codec| has(&encoders, codec.ffmpeg_name()))
            .collect(),
        audio_codecs: [AudioCodec::Aac, AudioCodec::Opus]
            .into_iter()
            .filter(|codec| has(&encoders, codec.ffmpeg_name()))
            .collect(),
    })
}

impl ExportSettings {
    pub fn validate(&self) -> MediaResult<()> {
        if self.width == 0
            || self.height == 0
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            return Err(crate::error::MediaError::Parse(
                "resolution must use positive, even dimensions".into(),
            ));
        }
        if !self.fps.is_finite() || self.fps <= 0.0 {
            return Err(crate::error::MediaError::Parse(
                "frame rate must be greater than zero".into(),
            ));
        }
        if matches!(self.container, ExportContainer::Webm)
            && !matches!(self.video_codec, VideoCodec::Vp9)
        {
            return Err(crate::error::MediaError::Parse(
                "WebM requires the VP9 video codec in FPV Editor".into(),
            ));
        }
        if matches!(self.container, ExportContainer::Webm)
            && !matches!(self.audio_codec, AudioCodec::Opus)
        {
            return Err(crate::error::MediaError::Parse(
                "WebM requires the Opus audio codec in FPV Editor".into(),
            ));
        }
        Ok(())
    }
}

/// Monitor renders should become interactive quickly and must not compete with
/// final export for every available CPU core.  Keep the source aspect ratio,
/// but cap the long edge at a practical editing-preview size.
pub fn preview_dimensions(width: u32, height: u32) -> (u32, u32) {
    const MAX_WIDTH: u32 = 960;
    if width <= MAX_WIDTH {
        return (width, height);
    }
    let scaled_height = ((height as f64 * MAX_WIDTH as f64 / width as f64).round() as u32 / 2) * 2;
    (MAX_WIDTH, scaled_height.max(2))
}

/// Build the `ffmpeg` argv that renders `clip` (trim, stabilization crop,
/// LUT, and a constant-rate speed change) to `settings.output_path`.
///
/// Speed ramps are approximated by the *first* keyframe's rate — proper
/// per-sample-accurate ramping needs a custom `setpts` expression built from
/// the whole curve, tracked as future work (PLAN.md section 5).
pub fn export_clip_args(clip: &Clip, settings: &ExportSettings) -> Vec<String> {
    settings
        .validate()
        .expect("export settings must be validated before building arguments");
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
    args.push(settings.video_codec.ffmpeg_name().to_string());
    args.push("-crf".to_string());
    args.push(settings.crf.unwrap_or(23).to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.extend([
        "-c:a".into(),
        settings.audio_codec.ffmpeg_name().into(),
        "-movflags".into(),
        "+faststart".into(),
    ]);

    args.push(settings.output_path.to_string_lossy().to_string());
    args
}

pub fn export_clip(clip: &Clip, settings: &ExportSettings) -> MediaResult<()> {
    settings.validate()?;
    let args = export_clip_args(clip, settings);
    process::run("ffmpeg", &args)
}

/// Render all video clips into their timeline positions. Audio from timeline
/// clips is delayed to the same position and mixed when sources provide it.
pub fn export_timeline(project: &Project, settings: &ExportSettings) -> MediaResult<()> {
    export_timeline_with_progress(project, settings, |_| {})
}

pub fn export_timeline_with_progress(
    project: &Project,
    settings: &ExportSettings,
    on_progress: impl FnMut(u64),
) -> MediaResult<()> {
    export_timeline_with_progress_and_cancel(
        project,
        settings,
        Arc::new(AtomicBool::new(false)),
        on_progress,
    )
}

pub fn export_timeline_with_progress_and_cancel(
    project: &Project,
    settings: &ExportSettings,
    cancelled: Arc<AtomicBool>,
    on_progress: impl FnMut(u64),
) -> MediaResult<()> {
    settings.validate()?;
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
            "timeline has no video clips to export".into(),
        ));
    }
    let duration = project.duration().seconds().max(0.01);
    let mut args = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!(
            "color=c=black:s={}x{}:r={}:d={duration}",
            settings.width, settings.height, settings.fps
        ),
    ];
    for clip in &clips {
        args.extend([
            "-ss".into(),
            format!("{:.6}", clip.in_point.seconds()),
            "-to".into(),
            format!("{:.6}", clip.out_point.seconds()),
            "-i".into(),
            clip.source_path.to_string_lossy().into_owned(),
        ]);
    }
    let mut filter = String::new();
    let mut previous = "[0:v]".to_string();
    let mut audio_labels = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        let input = index + 1;
        let video = format!("[v{index}]");
        let output = format!("[layer{index}]");
        let mut video_filters = format!(
            "[{input}:v]setpts=PTS-STARTPTS,scale={}:{}",
            settings.width, settings.height
        );
        if let Some(lut) = &clip.lut_path {
            video_filters.push_str(&format!(",lut3d='{}'", lut.to_string_lossy()));
        }
        // Offset each source before overlaying it. The earlier implementation
        // enabled the overlay at its timeline position but left its frames at
        // time zero, so clips on later tracks had already reached EOF.
        video_filters = format!(
            "[{input}:v]setpts=PTS-STARTPTS+{:.6}/TB,scale={}:{}",
            clip.position.seconds(),
            settings.width,
            settings.height
        );
        if let Some(lut) = &clip.lut_path {
            video_filters.push_str(&format!(",lut3d='{}'", lut.to_string_lossy()));
        }
        video_filters.push_str(&format!(
            "{video};{previous}{video}overlay=eof_action=pass:shortest=0:x=0:y=0{output};"
        ));
        filter.push_str(&video_filters);
        previous = output;
        if crate::probe::probe(&clip.source_path)
            .map(|info| info.has_audio)
            .unwrap_or(false)
        {
            let audio = format!("[a{index}]");
            filter.push_str(&format!(
                "[{input}:a]asetpts=PTS-STARTPTS,adelay={}:all=1{audio};",
                (clip.position.seconds() * 1000.0).round()
            ));
            audio_labels.push(audio);
        }
    }
    filter.push_str(&format!("{previous}format=yuv420p[video];"));
    if audio_labels.len() == 1 {
        filter.push_str(&format!("{}anull[audio]", audio_labels[0]));
    } else if !audio_labels.is_empty() {
        filter.push_str(&format!(
            "{}amix=inputs={}:duration=longest[audio]",
            audio_labels.join(""),
            audio_labels.len()
        ));
    }
    args.extend([
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[video]".into(),
        "-r".into(),
        settings.fps.to_string(),
        "-c:v".into(),
        settings.video_codec.ffmpeg_name().into(),
        "-crf".into(),
        settings.crf.unwrap_or(23).to_string(),
        "-c:a".into(),
        settings.audio_codec.ffmpeg_name().into(),
    ]);
    if !audio_labels.is_empty() {
        args.extend(["-map".into(), "[audio]".into()]);
    }
    if !matches!(settings.container, ExportContainer::Webm) {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
    args.push(settings.output_path.to_string_lossy().into_owned());
    process::run_with_progress_and_cancel("ffmpeg", &args, cancelled, on_progress)
}

/// Fast, resource-bounded rendition for the editor monitor.  Final exports
/// deliberately keep their normal encoder settings.
pub fn export_clip_preview(clip: &Clip, settings: &ExportSettings) -> MediaResult<()> {
    let mut args = export_clip_args(clip, settings);
    let output = args
        .pop()
        .expect("export arguments always include an output path");
    args.extend([
        "-preset".to_string(),
        "ultrafast".to_string(),
        "-threads".to_string(),
        "2".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        output,
    ]);
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
    export_timeline_preview_range(project, output_path, Timecode::ZERO, project.duration())
}

/// Render only a timeline window for the editor monitor. This keeps preview
/// work proportional to what the user is about to watch instead of rendering
/// every source clip in a long project before playback can begin.
pub fn export_timeline_preview_range(
    project: &Project,
    output_path: &Path,
    start: Timecode,
    duration: Timecode,
) -> MediaResult<()> {
    let (width, height) = preview_dimensions(project.width, project.height);
    let end = Timecode(start.0.saturating_add(duration.0.max(1)));
    let clips: Vec<Clip> = project
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Video)
        .flat_map(|track| {
            track
                .clip_order
                .iter()
                .filter_map(|id| project.clips.get(id))
                .filter_map(|clip| {
                    let clip_start = clip.position;
                    let clip_end = clip.position + clip.source_duration();
                    let visible_start = clip_start.max(start);
                    let visible_end = clip_end.min(end);
                    if visible_start >= visible_end {
                        return None;
                    }
                    let mut visible = clip.clone();
                    visible.in_point = clip.in_point + (visible_start - clip_start);
                    visible.out_point = clip.in_point + (visible_end - clip_start);
                    visible.position = visible_start - start;
                    Some(visible)
                })
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
        width,
        height,
        fps: project.fps,
        crf: Some(28),
        container: ExportContainer::Mp4,
        video_codec: VideoCodec::H264,
        audio_codec: AudioCodec::Aac,
    };
    let mut rendered = Vec::with_capacity(clips.len());
    for (index, clip) in clips.iter().enumerate() {
        let path = cache_dir.join(format!("{stem}-clip-{index}.mp4"));
        export_clip_preview(clip, &settings_for(path.clone()))?;
        rendered.push(path);
    }

    let duration = duration.seconds().max(0.01);
    let mut args = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!(
            "color=c=black:s={}x{}:r={}:d={duration}",
            width, height, project.fps
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
        "-threads".into(),
        "2".into(),
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
            container: ExportContainer::Mp4,
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
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
            container: ExportContainer::Mp4,
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
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
        assert_eq!((info.width, info.height), (960, 540));
        assert!(info.duration_us >= 1_900_000);

        let window_output = dir.join("timeline-window.mp4");
        export_timeline_preview_range(
            bus.project(),
            &window_output,
            Timecode::from_seconds(0.5),
            Timecode::from_seconds(0.75),
        )
        .unwrap();
        let window_info = crate::probe::probe(&window_output).unwrap();
        assert!(
            (700_000..=850_000).contains(&window_info.duration_us),
            "expected a short monitor window, got {}us",
            window_info.duration_us
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
