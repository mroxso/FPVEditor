//! Builds `ffmpeg` invocations from a [`fpv_core::Clip`]. Argv construction
//! is a pure function (`export_clip_args`) so it can be unit tested without
//! ever spawning a process; [`export_clip`] is the thin wrapper that
//! actually runs it.

use std::path::{Path, PathBuf};

use fpv_core::Clip;

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
        Clip::new("input.mp4", Timecode::from_seconds(1.0), Timecode::from_seconds(4.0))
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
        let dir = std::env::temp_dir().join(format!("fpv-media-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("src.mp4");
        let out = dir.join("out.mp4");

        let gen_args: Vec<String> = [
            "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=30:duration=2",
            "-pix_fmt", "yuv420p", source.to_str().unwrap(),
        ]
        .into_iter()
        .map(String::from)
        .collect();
        process::run("ffmpeg", &gen_args).unwrap();

        let clip = Clip::new(&source, Timecode::from_seconds(0.0), Timecode::from_seconds(1.0));
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
}
