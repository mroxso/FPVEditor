//! Thin subprocess wrapper around the `ffmpeg`/`ffprobe` binaries. Kept
//! separate from the argv-building logic so that argv construction stays
//! pure and unit-testable without actually running a process.

use std::io::ErrorKind;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::error::{MediaError, MediaResult};
use serde::Serialize;

/// The outcome of checking one of the external FFmpeg tools.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDiagnostic {
    /// The name passed to the process runner (for example, `ffmpeg`).
    pub binary: String,
    /// The executable selected by FPV Editor.
    pub path: PathBuf,
    /// How the executable was selected: an override, PATH, or a macOS fallback.
    pub source: String,
    /// One of `healthy`, `missing`, or `error`.
    pub status: String,
    /// The first line reported by `-version`, when the tool starts successfully.
    pub version: Option<String>,
    /// A short, user-facing explanation and next step.
    pub message: String,
}

struct ResolvedBinary {
    path: PathBuf,
    source: &'static str,
}

/// Resolve FFmpeg tools for both terminal launches and macOS Finder launches.
/// Finder supplies a deliberately minimal PATH which normally omits Homebrew's
/// `/opt/homebrew/bin`, even when ffmpeg is installed and available in a
/// developer's terminal.
fn resolve_binary(binary: &str) -> ResolvedBinary {
    let variable = format!("FPV_{}_PATH", binary.to_ascii_uppercase());
    if let Some(path) = std::env::var_os(&variable).filter(|path| !path.is_empty()) {
        return ResolvedBinary {
            path: path.into(),
            source: "Environment override",
        };
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(binary);
            if candidate.is_file() {
                return ResolvedBinary {
                    path: candidate,
                    source: "PATH",
                };
            }
        }
    }
    #[cfg(target_os = "macos")]
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let candidate = PathBuf::from(directory).join(binary);
        if candidate.is_file() {
            return ResolvedBinary {
                path: candidate,
                source: "macOS fallback",
            };
        }
    }
    ResolvedBinary {
        path: PathBuf::from(binary),
        source: "System command lookup",
    }
}

fn command_for(binary: &str) -> Command {
    Command::new(resolve_binary(binary).path)
}

/// Resolve and start a tool once, collecting enough information to help a
/// person fix their local FFmpeg installation without exposing stderr by
/// default in the UI.
pub fn diagnose(binary: &str) -> ToolDiagnostic {
    let resolved = resolve_binary(binary);
    match Command::new(&resolved.path).arg("-version").output() {
        Ok(output) if output.status.success() => ToolDiagnostic {
            binary: binary.to_string(),
            path: resolved.path,
            source: resolved.source.to_string(),
            status: "healthy".to_string(),
            version: version_line(&output.stdout),
            message: "Available and ready to use.".to_string(),
        },
        Ok(output) => ToolDiagnostic {
            binary: binary.to_string(),
            path: resolved.path,
            source: resolved.source.to_string(),
            status: "error".to_string(),
            version: None,
            message: format!(
                "The executable could not start successfully (exit {}). Check that this is a compatible FFmpeg installation.",
                output.status.code().map_or("unknown".to_string(), |code| code.to_string())
            ),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => ToolDiagnostic {
            binary: binary.to_string(),
            path: resolved.path,
            source: resolved.source.to_string(),
            status: "missing".to_string(),
            version: None,
            message: "Install FFmpeg, add it to PATH, or set this tool's FPV_*_PATH override.".to_string(),
        },
        Err(_) => ToolDiagnostic {
            binary: binary.to_string(),
            path: resolved.path,
            source: resolved.source.to_string(),
            status: "error".to_string(),
            version: None,
            message: "FPV Editor could not run this executable. Check that the file is accessible and executable.".to_string(),
        },
    }
}

fn version_line(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
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

/// Run FFmpeg with its machine-readable progress stream enabled. `out_time_us`
/// is reported in microseconds, which keeps the UI independent of locale and
/// FFmpeg's human-readable stderr formatting.
pub fn run_with_progress(
    binary: &str,
    args: &[String],
    mut on_progress: impl FnMut(u64),
) -> MediaResult<()> {
    let mut command_args = args.to_vec();
    let output = command_args
        .pop()
        .ok_or_else(|| MediaError::Parse("missing export output path".into()))?;
    command_args.extend([
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        output,
    ]);
    let mut child = command_for(binary)
        .args(&command_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| MediaError::Spawn {
            binary: binary.to_string(),
            source,
        })?;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_us=") {
                if let Ok(microseconds) = value.parse() {
                    on_progress(microseconds);
                }
            }
        }
    }
    let output = child
        .wait_with_output()
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

#[cfg(test)]
mod tests {
    use super::version_line;

    #[test]
    fn extracts_the_first_non_empty_version_line() {
        assert_eq!(
            version_line(b"ffmpeg version 7.1 Copyright\nconfiguration: test\n"),
            Some("ffmpeg version 7.1 Copyright".to_string())
        );
        assert_eq!(version_line(b""), None);
    }
}
