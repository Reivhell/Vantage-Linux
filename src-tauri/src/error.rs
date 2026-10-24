use std::fmt;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Machine-readable, structured errors surfaced to the frontend and logs.
#[derive(Debug, Clone)]
pub enum AppError {
    /// Underlying IO failure (file, sysfs, helper spawn).
    Io { op: String, path: Option<PathBuf>, source: String },
    /// Operation requires elevated privileges that are not available.
    Permission { op: String },
    /// Hardware/firmware does not expose the requested interface.
    HardwareUnsupported { feature: String },
    /// Invalid user input.
    InvalidInput { field: String, reason: String },
    /// Privileged helper invocation failed.
    HelperFailed { subcommand: String, exit: Option<i32>, stderr: String },
    /// Requested entity not found.
    NotFound { kind: String, key: String },
    /// Resource limit / threshold violation.
    LimitExceeded { metric: String, value: f64, limit: f64 },
    /// Serialization / deserialization failure.
    Serde { context: String, source: String },
    /// Generic fallback.
    Other { message: String },
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io { .. } => "ERR_IO",
            AppError::Permission { .. } => "ERR_PERMISSION",
            AppError::HardwareUnsupported { .. } => "ERR_HARDWARE_UNSUPPORTED",
            AppError::InvalidInput { .. } => "ERR_INVALID_INPUT",
            AppError::HelperFailed { .. } => "ERR_HELPER_FAILED",
            AppError::NotFound { .. } => "ERR_NOT_FOUND",
            AppError::LimitExceeded { .. } => "ERR_LIMIT_EXCEEDED",
            AppError::Serde { .. } => "ERR_SERDE",
            AppError::Other { .. } => "ERR_OTHER",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            AppError::Io { .. } => "I/O error",
            AppError::Permission { .. } => "Permission denied",
            AppError::HardwareUnsupported { .. } => "Hardware unsupported",
            AppError::InvalidInput { .. } => "Invalid input",
            AppError::HelperFailed { .. } => "Helper command failed",
            AppError::NotFound { .. } => "Not found",
            AppError::LimitExceeded { .. } => "Limit exceeded",
            AppError::Serde { .. } => "Serialization error",
            AppError::Other { .. } => "Error",
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io { op, path, source } => {
                write!(f, "io error during {op}")?;
                if let Some(p) = path {
                    write!(f, " at {}", p.display())?;
                }
                write!(f, ": {source}")
            }
            AppError::Permission { op } => write!(f, "permission denied for {op}"),
            AppError::HardwareUnsupported { feature } => {
                write!(f, "hardware does not support {feature}")
            }
            AppError::InvalidInput { field, reason } => {
                write!(f, "invalid input for {field}: {reason}")
            }
            AppError::HelperFailed {
                subcommand,
                exit,
                stderr,
            } => {
                write!(f, "helper '{subcommand}' failed")?;
                if let Some(e) = exit {
                    write!(f, " (exit {e})")?;
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            AppError::NotFound { kind, key } => write!(f, "{kind} '{key}' not found"),
            AppError::LimitExceeded {
                metric,
                value,
                limit,
            } => write!(f, "{metric} {value} exceeds limit {limit}"),
            AppError::Serde { context, source } => {
                write!(f, "serde error in {context}: {source}")
            }
            AppError::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io {
            op: "io".into(),
            path: None,
            source: e.to_string(),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Io {
            op: "sqlite".into(),
            path: None,
            source: e.to_string(),
        }
    }
}

impl From<toml::de::Error> for AppError {
    fn from(e: toml::de::Error) -> Self {
        AppError::Serde {
            context: "toml decode".into(),
            source: e.to_string(),
        }
    }
}

impl From<toml::ser::Error> for AppError {
    fn from(e: toml::ser::Error) -> Self {
        AppError::Serde {
            context: "toml encode".into(),
            source: e.to_string(),
        }
    }
}

/// Result alias carrying the structured error.
pub type AppResult<T> = Result<T, AppError>;

/// Frontend-friendly error envelope (JSON-serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub title: String,
    pub message: String,
}

impl From<&AppError> for ErrorEnvelope {
    fn from(e: &AppError) -> Self {
        ErrorEnvelope {
            code: e.code().to_string(),
            title: e.title().to_string(),
            message: e.to_string(),
        }
    }
}

impl From<AppError> for ErrorEnvelope {
    fn from(e: AppError) -> Self {
        (&e).into()
    }
}
