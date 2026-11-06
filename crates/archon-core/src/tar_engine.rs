//! Read-only engine for TAR archives and single-stream compressors.
//!
//! TAR has no per-entry checksums and no compression of its own, so this
//! engine exposes listing, extraction and an integrity read test. Creating or
//! modifying TAR archives is intentionally not implemented; capabilities say so.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::engine::ArchiveEngine;
use crate::error::{ArchonError, ArchonResult};
use crate::fsutil::{available_space, copy_with_progress, create_temp_file, ratio, unique_path};
use crate::model::{
    ArchiveEntry, ArchiveFormat, ArchiveInfo, ArchiveListing, ArchiveListing as Listing,
    CapabilityStatus, CreateOptions, CreateReport, ExtractOptions, ExtractReport, FormatCapability,
    Limits, TestIssue, TestReport,
};
use crate::progress::{OperationContext, ProgressPhase};
use crate::security::{normalize_archive_path, path_key, safe_join, sanitize_entry_path};

const READABLE_FORMATS: &[ArchiveFormat] = &[
    ArchiveFormat::Tar,
    ArchiveFormat::TarGz,
    ArchiveFormat::TarBz2,
    ArchiveFormat::TarXz,
    ArchiveFormat::Gz,
    ArchiveFormat::Bz2,
    ArchiveFormat::Xz,
    ArchiveFormat::Zst,
];

pub struct TarEngine {
    limits: Limits,
}

impl Default for TarEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TarEngine {
    pub fn new() -> Self {
        Self {
            limits: Limits::default(),
        }
    }

    fn format_of(&self, path: &Path) -> ArchiveFormat {
        crate::engine::detect_format(path)
    }

