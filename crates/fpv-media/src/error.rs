#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("export was cancelled")]
    Cancelled,
    #[error("failed to spawn `{binary}`: {source}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{binary}` exited with status {status}: {stderr}")]
    NonZeroExit {
        binary: String,
        status: i32,
        stderr: String,
    },
    #[error("failed to parse ffprobe output: {0}")]
    Parse(String),
    #[error("binary `{0}` not found on PATH")]
    BinaryNotFound(String),
}

pub type MediaResult<T> = Result<T, MediaError>;
