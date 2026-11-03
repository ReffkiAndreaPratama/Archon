use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{ArchonError, ArchonResult};
use crate::model::{
    ArchiveFormat, ArchiveInfo, ArchiveListing, CapabilityStatus, CreateOptions, CreateReport,
    ExtractOptions, ExtractReport, FormatCapability, TestReport,
};
use crate::progress::OperationContext;
use crate::zip_engine::ZipEngine;

/// Common interface every archive engine adapter must implement.
///
/// Operations that an engine cannot perform return
/// [`ArchonError::NotImplemented`]; they are never silently emulated.
pub trait ArchiveEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> Vec<FormatCapability>;

    fn list(&self, archive: &Path) -> ArchonResult<ArchiveListing>;
    fn info(&self, archive: &Path) -> ArchonResult<ArchiveInfo>;
    fn create(&self, options: &CreateOptions, ctx: &OperationContext)
    -> ArchonResult<CreateReport>;
    fn extract(
        &self,
        options: &ExtractOptions,
        ctx: &OperationContext,
    ) -> ArchonResult<ExtractReport>;
    fn test(
        &self,
        archive: &Path,
        password: Option<&str>,
        ctx: &OperationContext,
    ) -> ArchonResult<TestReport>;

    fn add_entries(
        &self,
        _archive: &Path,
        _sources: &[PathBuf],
        _ctx: &OperationContext,
    ) -> ArchonResult<CreateReport> {
        Err(ArchonError::NotImplemented(
            "Adding entries to an existing archive is not implemented in this build".into(),
        ))
    }

    fn remove_entries(
        &self,
        _archive: &Path,
        _entries: &[String],
        _ctx: &OperationContext,
    ) -> ArchonResult<()> {
        Err(ArchonError::NotImplemented(
            "Removing entries from an existing archive is not implemented in this build".into(),
        ))
    }
}

/// Owns the available engine adapters and the format capability registry.
pub struct EngineRegistry {
    zip: ZipEngine,
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self {
            zip: ZipEngine::new(),
        }
    }

    pub fn capabilities(&self) -> Vec<FormatCapability> {
        let mut formats = self.zip.capabilities();
        formats.extend(planned_capabilities());
        formats
    }

    pub fn engine_for_path(&self, path: &Path) -> ArchonResult<&dyn ArchiveEngine> {
        if !path.exists() {
            return Err(ArchonError::NotFound(path.to_path_buf()));
        }
        let format = detect_format(path);
        match format {
            ArchiveFormat::Zip => Ok(&self.zip),
            ArchiveFormat::Unknown => Err(ArchonError::UnsupportedFormat(
                "Archon could not recognise this file as a supported archive".into(),
            )),
            other => Err(unsupported_reading(other)),
        }
    }

    pub fn engine_for_format(&self, format: ArchiveFormat) -> ArchonResult<&dyn ArchiveEngine> {
        match format {
            ArchiveFormat::Zip => Ok(&self.zip),
            other => Err(unsupported_creating(other)),
        }
    }

    pub fn list(&self, path: &Path) -> ArchonResult<ArchiveListing> {
        self.engine_for_path(path)?.list(path)
    }

    pub fn info(&self, path: &Path) -> ArchonResult<ArchiveInfo> {
        self.engine_for_path(path)?.info(path)
    }

    pub fn extract(
        &self,
        options: &ExtractOptions,
        ctx: &OperationContext,
    ) -> ArchonResult<ExtractReport> {
        self.engine_for_path(&options.archive)?
            .extract(options, ctx)
    }

    pub fn create(
        &self,
        options: &CreateOptions,
        ctx: &OperationContext,
    ) -> ArchonResult<CreateReport> {
        self.engine_for_format(options.format)?.create(options, ctx)
    }

    pub fn test(
        &self,
        archive: &Path,
        password: Option<&str>,
        ctx: &OperationContext,
    ) -> ArchonResult<TestReport> {
        self.engine_for_path(archive)?.test(archive, password, ctx)
    }
}

fn unsupported_reading(format: ArchiveFormat) -> ArchonError {
    let phase = match format {
        ArchiveFormat::SevenZ => "Phase 2",
        ArchiveFormat::Rar | ArchiveFormat::Rar5 => "Phase 3",
        ArchiveFormat::Tar
        | ArchiveFormat::TarGz
        | ArchiveFormat::TarBz2
        | ArchiveFormat::TarXz
        | ArchiveFormat::Gz
        | ArchiveFormat::Bz2
        | ArchiveFormat::Xz
        | ArchiveFormat::Zst => "Phase 2",
        _ => "a later phase",
    };
    ArchonError::UnsupportedFormat(format!(
        "{} archives are recognised, but reading them is planned for {phase} and is not available in this build. ZIP is the only readable format in Phase 1.",
        format.label()
    ))
}