    fn open_decoder(&self, path: &Path, format: ArchiveFormat) -> ArchonResult<Box<dyn Read>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let decoder: Box<dyn Read> = match format {
            ArchiveFormat::Tar => Box::new(reader),
            ArchiveFormat::TarGz | ArchiveFormat::Gz => {
                Box::new(flate2::read::GzDecoder::new(reader))
            }
            ArchiveFormat::TarBz2 | ArchiveFormat::Bz2 => {
                Box::new(bzip2::read::BzDecoder::new(reader))
            }
            ArchiveFormat::TarXz | ArchiveFormat::Xz => Box::new(xz2::read::XzDecoder::new(reader)),
            ArchiveFormat::Zst => {
                Box::new(zstd::stream::read::Decoder::new(reader).map_err(|error| {
                    ArchonError::InvalidArchive(format!(
                        "{} is not a readable Zstandard stream: {error}",
                        path.display()
                    ))
                })?)
            }
            other => {
                return Err(ArchonError::UnsupportedFormat(format!(
                    "The TAR engine cannot read {} archives",
                    other.label()
                )));
            }
        };
        Ok(decoder)
    }

    fn is_single_stream(format: ArchiveFormat) -> bool {
        matches!(
            format,
            ArchiveFormat::Gz | ArchiveFormat::Bz2 | ArchiveFormat::Xz | ArchiveFormat::Zst
        )
    }

    fn method_label(format: ArchiveFormat) -> &'static str {
        match format {
            ArchiveFormat::Tar => "TAR",
            ArchiveFormat::TarGz | ArchiveFormat::Gz => "Gzip",
            ArchiveFormat::TarBz2 | ArchiveFormat::Bz2 => "Bzip2",
            ArchiveFormat::TarXz | ArchiveFormat::Xz => "Xz",
            ArchiveFormat::Zst => "Zstandard",
            _ => "Unknown",
        }
    }

    fn read_entries(
        &self,
        path: &Path,
        format: ArchiveFormat,
    ) -> ArchonResult<(Vec<StreamEntry>, Vec<String>)> {
        if Self::is_single_stream(format) {
            let name = single_stream_name(path, format);
            let modified = fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(system_time_seconds)
                .and_then(format_unix_seconds);
            let size = if format == ArchiveFormat::Gz {
                gzip_isize(path).unwrap_or(0)
            } else {
                0
            };
            let mut warnings = Vec::new();
            if size == 0 && format != ArchiveFormat::Gz {
                warnings.push(
                    "The uncompressed size is not stored in this stream and is only known after extraction."
                        .into(),
                );
            }
            return Ok((
                vec![StreamEntry {
                    path: name,
                    is_dir: false,
                    is_symlink: false,
                    size,
                    modified,
                    compressed_size: fs::metadata(path).map(|value| value.len()).unwrap_or(0),
                }],
                warnings,
            ));
        }

        let reader = self.open_decoder(path, format)?;
        let mut archive = tar::Archive::new(reader);
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut unsafe_entries = 0_usize;
        let mut symlink_entries = 0_usize;
        let mut invalid_names = 0_usize;

        let tar_entries = archive.entries().map_err(|error| {
            ArchonError::InvalidArchive(format!(
                "{} is not a readable TAR archive: {error}",
                path.display()
            ))
        })?;

        for (index, entry) in tar_entries.enumerate() {
            let entry = entry.map_err(|error| {
                ArchonError::InvalidArchive(format!(
                    "Cannot read TAR entry in {}: {error}",
                    path.display()
                ))
            })?;
            if entries.len() >= self.limits.max_entries {
                return Err(ArchonError::LimitExceeded(format!(
                    "This archive contains more than {} entries; the safety limit was reached",
                    self.limits.max_entries
                )));
            }

            let raw = match entry.path() {
                Ok(value) => value.to_string_lossy().replace('\\', "/"),
                Err(_) => {
                    invalid_names += 1;
                    String::new()
                }
            };
            let trimmed = raw.trim_end_matches('/');
            if trimmed.is_empty()
                || trimmed
                    .split('/')
                    .all(|component| component.is_empty() || component == ".")
            {
                // Common tar writers emit a `./` entry for the archive root.
                // It has no filesystem target of its own, so it is not an entry.
                continue;
            }
            let raw = trimmed.to_string();
            if sanitize_entry_path(&raw).is_err() {
                unsafe_entries += 1;
            }

            let kind = entry.header().entry_type();
            let is_dir = kind.is_dir();
            let is_symlink = kind.is_symlink() || kind.is_hard_link();
            if is_symlink {
                symlink_entries += 1;
            }
            let size = if is_dir || is_symlink {
                0
            } else {
                entry.size()
            };
            if size > self.limits.max_entry_uncompressed {
                return Err(ArchonError::LimitExceeded(format!(
                    "Entry {raw} declares {size} bytes, above the per-file limit of {}",
                    self.limits.max_entry_uncompressed
                )));
            }
            let modified = entry
                .header()
                .mtime()
                .ok()
                .and_then(|seconds| format_unix_seconds(seconds));

            entries.push(StreamEntry {
                path: raw,
                is_dir,
                is_symlink,
                size,
                modified,
                compressed_size: 0,
            });
            let _ = index;
        }

        if unsafe_entries > 0 {
            warnings.push(format!(
                "{unsafe_entries} archive entr{} use unsafe paths and will be rejected during extraction",
                if unsafe_entries == 1 { "y" } else { "ies" }
            ));
        }
        if symlink_entries > 0 {
            warnings.push(format!(
                "{symlink_entries} link entr{} detected; links are never created",
                if symlink_entries == 1 { "y" } else { "ies" }
            ));
        }
        if invalid_names > 0 {
            warnings.push(format!(
                "{invalid_names} entr{} have names that are not valid UTF-8",
                if invalid_names == 1 { "y" } else { "ies" }
            ));
        }

        Ok((entries, warnings))
    }

    fn listing(&self, path: &Path) -> ArchonResult<ArchiveListing> {
        let format = self.format_of(path);
        let (stream_entries, mut warnings) = self.read_entries(path, format)?;
        let file_size = fs::metadata(path).map(|value| value.len()).unwrap_or(0);

        let mut entries = Vec::with_capacity(stream_entries.len());
        let mut total_size = 0_u64;
        for (index, entry) in stream_entries.into_iter().enumerate() {
            if !entry.is_dir {
                total_size = total_size.saturating_add(entry.size);
            }
            let name = entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&entry.path)
                .to_string();
            let parent = entry
                .path
                .rfind('/')
                .map(|position| entry.path[..position].to_string())
                .unwrap_or_default();
            entries.push(ArchiveEntry {
                index,
                path: entry.path,
                name,
                parent,
                is_dir: entry.is_dir,
                is_symlink: entry.is_symlink,
                size: entry.size,
                compressed_size: entry.compressed_size,
                ratio: ratio(entry.compressed_size, entry.size),
                modified: entry.modified,
                encrypted: false,
                crc32: 0,
                method: Self::method_label(format).to_string(),
            });
        }

        if Self::is_single_stream(format) {
            warnings.push(
                "TAR metadata and checksums do not apply to single-stream formats; the entry name comes from the file name."
                    .into(),
            );
        }

        Ok(Listing {
            path: path.display().to_string(),
            format,
            format_label: format.label().to_string(),
            file_size,
            entries,
            total_size,
            total_compressed: file_size,
            encrypted_entries: 0,
            comment: None,
            warnings,
        })
    }

    fn selected_plan(
        &self,
        listing: &ArchiveListing,
        selected: Option<&[String]>,
    ) -> ArchonResult<Vec<(usize, PathBuf, StreamMeta)>> {
        let Some(selected) = selected else {
            return self.plan_from_entries(&listing.entries);
        };

        if selected.is_empty() {
            return Err(ArchonError::Configuration(
                "No archive entries were selected".into(),
            ));
        }
        let mut wanted: Vec<String> = selected
            .iter()
            .map(|value| normalize_archive_path(value))
            .filter(|value| !value.is_empty())
            .collect();
        wanted.sort();
        wanted.dedup();
        if wanted.is_empty() {
            return Err(ArchonError::Configuration(
                "No archive entries were selected".into(),
            ));
        }

        let mut matched = vec![false; wanted.len()];
        let mut chosen = Vec::new();
        for entry in &listing.entries {
            let path = normalize_archive_path(&entry.path);
            let mut hit = false;
            for (position, candidate) in wanted.iter().enumerate() {
                if path == *candidate || path.starts_with(&format!("{candidate}/")) {
                    matched[position] = true;
                    hit = true;
                }
            }
            if hit {
                chosen.push(entry.clone());
            }
        }
        for (position, was_matched) in matched.into_iter().enumerate() {
            if !was_matched {
                return Err(ArchonError::Conflict(format!(
                    "Selected entry not found in archive: {}",
                    wanted[position]
                )));
            }
        }
        self.plan_from_entries(&chosen)
    }

    fn plan_from_entries(
        &self,
        entries: &[ArchiveEntry],
    ) -> ArchonResult<Vec<(usize, PathBuf, StreamMeta)>> {
        let mut plan = Vec::new();
        let mut file_keys = HashSet::new();
        let mut dir_keys = HashSet::new();

        for entry in entries {
            if entry.is_symlink {
                continue;
            }
            let safe = sanitize_entry_path(&entry.path)?;
            let relative = if safe.as_os_str().is_empty() {
                return Err(ArchonError::PathTraversal {
                    entry: entry.path.clone(),
                });
            } else {
                safe
            };
            let key = path_key(&relative);

            if entry.is_dir {
                dir_keys.insert(key);
            } else {
                if dir_keys.contains(&key) {
                    return Err(ArchonError::Conflict(format!(
                        "Archive entry {} is both a file and a folder",
                        relative.display()
                    )));
                }
                if !file_keys.insert(key.clone()) {
                    return Err(ArchonError::Conflict(format!(
                        "Two entries would extract to the same path: {}",
                        relative.display()
                    )));
                }
                let mut ancestor = relative.parent();
                while let Some(parent) = ancestor {
                    if !parent.as_os_str().is_empty() && file_keys.contains(&path_key(parent)) {
                        return Err(ArchonError::Conflict(format!(
                            "Archive entry {} conflicts with a file of the same name",
                            relative.display()
                        )));
                    }
                    ancestor = parent.parent();
                }
            }
            if entry.is_dir && file_keys.contains(&path_key(&relative)) {
                return Err(ArchonError::Conflict(format!(
                    "Archive entry {} is both a file and a folder",
                    relative.display()
                )));
            }

            plan.push((
                entry.index,
                relative,
                StreamMeta {
                    path: entry.path.clone(),
                    is_dir: entry.is_dir,
                    size: entry.size,
                    modified: entry.modified.clone(),
                },
            ));
        }

        Ok(plan)
    }
}

