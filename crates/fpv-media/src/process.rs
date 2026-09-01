//! Thin subprocess wrapper around the `ffmpeg`/`ffprobe` binaries. Kept
//! separate from the argv-building logic so that argv construction stays
//! pure and unit-testable without actually running a process.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{MediaError, MediaResult};

/// Resolve FFmpeg tools for both terminal launches and macOS Finder launches.
/// Finder supplies a deliberately minimal PATH which normally omits Homebrew's
/// `/opt/homebrew/bin`, even when ffmpeg is installed and available in a
/// developer's terminal.
fn binary_path(binary: &str) -> PathBuf {
    let variable = format!("FPV_{}_PATH", binary.to_ascii_uppercase());
    if let Some(path) = std::env::var_os(&variable).filter(|path| !path.is_empty()) {
        return path.into();
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(binary);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    #[cfg(target_os = "macos")]
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let candidate = PathBuf::from(directory).join(binary);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(binary)
}

fn command_for(binary: &str) -> Command {
    Command::new(binary_path(binary))
}

pub fn is_available(binary: &str) -> bool {
    command_for(binary)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `binary` with `args`, returning stdout as a `String` on success.
pub fn run_capture_stdout(binary: &str, args: &[String]) -> MediaResult<String> {
    let output = command_for(binary)
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
    let output = command_for(binary)
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
