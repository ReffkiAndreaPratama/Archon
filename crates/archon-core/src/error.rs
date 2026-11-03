use std::path::PathBuf;

/// Errors surfaced by Archon's archive core.
///
/// Every variant has a stable machine-readable [`ArchonError::code`] so the
/// desktop UI and CLI can react without parsing human text.
#[derive(Debug, thiserror::Error)]
pub enum ArchonError {
    #[error("{0}")]
    Io(String),

    #[error("File not found: {}", .0.display())]
    NotFound(PathBuf),

    #[error("{0}")]
    InvalidArchive(String),

    #[error("{0}")]
    UnsupportedFormat(String),

    #[error("{0}")]
    UnsupportedOperation(String),

    #[error("A password is required to read this archive")]
    PasswordRequired,

    #[error("The password is incorrect")]
    InvalidPassword,

    #[error("Unsafe archive entry rejected: {entry}")]
    PathTraversal { entry: String },

    #[error("{0}")]
    LimitExceeded(String),

    #[error("{0}")]
    Conflict(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Not enough disk space: {needed} bytes required, {available} bytes available")]
    InsufficientSpace { needed: u64, available: u64 },

    #[error("{0}")]
    Configuration(String),

    #[error("{0}")]
    NotImplemented(String),

    #[error("{0}")]
    Engine(String),
}

impl ArchonError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::NotFound(_) => "not_found",
            Self::InvalidArchive(_) => "invalid_archive",
            Self::UnsupportedFormat(_) => "unsupported_format",
            Self::UnsupportedOperation(_) => "unsupported_operation",
            Self::PasswordRequired => "password_required",
            Self::InvalidPassword => "invalid_password",
            Self::PathTraversal { .. } => "path_traversal",
            Self::LimitExceeded(_) => "limit_exceeded",
            Self::Conflict(_) => "conflict",
            Self::Cancelled => "cancelled",
            Self::InsufficientSpace { .. } => "insufficient_space",
            Self::Configuration(_) => "configuration",
            Self::NotImplemented(_) => "not_implemented",
            Self::Engine(_) => "engine",
        }
    }

    pub fn invalid_archive(message: impl Into<String>) -> Self {
        Self::InvalidArchive(message.into())
    }
}

impl From<std::io::Error> for ArchonError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<zip::result::ZipError> for ArchonError {
    fn from(value: zip::result::ZipError) -> Self {
        use zip::result::ZipError;
        match value {
            ZipError::Io(error) => Self::Io(error.to_string()),
            ZipError::InvalidArchive(message) => Self::InvalidArchive(message.into_owned()),
            ZipError::UnsupportedArchive(message) if message == ZipError::PASSWORD_REQUIRED => {
                Self::PasswordRequired
            }
            ZipError::UnsupportedArchive(message) => {
                Self::UnsupportedOperation(format!("Unsupported ZIP feature: {message}"))
            }
            ZipError::FileNotFound => Self::InvalidArchive("Entry not found in archive".into()),
            ZipError::InvalidPassword => Self::InvalidPassword,
            ZipError::CompressionMethodNotSupported(id) => Self::UnsupportedOperation(format!(
                "This archive uses compression method {id}, which is not supported by the current engine"
            )),
            ZipError::DecompressedSizeLimitExceeded { size, limit } => Self::LimitExceeded(
                format!("Decompressed size {size} exceeds the safety limit of {limit} bytes"),
            ),
            ZipError::DecompressedSizeUnknown => Self::LimitExceeded(
                "Decompressed size is unknown, so limits cannot be verified".into(),
            ),
            other => Self::Engine(other.to_string()),
        }
    }
}

pub type ArchonResult<T> = Result<T, ArchonError>;