#[derive(Debug, Clone)]
struct StreamEntry {
    path: String,
    is_dir: bool,
    is_symlink: bool,
    size: u64,
    modified: Option<String>,
    compressed_size: u64,
}

#[derive(Debug, Clone)]
struct StreamMeta {
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
}

impl ArchiveEngine for TarEngine {
    fn id(&self) -> &'static str {
        "tar"
    }

    fn capabilities(&self) -> Vec<FormatCapability> {
        READABLE_FORMATS
            .iter()
            .map(|format| {
                let label = format.label();
                let notes = if Self::is_single_stream(*format) {
                    format!(
                        "{label} single-stream decompression is read-only. Creating, modifying and encrypting {label} archives is not implemented."
                    )
                } else {
                    format!(
                        "{label} listing, extraction and integrity testing are implemented. Creating and modifying {label} archives is not implemented, and TAR stores no per-entry checksum."
                    )
                };
                FormatCapability {
                    format: *format,
                    label: label.to_string(),
                    extensions: format
                        .extensions()
                        .iter()
                        .map(|value| (*value).to_string())
                        .collect(),
                    can_read: true,
                    can_create: false,
                    can_modify: false,
                    can_test: true,
                    encryption: false,
                    multi_volume: false,
                    status: CapabilityStatus::ReadOnly,
                    notes,
                }
            })
            .collect()
    }

    fn list(&self, archive: &Path, _password: Option<&str>) -> ArchonResult<ArchiveListing> {
        self.listing(archive)
    }

    fn info(&self, archive: &Path, _password: Option<&str>) -> ArchonResult<ArchiveInfo> {
        let listing = self.listing(archive)?;
        let file_count = listing.entries.iter().filter(|entry| !entry.is_dir).count();
        let dir_count = listing.entries.len() - file_count;
        Ok(ArchiveInfo {
            path: listing.path.clone(),
            format: listing.format,
            format_label: listing.format_label.clone(),
            archive_size: listing.file_size,
            entry_count: listing.entries.len(),
            file_count,
            dir_count,
            total_size: listing.total_size,
            total_compressed: listing.total_compressed,
            ratio: ratio(listing.total_compressed, listing.total_size),
            encrypted_entries: 0,
            comment: None,
            warnings: listing.warnings,
        })
    }

    fn create(
        &self,
        _options: &CreateOptions,
        _ctx: &OperationContext,
    ) -> ArchonResult<CreateReport> {
        Err(ArchonError::UnsupportedOperation(
            "Creating TAR, GZIP, BZIP2, XZ or Zstandard archives is not implemented. Phase 1 creates ZIP only."
                .into(),
        ))
    }

    fn extract(
        &self,
        options: &ExtractOptions,
        ctx: &OperationContext,
    ) -> ArchonResult<ExtractReport> {
        let started = Instant::now();
        ctx.check_cancelled()?;

        if options.destination.as_os_str().is_empty() {
            return Err(ArchonError::Configuration(
                "Choose an extraction folder".into(),
            ));
        }
        if options.destination.exists() && !options.destination.is_dir() {
            return Err(ArchonError::Conflict(format!(
                "The extraction destination is not a folder: {}",
                options.destination.display()
            )));
        }

        let format = self.format_of(&options.archive);
        let listing = self.listing(&options.archive)?;
        let selected = self.selected_plan(&listing, options.entries.as_deref())?;
        if selected.len() > self.limits.max_entries {
            return Err(ArchonError::LimitExceeded(format!(
                "This extraction contains {} entries; the safety limit is {}",
                selected.len(),
                self.limits.max_entries
            )));
        }

        let total_size = selected.iter().try_fold(0_u64, |sum, (_, _, entry)| {
            sum.checked_add(if entry.is_dir { 0 } else { entry.size })
                .ok_or_else(|| ArchonError::LimitExceeded("Archive size metadata overflows".into()))
        })?;
        if total_size > self.limits.max_total_uncompressed {
            return Err(ArchonError::LimitExceeded(format!(
                "This extraction would produce {total_size} bytes; the safety limit is {}",
                self.limits.max_total_uncompressed
            )));
        }

        fs::create_dir_all(&options.destination).map_err(|error| {
            ArchonError::Io(format!(
                "Cannot create {}: {error}",
                options.destination.display()
            ))
        })?;
        let destination = options.destination.canonicalize().map_err(|error| {
            ArchonError::Io(format!(
                "Cannot resolve {}: {error}",
                options.destination.display()
            ))
        })?;
        if let Some(available) = available_space(&destination) {
            if total_size > available {
                return Err(ArchonError::InsufficientSpace {
                    needed: total_size,
                    available,
                });
            }
        }

        let mut warnings = listing.warnings.clone();
        let symlinks = listing
            .entries
            .iter()
            .filter(|entry| entry.is_symlink)
            .count();
        let mut plan: Vec<(usize, PathBuf, StreamMeta)> = selected;
        if !options.preserve_structure {
            for (_, relative, _) in plan.iter_mut() {
                let name = relative
                    .file_name()
                    .ok_or_else(|| ArchonError::PathTraversal {
                        entry: relative.display().to_string(),
                    })?
                    .to_owned();
                *relative = PathBuf::from(name);
            }
            let mut seen = HashSet::new();
            for (_, relative, _) in &plan {
                if !seen.insert(path_key(relative)) {
                    return Err(ArchonError::Conflict(format!(
                        "Two entries would extract to the same file: {}",
                        relative.display()
                    )));
                }
            }
        }
        if symlinks > 0 && !warnings.iter().any(|value| value.contains("link entr")) {
            warnings.push(format!(
                "{symlinks} link entr{} skipped; Archon never creates links from archives",
                if symlinks == 1 { "y was" } else { "ies were" }
            ));
        }

        let file_total = plan.iter().filter(|(_, _, entry)| !entry.is_dir).count();
        let mut bytes_done = 0_u64;
        let mut extracted_files = 0_usize;
        let mut extracted_dirs = 0_usize;
        let mut skipped = 0_usize;
        let mut processed = 0_usize;

        ctx.report(
            ProgressPhase::Extracting,
            0,
            file_total,
            None,
            0,
            total_size,
        );

        let mut by_path: HashMap<String, usize> = HashMap::new();
        for (position, (_, _, entry)) in plan.iter().enumerate() {
            let key = normalize_archive_path(&entry.path);
            if by_path.insert(key, position).is_some() {
                return Err(ArchonError::Conflict(format!(
                    "Two archive entries share the path {}",
                    entry.path
                )));
            }
        }

        if Self::is_single_stream(format) {
            let (_, relative, entry) = &plan[0];
            let target = safe_join(&destination, relative)?;
            let parent = target.parent().ok_or_else(|| {
                ArchonError::Configuration(format!(
                    "Cannot determine the parent folder for {}",
                    target.display()
                ))
            })?;
            fs::create_dir_all(parent).map_err(|error| {
                ArchonError::Io(format!("Cannot create {}: {error}", parent.display()))
            })?;
            let final_path = resolve_overwrite(&target, options.overwrite, &mut skipped)?;
            if let Some(final_path) = final_path {
                processed += 1;
                let mut reader = self.open_decoder(&options.archive, format)?;
                let (mut temp_file, temp_path) = create_temp_file(parent)?;
                let copy_result = copy_with_progress(
                    &mut reader,
                    &mut temp_file,
                    ctx,
                    ProgressPhase::Extracting,
                    processed,
                    file_total,
                    &entry.path,
                    &mut bytes_done,
                    total_size,
                )
                .and_then(|_| {
                    temp_file.flush().map_err(|error| {
                        ArchonError::Io(format!("Cannot flush {}: {error}", entry.path))
                    })
                });
                if let Err(error) = copy_result {
                    drop(temp_file);
                    let _ = fs::remove_file(&temp_path);
                    return Err(error);
                }
                drop(temp_file);
                if bytes_done > self.limits.max_entry_uncompressed {
                    let _ = fs::remove_file(&temp_path);
                    return Err(ArchonError::LimitExceeded(format!(
                        "Entry {} produced {bytes_done} bytes, above the per-file limit of {}",
                        entry.path, self.limits.max_entry_uncompressed
                    )));
                }
                finish_file(&temp_path, &final_path, options.overwrite)?;
                apply_unix_mtime(&final_path, entry.modified.as_deref());
                extracted_files += 1;
            }
            return Ok(ExtractReport {
                destination: options.destination.display().to_string(),
                extracted_files,
                extracted_dirs,
                skipped,
                total_bytes: bytes_done,
                elapsed_ms: started.elapsed().as_millis(),
                warnings,
            });
        }

        let reader = self.open_decoder(&options.archive, format)?;
        let mut archive = tar::Archive::new(reader);
        let tar_entries = archive.entries().map_err(|error| {
            ArchonError::InvalidArchive(format!(
                "{} is not a readable TAR archive: {error}",
                options.archive.display()
            ))
        })?;
        let mut done: HashSet<usize> = HashSet::new();

        for entry in tar_entries {
            let mut entry = entry.map_err(|error| {
                ArchonError::InvalidArchive(format!(
                    "Cannot read TAR entry in {}: {error}",
                    options.archive.display()
                ))
            })?;
            let raw = entry
                .path()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let key = normalize_archive_path(raw.trim_end_matches('/'));
            let Some(&position) = by_path.get(&key) else {
                continue;
            };
            if !done.insert(position) {
                continue;
            }
            let (_, relative, meta) = &plan[position];
            ctx.check_cancelled()?;
            let target = safe_join(&destination, relative)?;

            if meta.is_dir {
                if options.preserve_structure {
                    fs::create_dir_all(&target).map_err(|error| {
                        ArchonError::Io(format!("Cannot create {}: {error}", target.display()))
                    })?;
                    apply_unix_mtime(&target, meta.modified.as_deref());
                    extracted_dirs += 1;
                }
                continue;
            }

            processed += 1;
            let parent = target.parent().ok_or_else(|| {
                ArchonError::Configuration(format!(
                    "Cannot determine the parent folder for {}",
                    target.display()
                ))
            })?;
            fs::create_dir_all(parent).map_err(|error| {
                ArchonError::Io(format!("Cannot create {}: {error}", parent.display()))
            })?;

            let Some(final_path) = resolve_overwrite(&target, options.overwrite, &mut skipped)?
            else {
                continue;
            };

            let (mut temp_file, temp_path) = create_temp_file(parent)?;
            let bytes_before = bytes_done;
            let copy_result = copy_with_progress(
                &mut entry,
                &mut temp_file,
                ctx,
                ProgressPhase::Extracting,
                processed,
                file_total,
                &meta.path,
                &mut bytes_done,
                total_size,
            )
            .and_then(|_| {
                temp_file.flush().map_err(|error| {
                    ArchonError::Io(format!("Cannot flush {}: {error}", meta.path))
                })
            });
            if let Err(error) = copy_result {
                drop(temp_file);
                let _ = fs::remove_file(&temp_path);
                return Err(error);
            }
            drop(temp_file);
            let entry_bytes = bytes_done.saturating_sub(bytes_before);
            if entry_bytes > self.limits.max_entry_uncompressed {
                let _ = fs::remove_file(&temp_path);
                return Err(ArchonError::LimitExceeded(format!(
                    "Entry {} produced {entry_bytes} bytes, above the per-file limit of {}",
                    meta.path, self.limits.max_entry_uncompressed
                )));
            }
            finish_file(&temp_path, &final_path, options.overwrite)?;
            apply_unix_mtime(&final_path, meta.modified.as_deref());
            extracted_files += 1;
        }

        Ok(ExtractReport {
            destination: options.destination.display().to_string(),
            extracted_files,
            extracted_dirs,
            skipped,
            total_bytes: bytes_done,
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
    }

    fn test(
        &self,
        archive: &Path,
        _password: Option<&str>,
        ctx: &OperationContext,
    ) -> ArchonResult<TestReport> {
        let started = Instant::now();
        ctx.check_cancelled()?;
        let format = self.format_of(archive);
        let listing = self.listing(archive)?;
        let mut issues = Vec::new();
        let mut warnings = listing.warnings.clone();
        let mut bytes_checked = 0_u64;
        let mut entries_checked = 0_usize;
        let total_size = listing.total_size;

        if Self::is_single_stream(format) {
            let mut reader = self.open_decoder(archive, format)?;
            let mut sink = std::io::sink();
            let mut bytes = 0_u64;
            match copy_with_progress(
                &mut reader,
                &mut sink,
                ctx,
                ProgressPhase::Testing,
                1,
                1,
                &listing.entries[0].path,
                &mut bytes,
                total_size,
            ) {
                Ok(()) => {
                    bytes_checked = bytes;
                    entries_checked = 1;
                }
                Err(ArchonError::Cancelled) => return Err(ArchonError::Cancelled),
                Err(error) => issues.push(TestIssue {
                    entry: listing.entries[0].path.clone(),
                    kind: "read_error".into(),
                    message: error.to_string(),
                }),
            }
            if let Some(entry) = listing.entries.first() {
                if entry.size > 0 && bytes != entry.size {
                    issues.push(TestIssue {
                        entry: entry.path.clone(),
                        kind: "size_mismatch".into(),
                        message: format!(
                            "Expected {} bytes, decompressed {} bytes",
                            entry.size, bytes
                        ),
                    });
                }
            }
            return Ok(TestReport {
                ok: issues.is_empty(),
                entries_checked,
                bytes_checked,
                encrypted_entries: 0,
                issues,
                elapsed_ms: started.elapsed().as_millis(),
                warnings: std::mem::take(&mut warnings),
            });
        }

        let reader = self.open_decoder(archive, format)?;
        let mut tar = tar::Archive::new(reader);
        let tar_entries = tar.entries().map_err(|error| {
            ArchonError::InvalidArchive(format!(
                "{} is not a readable TAR archive: {error}",
                archive.display()
            ))
        })?;
        let total = listing.entries.len();

        for (position, entry) in tar_entries.enumerate() {
            ctx.check_cancelled()?;
            let mut entry = match entry {
                Ok(value) => value,
                Err(error) => {
                    issues.push(TestIssue {
                        entry: format!("entry {position}"),
                        kind: "read_error".into(),
                        message: error.to_string(),
                    });
                    break;
                }
            };
            let raw = entry
                .path()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let trimmed = raw.trim_end_matches('/');
            if trimmed.is_empty()
                || trimmed
                    .split('/')
                    .all(|component| component.is_empty() || component == ".")
            {
                continue;
            }
            let raw = trimmed.to_string();
            let is_dir = entry.header().entry_type().is_dir();
            let is_link = entry.header().entry_type().is_symlink()
                || entry.header().entry_type().is_hard_link();
            let declared = entry.size();
            if is_dir || is_link {
                entries_checked += 1;
                continue;
            }
            let mut sink = std::io::sink();
            let mut bytes = 0_u64;
            match copy_with_progress(
                &mut entry,
                &mut sink,
                ctx,
                ProgressPhase::Testing,
                position + 1,
                total,
                &raw,
                &mut bytes,
                total_size,
            ) {
                Ok(()) => {
                    entries_checked += 1;
                    bytes_checked = bytes_checked.saturating_add(bytes);
                    if bytes != declared {
                        if issues.len() < self.limits.max_reported_issues {
                            issues.push(TestIssue {
                                entry: raw,
                                kind: "size_mismatch".into(),
                                message: format!("Expected {declared} bytes, read {bytes} bytes"),
                            });
                        }
                    }
                }
                Err(ArchonError::Cancelled) => return Err(ArchonError::Cancelled),
                Err(error) => {
                    if issues.len() < self.limits.max_reported_issues {
                        issues.push(TestIssue {
                            entry: raw,
                            kind: "read_error".into(),
                            message: error.to_string(),
                        });
                    }
                    break;
                }
            }
        }

        if issues.len() >= self.limits.max_reported_issues {
            warnings.push("The issue list was truncated at the safety limit".into());
        }

        Ok(TestReport {
            ok: issues.is_empty(),
            entries_checked,
            bytes_checked,
            encrypted_entries: 0,
            issues,
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
    }
}

fn single_stream_name(path: &Path, format: ArchiveFormat) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "stream".to_string());
    let lower = name.to_ascii_lowercase();
    let suffixes: &[&str] = match format {
        ArchiveFormat::Gz => &[".gz"],
        ArchiveFormat::Bz2 => &[".bz2"],
        ArchiveFormat::Xz => &[".xz"],
        ArchiveFormat::Zst => &[".zst", ".zstd"],
        _ => &[],
    };
    for suffix in suffixes {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name
}

fn gzip_isize(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length < 4 {
        return None;
    }
    file.seek(SeekFrom::End(-4)).ok()?;
    let mut buffer = [0_u8; 4];
    file.read_exact(&mut buffer).ok()?;
    Some(u32::from_le_bytes(buffer) as u64)
}

fn resolve_overwrite(
    target: &Path,
    policy: crate::model::OverwritePolicy,
    skipped: &mut usize,
) -> ArchonResult<Option<PathBuf>> {
    use crate::model::OverwritePolicy;

    if !target.exists() {
        return Ok(Some(target.to_path_buf()));
    }
    if policy == OverwritePolicy::Skip {
        *skipped += 1;
        return Ok(None);
    }
    if target.is_dir() {
        return Err(ArchonError::Conflict(format!(
            "Cannot overwrite the folder {} with a file",
            target.display()
        )));
    }
    if policy == OverwritePolicy::Rename {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let name = target
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        return Ok(Some(unique_path(parent, &name)));
    }
    Ok(Some(target.to_path_buf()))
}

fn finish_file(
    temp_path: &Path,
    final_path: &Path,
    policy: crate::model::OverwritePolicy,
) -> ArchonResult<()> {
    use crate::model::OverwritePolicy;

    if policy == OverwritePolicy::Overwrite && final_path.exists() {
        fs::remove_file(final_path).map_err(|error| {
            ArchonError::Io(format!("Cannot replace {}: {error}", final_path.display()))
        })?;
    }
    if let Err(error) = fs::rename(temp_path, final_path) {
        let _ = fs::remove_file(temp_path);
        return Err(ArchonError::Io(format!(
            "Cannot write {}: {error}",
            final_path.display()
        )));
    }
    Ok(())
}

fn system_time_seconds(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn format_unix_seconds(seconds: u64) -> Option<String> {
    let value = time::OffsetDateTime::from_unix_timestamp(seconds as i64).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second()
    ))
}

fn apply_unix_mtime(path: &Path, value: Option<&str>) {
    let Some(value) = value else {
        return;
    };
    // The listing stores a formatted timestamp; convert it back for file times.
    let Some((date, clock)) = value.split_once(' ') else {
        return;
    };
    let mut date_parts = date.split('-');
    let mut clock_parts = clock.split(':');
    let parsed = (|| {
        Some((
            date_parts.next()?.parse::<i32>().ok()?,
            date_parts.next()?.parse::<u8>().ok()?,
            date_parts.next()?.parse::<u8>().ok()?,
            clock_parts.next()?.parse::<u8>().ok()?,
            clock_parts.next()?.parse::<u8>().ok()?,
            clock_parts.next()?.parse::<u8>().ok()?,
        ))
    })();
    let Some((year, month, day, hour, minute, second)) = parsed else {
        return;
    };
    let Ok(month) = time::Month::try_from(month) else {
        return;
    };
    let Ok(date) = time::Date::from_calendar_date(year, month, day) else {
        return;
    };
    let Ok(clock) = time::Time::from_hms(hour, minute, second) else {
        return;
    };
    let value = date.with_time(clock).assume_utc().unix_timestamp();
    let _ = filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(value, 0));
}
