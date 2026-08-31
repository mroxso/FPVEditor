//! Media probing via `ffprobe`.

use std::path::Path;

use serde::Deserialize;

use crate::error::{MediaError, MediaResult};
use crate::process;

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    /// Frames per second, as a decimal (e.g. 59.94).
    pub fps: f64,
    pub video_codec: String,
    pub has_audio: bool,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: FfprobeFormat,
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
}

/// Parse ffprobe's `-print_format json -show_format -show_streams` output.
/// Pure and dependency-free of any subprocess, so it's cheap to unit test.
pub fn parse_ffprobe_json(json: &str) -> MediaResult<MediaInfo> {
    let parsed: FfprobeOutput =
        serde_json::from_str(json).map_err(|e| MediaError::Parse(e.to_string()))?;

    let video = parsed
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .ok_or_else(|| MediaError::Parse("no video stream found".to_string()))?;

    let fps = video
        .r_frame_rate
        .as_deref()
        .and_then(parse_frame_rate)
        .unwrap_or(0.0);

    let duration_us = parsed
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .map(|secs| (secs * 1_000_000.0).round() as i64)
        .unwrap_or(0);

    Ok(MediaInfo {
        duration_us,
        width: video.width.unwrap_or(0),
        height: video.height.unwrap_or(0),
        fps,
        video_codec: video.codec_name.clone().unwrap_or_default(),
        has_audio: parsed.streams.iter().any(|s| s.codec_type == "audio"),
    })
}

/// ffprobe reports frame rate as a rational string like "60000/1001".
fn parse_frame_rate(raw: &str) -> Option<f64> {
    let (num, den) = raw.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 {
        None
    } else {
        Some(num / den)
    }
}

/// Probe a media file on disk by shelling out to `ffprobe`.
pub fn probe(path: &Path) -> MediaResult<MediaInfo> {
    if !process::is_available("ffprobe") {
        return Err(MediaError::BinaryNotFound("ffprobe".to_string()));
    }
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-print_format".to_string(),
        "json".to_string(),
        "-show_format".to_string(),
        "-show_streams".to_string(),
        path.to_string_lossy().to_string(),
    ];
    let stdout = process::run_capture_stdout("ffprobe", &args)?;
    parse_ffprobe_json(&stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSON: &str = r#"
    {
        "streams": [
            {
                "codec_type": "video",
                "codec_name": "h264",
                "width": 1920,
                "height": 1080,
                "r_frame_rate": "60000/1001"
            },
            {
                "codec_type": "audio",
                "codec_name": "aac"
            }
        ],
        "format": {
            "duration": "12.345000"
        }
    }
    "#;

    #[test]
    fn parses_video_and_audio_stream_info() {
        let info = parse_ffprobe_json(SAMPLE_JSON).unwrap();
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert!((info.fps - 59.94).abs() < 0.01);
        assert_eq!(info.video_codec, "h264");
        assert!(info.has_audio);
        assert_eq!(info.duration_us, 12_345_000);
    }

    #[test]
    fn video_only_file_reports_no_audio() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":640,"height":480,"r_frame_rate":"30/1"}],"format":{"duration":"1.0"}}"#;
        let info = parse_ffprobe_json(json).unwrap();
        assert!(!info.has_audio);
        assert_eq!(info.fps, 30.0);
    }

    #[test]
    fn missing_video_stream_is_an_error() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"aac"}],"format":{"duration":"1.0"}}"#;
        assert!(parse_ffprobe_json(json).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_ffprobe_json("not json").is_err());
    }

    #[test]
    fn probing_a_real_generated_file_reports_correct_dimensions_and_fps() {
        if !process::is_available("ffmpeg") || !process::is_available("ffprobe") {
            eprintln!("skipping: ffmpeg/ffprobe not available on PATH");
            return;
        }
        let dir = std::env::temp_dir().join(format!("fpv-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("testsrc.mp4");

        let args: Vec<String> = [
            "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=1",
            "-pix_fmt", "yuv420p", path.to_str().unwrap(),
        ]
        .into_iter()
        .map(String::from)
        .collect();
        process::run("ffmpeg", &args).expect("ffmpeg should generate a synthetic test clip");

        let info = probe(&path).unwrap();
        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert!((info.fps - 25.0).abs() < 0.1);
        assert!(info.duration_us > 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
