use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{ArchonError, ArchonResult};
use crate::progress::{OperationContext, ProgressPhase};

const COPY_BUFFER: usize = 256 * 1024;
const REPORT_INTERVAL: u64 = 1024 * 1024;

/// Stream bytes while enforcing cancellation and reporting throttled progress.
/// Large files are never buffered in memory.
#[allow(clippy::too_many_arguments)]
pub fn copy_with_progress<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    ctx: &OperationContext,
    phase: ProgressPhase,
    current: usize,
    total: usize,
    path: &str,
    bytes_done: &mut u64,
    bytes_total: u64,
) -> ArchonResult<()> {
    let mut buffer = vec![0_u8; COPY_BUFFER];
    let mut since_report = 0_u64;

    loop {
        ctx.check_cancelled()?;
        let read = reader
            .read(&mut buffer)
            .map_err(|error| ArchonError::Io(format!("Read failed for {path}: {error}")))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|error| ArchonError::Io(format!("Write failed for {path}: {error}")))?;
        *bytes_done += read as u64;
        since_report += read as u64;

        if since_report >= REPORT_INTERVAL {
            ctx.report(phase, current, total, Some(path), *bytes_done, bytes_total);
            since_report = 0;
        }
    }

    ctx.report(phase, current, total, Some(path), *bytes_done, bytes_total);
    Ok(())
}

/// Create a collision-free path using Windows Explorer's `name (1).ext`
/// convention.
pub fn unique_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_string());
    let extension = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();

    for counter in 1..=100_000_u32 {
        let next = directory.join(format!("{stem} ({counter}){extension}"));
        if !next.exists() {
            return next;
        }
    }

    directory.join(format!(
        "{stem} ({})-{}{}",
        std::process::id(),
        counter_suffix(),
        extension
    ))
}

fn counter_suffix() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0)
}

/// Create a uniquely named temporary file in `directory`.
pub fn create_temp_file(directory: &Path) -> ArchonResult<(File, PathBuf)> {
    for counter in 0..1_000_u32 {
        let path = directory.join(format!(".archon-{}-{counter}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(ArchonError::Io(format!(
                    "Cannot create a temporary file in {}: {error}",
                    directory.display()
                )));
            }
        }
    }
    Err(ArchonError::Io(format!(
        "Cannot create a temporary file in {}: too many stale files",
        directory.display()
    )))
}

pub fn available_space(path: &Path) -> Option<u64> {
    fs2::available_space(path).ok()
}

pub fn ratio(savings: u64, original: u64) -> f64 {
    if original == 0 {
        0.0
    } else {
        ((1.0 - (savings as f64 / original as f64)) * 100.0).clamp(0.0, 100.0)
    }
}

pub fn zip_time_from_system_time(value: SystemTime) -> zip::DateTime {
    let offset = time::OffsetDateTime::from(value);
    let primitive = time::PrimitiveDateTime::new(offset.date(), offset.time());
    zip::DateTime::try_from(primitive).unwrap_or_default()
}

pub fn file_modified_time(path: &Path) -> Option<zip::DateTime> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(zip_time_from_system_time)
}

pub fn apply_modified_time(path: &Path, value: Option<zip::DateTime>) {
    let Some(value) = value else {
        return;
    };
    let Ok(primitive) = time::PrimitiveDateTime::try_from(value) else {
        return;
    };
    let offset = primitive.assume_utc();
    let timestamp = offset.unix_timestamp();
    let _ = filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(timestamp, 0));
}

pub fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

/// Archive-relative names always use `/` regardless of host platform.
pub fn archive_name_from_relative(relative: &Path) -> ArchonResult<String> {
    let mut parts = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(value) = component else {
            return Err(ArchonError::Configuration(format!(
                "Unsupported source path component in {}",
                relative.display()
            )));
        };
        let Some(value) = value.to_str() else {
            return Err(ArchonError::UnsupportedOperation(format!(
                "Non-UTF-8 file names are not supported: {}",
                relative.display()
            )));
        };
        parts.push(value.to_string());
    }
    if parts.is_empty() {
        return Err(ArchonError::Configuration(
            "Cannot derive an archive name from the selected source".into(),
        ));
    }
    Ok(parts.join("/"))
}
