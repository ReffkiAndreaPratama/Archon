//! Application-level volume splitting and joining.
//!
//! Archon does not write spanned ZIP archives because most unzip tools cannot
//! read them reliably. Instead, a finished archive can be split into numbered
//! parts (`archive.zip.001`, `.002`, ...) and joined back byte-for-byte. The
//! result is a normal archive again after joining.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{ArchonError, ArchonResult};
use crate::fsutil::create_temp_file;

const COPY_BUFFER: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitReport {
    pub source: String,
    pub part_size: u64,
    pub part_count: usize,
    pub total_bytes: u64,
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinReport {
    pub output: String,
    pub part_count: usize,
    pub total_bytes: u64,
}

fn part_path(first: &Path, index: usize) -> PathBuf {
    let mut value = first.as_os_str().to_owned();
    value.push(format!(".{index:03}"));
    PathBuf::from(value)
}

/// Split `path` into parts of at most `part_size` bytes.
///
/// Existing part files are never overwritten; the operation fails before
/// writing anything when a target part already exists.
pub fn split_archive(path: &Path, part_size: u64) -> ArchonResult<SplitReport> {
    if part_size == 0 {
        return Err(ArchonError::Configuration(
            "Choose a volume size greater than zero".into(),
        ));
    }
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ArchonError::NotFound(path.to_path_buf())
        } else {
            ArchonError::Io(format!("Cannot inspect {}: {error}", path.display()))
        }
    })?;
    if !metadata.is_file() {
        return Err(ArchonError::Configuration(format!(
            "Only files can be split: {}",
            path.display()
        )));
    }
    let total_bytes = metadata.len();

    let part_count = if total_bytes == 0 {
        1
    } else {
        total_bytes.div_ceil(part_size) as usize
    };

    let mut targets = Vec::with_capacity(part_count);
    for index in 1..=part_count {
        let target = part_path(path, index);
        if target.exists() {
            return Err(ArchonError::Conflict(format!(
                "The volume already exists: {}. Remove it or choose another size.",
                target.display()
            )));
        }
        targets.push(target);
    }

    let mut input =
        BufReader::new(File::open(path).map_err(|error| {
            ArchonError::Io(format!("Cannot open {}: {error}", path.display()))
        })?);
    let mut written = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER];

    for target in &targets {
        let mut output = BufWriter::new(File::create(target).map_err(|error| {
            ArchonError::Io(format!("Cannot create {}: {error}", target.display()))
        })?);
        let mut remaining = part_size;
        while remaining > 0 {
            let want = remaining.min(buffer.len() as u64) as usize;
            let read = input.read(&mut buffer[..want]).map_err(|error| {
                ArchonError::Io(format!("Cannot read {}: {error}", path.display()))
            })?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).map_err(|error| {
                ArchonError::Io(format!("Cannot write {}: {error}", target.display()))
            })?;
            remaining -= read as u64;
            written += read as u64;
        }
        output.flush().map_err(|error| {
            ArchonError::Io(format!("Cannot flush {}: {error}", target.display()))
        })?;
    }

    if written != total_bytes {
        for target in &targets {
            let _ = fs::remove_file(target);
        }
        return Err(ArchonError::Io(format!(
            "Only {written} of {total_bytes} bytes could be split from {}",
            path.display()
        )));
    }

    Ok(SplitReport {
        source: path.display().to_string(),
        part_size,
        part_count,
        total_bytes,
        parts: targets
            .iter()
            .map(|value| value.display().to_string())
            .collect(),
    })
}

/// Join numbered parts back into a single file.
///
/// `first` must be the `.001` part. Parts are read in order until the next
/// number is missing.
pub fn join_volumes(first: &Path, output: &Path) -> ArchonResult<JoinReport> {
    let first_text = first.to_string_lossy();
    let Some(stem) = first_text.strip_suffix(".001") else {
        return Err(ArchonError::Configuration(
            "Select the first volume, which ends with .001".into(),
        ));
    };
    let stem = PathBuf::from(stem);
    if output.as_os_str().is_empty() {
        return Err(ArchonError::Configuration(
            "Choose an output file path".into(),
        ));
    }
    if output == first || output == stem {
        return Err(ArchonError::Configuration(
            "The joined output cannot replace one of the volume files".into(),
        ));
    }
    if output.exists() {
        return Err(ArchonError::Conflict(format!(
            "The output already exists: {}",
            output.display()
        )));
    }
    let parent = output
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|error| {
            ArchonError::Io(format!("Cannot create {}: {error}", parent.display()))
        })?;
    }

    let mut parts = Vec::new();
    let mut index = 1_usize;
    loop {
        let candidate = part_path(&stem, index);
        if !candidate.exists() {
            break;
        }
        parts.push(candidate);
        index += 1;
        if index > 100_000 {
            return Err(ArchonError::LimitExceeded(
                "More than 100000 volumes were found; stop and check the folder".into(),
            ));
        }
    }
    if parts.is_empty() {
        return Err(ArchonError::NotFound(first.to_path_buf()));
    }

    let (temp_file, temp_path) = create_temp_file(parent)?;
    let mut writer = BufWriter::new(temp_file);
    let mut buffer = vec![0_u8; COPY_BUFFER];

    let build = (|| -> ArchonResult<u64> {
        let mut total_bytes = 0_u64;
        for part in &parts {
            let mut reader = BufReader::new(File::open(part).map_err(|error| {
                ArchonError::Io(format!("Cannot open {}: {error}", part.display()))
            })?);
            loop {
                let read = reader.read(&mut buffer).map_err(|error| {
                    ArchonError::Io(format!("Cannot read {}: {error}", part.display()))
                })?;
                if read == 0 {
                    break;
                }
                writer.write_all(&buffer[..read]).map_err(|error| {
                    ArchonError::Io(format!("Cannot write {}: {error}", temp_path.display()))
                })?;
                total_bytes += read as u64;
            }
        }
        writer.flush().map_err(|error| {
            ArchonError::Io(format!("Cannot flush {}: {error}", temp_path.display()))
        })?;
        writer.get_ref().sync_all().map_err(|error| {
            ArchonError::Io(format!("Cannot flush {}: {error}", temp_path.display()))
        })?;
        Ok(total_bytes)
    })();

    let total_bytes = match build {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };
    drop(writer);

    if let Err(error) = fs::rename(&temp_path, output) {
        let _ = fs::remove_file(&temp_path);
        return Err(ArchonError::Io(format!(
            "Cannot move the joined file to {}: {error}",
            output.display()
        )));
    }

    Ok(JoinReport {
        output: output.display().to_string(),
        part_count: parts.len(),
        total_bytes,
    })
}
