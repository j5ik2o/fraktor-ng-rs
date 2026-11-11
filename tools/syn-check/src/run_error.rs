use std::{fmt, io, path::PathBuf};

/// Why a run could not start.
#[derive(Debug)]
pub enum RunError {
    UnknownRule(String),
    Io(PathBuf, io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRule(rule) => write!(formatter, "unknown rule `{rule}`"),
            Self::Io(path, error) => write!(formatter, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for RunError {}
