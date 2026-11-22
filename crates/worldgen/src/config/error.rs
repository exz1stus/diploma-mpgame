use std::{error::Error, fmt, io, path::PathBuf};

use super::ConfigFile;

#[derive(Debug)]
pub enum ConfigError {
    MissingDir(PathBuf),
    Read { path: PathBuf, error: io::Error },
    Invalid { file: ConfigFile, message: String },
}

impl ConfigError {
    pub(crate) fn invalid(file: ConfigFile, error: impl fmt::Display) -> Self {
        Self::Invalid {
            file,
            message: error.to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDir(path) => write!(f, "config folder {} does not exist", path.display()),
            Self::Read { path, error } => write!(f, "cannot read {}: {error}", path.display()),
            Self::Invalid { file, message } => write!(f, "invalid {}: {message}", file.path()),
        }
    }
}

impl Error for ConfigError {}
