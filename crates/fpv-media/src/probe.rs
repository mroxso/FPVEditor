//! Media probing via `ffprobe`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{MediaError, MediaResult};
use crate::process;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaInfo {
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    /// Frames per second, as a decimal (e.g. 59.94).
    pub fps: f64,
    pub video_codec: String,
    pub has_audio: bool,
}

/// A downsampled gyroscope trace suitable for a lightweight timeline display.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GyroTrace {
    /// One [X, Y, Z] angular-velocity sample per point.
    pub samples: Vec<[f32; 3]>,
}

#[derive(Debug, Deserialize)]
struct FfprobePackets {
    packets: Vec<FfprobePacket>,
}

#[derive(Debug, Deserialize)]
struct FfprobePacket {
    data: Option<String>,
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

    // A missing field is a legitimate corner case for some containers, but a
    // *present-but-unparseable* value (e.g. ffprobe's "N/A") means something
    // is wrong with the source, so that case is a hard error rather than a
    // silent 0 that downstream code could mistake for a genuinely zero-length
    // or zero-fps file.
    let fps = match video.r_frame_rate.as_deref() {
        Some(raw) => parse_frame_rate(raw)
            .ok_or_else(|| MediaError::Parse(format!("unparseable frame rate: {raw:?}")))?,
        None => 0.0,
    };

    let duration_us = match parsed.format.duration.as_deref() {
        Some(raw) => raw
            .parse::<f64>()
            .map(|secs| (secs * 1_000_000.0).round() as i64)
            .map_err(|_| MediaError::Parse(format!("unparseable duration: {raw:?}")))?,
        None => 0,
    };

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

/// Read GoPro's GPMF `GYRO` metadata from the file's data track when present.
///
/// FFmpeg exposes the raw GPMF packets but does not decode them itself. The
/// parser below extracts the nested `GYRO` samples and their `SCAL` factor,
/// then limits the result to a practical number of timeline points. Formats
/// without GPMF are deliberately reported as `None` rather than as an error.
pub fn probe_gyro_trace(path: &Path) -> MediaResult<Option<GyroTrace>> {
    if !process::is_available("ffprobe") {
        return Err(MediaError::BinaryNotFound("ffprobe".to_string()));
    }
    let args = vec![
        "-v".into(),
        "error".into(),
        "-select_streams".into(),
        "d".into(),
        "-show_packets".into(),
        "-show_data".into(),
        "-show_entries".into(),
        "packet=data".into(),
        "-of".into(),
        "json".into(),
        path.to_string_lossy().to_string(),
    ];
    let stdout = process::run_capture_stdout("ffprobe", &args)?;
    Ok(parse_gyro_packets_json(&stdout))
}

fn parse_gyro_packets_json(json: &str) -> Option<GyroTrace> {
    let packets: FfprobePackets = serde_json::from_str(json).ok()?;
    let mut samples = Vec::new();
    for packet in packets.packets {
        let bytes = decode_ffprobe_hex(packet.data.as_deref()?);
        extract_gpmf_gyro(&bytes, 1.0, &mut samples);
    }
    if samples.is_empty() {
        return None;
    }
    // Keeping this bounded makes a large action-camera file cheap to send to
    // the UI while preserving its overall movement profile.
    const MAX_POINTS: usize = 160;
    let stride = samples.len().div_ceil(MAX_POINTS);
    Some(GyroTrace {
        samples: samples.into_iter().step_by(stride).collect(),
    })
}

fn decode_ffprobe_hex(data: &str) -> Vec<u8> {
    data.lines()
        .flat_map(|line| {
            line.split_once(':')
                .map(|(_, hex)| hex)
                .unwrap_or("")
                .split_whitespace()
                .filter(|word| word.len() == 4 && word.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .flat_map(|word| {
                    [
                        u8::from_str_radix(&word[..2], 16),
                        u8::from_str_radix(&word[2..], 16),
                    ]
                })
                .flatten()
        })
        .collect()
}

fn extract_gpmf_gyro(bytes: &[u8], inherited_scale: f32, output: &mut Vec<[f32; 3]>) {
    let mut offset = 0;
    let mut scale = inherited_scale;
    while offset + 8 <= bytes.len() {
        let key = &bytes[offset..offset + 4];
        let kind = bytes[offset + 4];
        let size = bytes[offset + 5] as usize;
        let count = u16::from_be_bytes([bytes[offset + 6], bytes[offset + 7]]) as usize;
        let data_len = match size.checked_mul(count) {
            Some(value) => value,
            None => return,
        };
        let data_start = offset + 8;
        let data_end = match data_start.checked_add(data_len) {
            Some(value) if value <= bytes.len() => value,
            _ => return,
        };
        let data = &bytes[data_start..data_end];
        if key == b"SCAL" && data.len() >= 4 {
            let raw = i32::from_be_bytes(data[..4].try_into().expect("four bytes"));
            if raw != 0 {
                scale = raw as f32;
            }
        } else if key == b"GYRO" && kind == b's' && size >= 6 {
            for chunk in data.chunks_exact(size) {
                output.push([
                    i16::from_be_bytes([chunk[0], chunk[1]]) as f32 / scale,
                    i16::from_be_bytes([chunk[2], chunk[3]]) as f32 / scale,
                    i16::from_be_bytes([chunk[4], chunk[5]]) as f32 / scale,
                ]);
            }
        } else if kind == 0 || key == b"DEVC" || key == b"STRM" {
            extract_gpmf_gyro(data, scale, output);
        }
        offset = data_end + ((4 - data_len % 4) % 4);
    }
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
    fn non_numeric_duration_is_an_error_not_a_silent_zero() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":640,"height":480,"r_frame_rate":"30/1"}],"format":{"duration":"N/A"}}"#;
        assert!(parse_ffprobe_json(json).is_err());
    }

    #[test]
    fn non_numeric_frame_rate_is_an_error_not_a_silent_zero() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":640,"height":480,"r_frame_rate":"0/0"}],"format":{"duration":"1.0"}}"#;
        assert!(parse_ffprobe_json(json).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_ffprobe_json("not json").is_err());
    }

    #[test]
    fn parses_nested_gpmf_gyro_samples_from_ffprobe_packets() {
        // DEVC -> SCAL (100) + GYRO (two signed-short XYZ samples).
        let hex =
            "4445 5643 0000 0020 5343 414c 6c04 0001 0000 0064 4759 524f 7306 0002 000a 0014 ffe2 001e ffd8 0028";
        let json = format!(r#"{{"packets":[{{"data":"\n00000000: {hex}"}}]}}"#);
        let trace = parse_gyro_packets_json(&json).unwrap();
        assert_eq!(trace.samples.len(), 2);
        assert_eq!(trace.samples[0], [0.1, 0.2, -0.3]);
        assert_eq!(trace.samples[1], [0.3, -0.4, 0.4]);
    }

    #[test]
    fn gyro_packets_without_gyro_data_are_ignored() {
        assert!(parse_gyro_packets_json(r#"{"packets":[]}"#).is_none());
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
