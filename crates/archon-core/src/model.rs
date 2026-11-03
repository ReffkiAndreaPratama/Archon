use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Archive formats recognised by Archon's capability registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArchiveFormat {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    #[serde(rename = "rar")]
    Rar,
    #[serde(rename = "rar5")]
    Rar5,
    #[serde(rename = "tar")]
    Tar,
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "tar.bz2")]
    TarBz2,
    #[serde(rename = "tar.xz")]
    TarXz,
    #[serde(rename = "gz")]
    Gz,
    #[serde(rename = "bz2")]
    Bz2,
    #[serde(rename = "xz")]
    Xz,
    #[serde(rename = "zst")]
    Zst,
    #[serde(rename = "wim")]
    Wim,
    #[serde(rename = "iso")]
    Iso,
    #[serde(rename = "unknown")]
    Unknown,
}

impl ArchiveFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZ => "7Z",
            Self::Rar => "RAR",
            Self::Rar5 => "RAR5",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::TarBz2 => "TAR.BZ2",
            Self::TarXz => "TAR.XZ",
            Self::Gz => "GZ",
            Self::Bz2 => "BZ2",
            Self::Xz => "XZ",
            Self::Zst => "ZST",
            Self::Wim => "WIM",
            Self::Iso => "ISO",
            Self::Unknown => "Unknown",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Zip => &["zip"],
            Self::SevenZ => &["7z"],
            Self::Rar => &["rar"],
            Self::Rar5 => &["rar"],
            Self::Tar => &["tar"],
            Self::TarGz => &["tar.gz", "tgz"],
            Self::TarBz2 => &["tar.bz2", "tbz2", "tbz"],
            Self::TarXz => &["tar.xz", "txz"],
            Self::Gz => &["gz"],
            Self::Bz2 => &["bz2"],
            Self::Xz => &["xz"],
            Self::Zst => &["zst", "zstd"],
            Self::Wim => &["wim"],
            Self::Iso => &["iso"],
            Self::Unknown => &[],
        }
    }

    /// Best-effort format detection from a file name. Multi-part extensions are
    /// checked before their single-part counterparts (`.tar.gz` before `.gz`).
    pub fn from_path(path: &std::path::Path) -> Self {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        const MATCHERS: &[(&str, ArchiveFormat)] = &[
            (".tar.gz", ArchiveFormat::TarGz),
            (".tar.bz2", ArchiveFormat::TarBz2),
            (".tar.xz", ArchiveFormat::TarXz),
            (".tgz", ArchiveFormat::TarGz),
            (".tbz2", ArchiveFormat::TarBz2),
            (".tbz", ArchiveFormat::TarBz2),
            (".txz", ArchiveFormat::TarXz),
            (".zip", ArchiveFormat::Zip),
            (".7z", ArchiveFormat::SevenZ),
            (".rar", ArchiveFormat::Rar),
            (".tar", ArchiveFormat::Tar),
            (".gz", ArchiveFormat::Gz),
            (".bz2", ArchiveFormat::Bz2),
            (".xz", ArchiveFormat::Xz),
            (".zst", ArchiveFormat::Zst),
            (".zstd", ArchiveFormat::Zst),
            (".wim", ArchiveFormat::Wim),
            (".iso", ArchiveFormat::Iso),
        ];

        for (suffix, format) in MATCHERS {
            if name.ends_with(suffix) {
                return *format;
            }
        }
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityStatus {
    Supported,
    ReadOnly,
    Planned,
}

/// What an engine can actually do with one format. Capabilities are never
/// inferred from a file extension alone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    pub format: ArchiveFormat,
    pub label: String,
    pub extensions: Vec<String>,
    pub can_read: bool,
    pub can_create: bool,
    pub can_modify: bool,
    pub can_test: bool,
    pub encryption: bool,
    pub multi_volume: bool,
    pub status: CapabilityStatus,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionPreset {
    Store,
    Fast,
    Normal,
    Maximum,
    Ultra,
}

impl Default for CompressionPreset {
    fn default() -> Self {
        Self::Normal
    }
}

impl CompressionPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Store => "Store",
            Self::Fast => "Fast",
            Self::Normal => "Normal",
            Self::Maximum => "Maximum",
            Self::Ultra => "Ultra",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZipMethod {
    Store,
    Deflate,
}

impl ZipMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Store => "Store",
            Self::Deflate => "Deflate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OverwritePolicy {
    Overwrite,
    Skip,
    #[default]
    Rename,
}

impl OverwritePolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overwrite => "Overwrite",
            Self::Skip => "Skip",
            Self::Rename => "Rename",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub index: usize,
    /// Full path inside the archive, using `/` separators.
    pub path: String,
    /// File or folder name.
    pub name: String,
    /// Parent folder path inside the archive (`""` at the root).
    pub parent: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub compressed_size: u64,
    /// Percentage of bytes saved, 0 when it cannot be computed.
    pub ratio: f64,
    pub modified: Option<String>,
    pub encrypted: bool,
    pub crc32: u32,
    pub method: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveListing {
    pub path: String,
    pub format: ArchiveFormat,
    pub format_label: String,
    pub file_size: u64,
    pub entries: Vec<ArchiveEntry>,
    pub total_size: u64,
    pub total_compressed: u64,
    pub encrypted_entries: usize,
    pub comment: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub path: String,
    pub format: ArchiveFormat,
    pub format_label: String,
    pub archive_size: u64,
    pub entry_count: usize,
    pub file_count: usize,
    pub dir_count: usize,
    pub total_size: u64,
    pub total_compressed: u64,
    pub ratio: f64,
    pub encrypted_entries: usize,
    pub comment: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
    pub sources: Vec<PathBuf>,
    pub output: PathBuf,
    pub format: ArchiveFormat,
    #[serde(default)]
    pub preset: CompressionPreset,
    #[serde(default)]
    pub method: Option<ZipMethod>,
    #[serde(default)]
    pub level: Option<i64>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub overwrite_output: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReport {
    pub output: String,
    pub format: ArchiveFormat,
    pub entry_count: usize,
    pub file_count: usize,
    pub dir_count: usize,
    pub total_size: u64,
    pub archive_size: u64,
    pub ratio: f64,
    pub elapsed_ms: u128,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractOptions {
    pub archive: PathBuf,
    pub destination: PathBuf,
    /// `None` extracts everything. An empty list is rejected.
    #[serde(default)]
    pub entries: Option<Vec<String>>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub overwrite: OverwritePolicy,
    #[serde(default = "default_true")]
    pub preserve_structure: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractReport {
    pub destination: String,
    pub extracted_files: usize,
    pub extracted_dirs: usize,
    pub skipped: usize,
    pub total_bytes: u64,
    pub elapsed_ms: u128,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestIssue {
    pub entry: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReport {
    pub ok: bool,
    pub entries_checked: usize,
    pub bytes_checked: u64,
    pub encrypted_entries: usize,
    pub issues: Vec<TestIssue>,
    pub elapsed_ms: u128,
    pub warnings: Vec<String>,
}

/// Resource-exhaustion guardrails applied before and during extraction.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_entries: usize,
    pub max_total_uncompressed: u64,
    pub max_entry_uncompressed: u64,
    pub max_reported_issues: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 200_000,
            max_total_uncompressed: 100 * 1024 * 1024 * 1024,
            max_entry_uncompressed: 50 * 1024 * 1024 * 1024,
            max_reported_issues: 50,
        }
    }
}
