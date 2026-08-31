//! Thin subprocess wrapper around the `ffmpeg`/`ffprobe` binaries. Kept
//! separate from the argv-building logic so that argv construction stays
//! pure and unit-testable without actually running a process.

use std::process::Command;

use crate::error::{MediaError, MediaResult};

pub fn is_available(binary: &str) -> bool {
    Command::new(binary)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `binary` with `args`, returning stdout as a `String` on success.
pub fn run_capture_stdout(binary: &str, args: &[String]) -> MediaResult<String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|source| MediaError::Spawn {
            binary: binary.to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(MediaError::NonZeroExit {
            binary: binary.to_string(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    String::from_utf8(output.stdout).map_err(|e| MediaError::Parse(e.to_string()))
}

/// Run `binary` with `args`, discarding stdout, surfacing stderr on failure.
pub fn run(binary: &str, args: &[String]) -> MediaResult<()> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|source| MediaError::Spawn {
            binary: binary.to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(MediaError::NonZeroExit {
            binary: binary.to_string(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(())
}