fn unsupported_creating(format: ArchiveFormat) -> ArchonError {
    let message = match format {
        ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
            "Creating RAR archives is not supported. RAR is a proprietary format; Archon will only offer extraction through a redistributable engine in Phase 3.".to_string()
        }
        ArchiveFormat::Zip => "ZIP creation is unavailable in this build".to_string(),
        other => format!(
            "Creating {} archives is not implemented in this build. Phase 1 supports ZIP creation.",
            other.label()
        ),
    };
    ArchonError::UnsupportedOperation(message)
}

/// Detect an archive by magic bytes first, then by extension. Magic wins so a
/// ZIP named `.bin` still opens and a text file named `.zip` is rejected.
pub fn detect_format(path: &Path) -> ArchiveFormat {
    if let Ok(format) = detect_magic(path) {
        return format;
    }
    ArchiveFormat::from_path(path)
}

fn detect_magic(path: &Path) -> ArchonResult<ArchiveFormat> {
    let mut file = File::open(path)?;
    let mut header = vec![0_u8; 64 * 1024];
    let read = file.read(&mut header)?;
    header.truncate(read);

    if header.len() >= 4
        && (header.starts_with(b"PK\x03\x04")
            || header.starts_with(b"PK\x05\x06")
            || header.starts_with(b"PK\x07\x08"))
    {
        return Ok(ArchiveFormat::Zip);
    }
    if header.starts_with(b"Rar!\x1a\x07\x01\x00") {
        return Ok(ArchiveFormat::Rar5);
    }
    if header.starts_with(b"Rar!\x1a\x07\x00") {
        return Ok(ArchiveFormat::Rar);
    }
    if header.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return Ok(ArchiveFormat::SevenZ);
    }
    if header.starts_with(&[0x1f, 0x8b]) {
        // Could be a gzip-compressed tar; extension refines this below.
        return Ok(if ArchiveFormat::from_path(path) == ArchiveFormat::TarGz {
            ArchiveFormat::TarGz
        } else {
            ArchiveFormat::Gz
        });
    }
    if header.starts_with(b"BZh") {
        return Ok(if ArchiveFormat::from_path(path) == ArchiveFormat::TarBz2 {
            ArchiveFormat::TarBz2
        } else {
            ArchiveFormat::Bz2
        });
    }
    if header.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        return Ok(if ArchiveFormat::from_path(path) == ArchiveFormat::TarXz {
            ArchiveFormat::TarXz
        } else {
            ArchiveFormat::Xz
        });
    }
    if header.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        return Ok(ArchiveFormat::Zst);
    }
    if header.len() > 262 && &header[257..262] == b"ustar" {
        return Ok(ArchiveFormat::Tar);
    }
    if header.len() > 32_774 && &header[32_769..32_774] == b"CD001" {
        return Ok(ArchiveFormat::Iso);
    }
    if header.starts_with(b"MSWIM\0\0\0") {
        return Ok(ArchiveFormat::Wim);
    }

    Err(ArchonError::UnsupportedFormat(
        "Unrecognised archive signature".into(),
    ))
}

fn planned_capabilities() -> Vec<FormatCapability> {
    use ArchiveFormat as F;

    let entries = [
        (
            F::SevenZ,
            "7Z archives. Reading and creation are planned for Phase 2 using a 7-Zip-compatible engine.",
        ),
        (
            F::Rar,
            "RAR extraction is planned for Phase 3. Creating RAR archives will not be offered.",
        ),
        (
            F::Rar5,
            "RAR5 extraction is planned for Phase 3. Creating RAR archives will not be offered.",
        ),
        (F::Tar, "Uncompressed TAR archives are planned for Phase 2."),
        (F::TarGz, "TAR.GZ archives are planned for Phase 2."),
        (F::TarBz2, "TAR.BZ2 archives are planned for Phase 2."),
        (F::TarXz, "TAR.XZ archives are planned for Phase 2."),
        (F::Gz, "Raw GZIP streams are planned for Phase 2."),
        (F::Bz2, "Raw BZIP2 streams are planned for Phase 2."),
        (F::Xz, "Raw XZ streams are planned for Phase 2."),
        (F::Zst, "Zstandard streams are planned for a later phase."),
        (F::Wim, "WIM images are planned for a later phase."),
        (F::Iso, "ISO images are planned for a later phase."),
    ];

    entries
        .into_iter()
        .map(|(format, notes)| FormatCapability {
            format,
            label: format.label().to_string(),
            extensions: format
                .extensions()
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            can_read: false,
            can_create: false,
            can_modify: false,
            can_test: false,
            encryption: false,
            multi_volume: false,
            status: CapabilityStatus::Planned,
            notes: notes.to_string(),
        })
        .collect()
}
