use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;
use zeroize::Zeroizing;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::{AesMode, CompressionMethod, DateTime, ZipArchive};

use crate::engine::ArchiveEngine;
use crate::error::{ArchonError, ArchonResult};
use crate::fsutil::{
    apply_modified_time, archive_name_from_relative, available_space, copy_with_progress,
    create_temp_file, paths_equal, ratio, zip_time_from_system_time,
};
use crate::model::{
    ArchiveEntry, ArchiveFormat, ArchiveInfo, ArchiveListing, CapabilityStatus, CompressionPreset,
    CreateOptions, CreateReport, ExtractOptions, ExtractReport, FormatCapability, Limits,
    OverwritePolicy, TestIssue, TestReport, ZipMethod,
};
use crate::progress::{OperationContext, ProgressPhase};
use crate::security::{
    normalize_archive_path, path_key as archive_path_key, safe_join, sanitize_entry_path,
};

pub struct ZipEngine {
    limits: Limits,
}

impl Default for ZipEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipEngine {
    pub fn new() -> Self {
        Self {
            limits: Limits::default(),
        }
    }

    pub fn with_limits(limits: Limits) -> Self {
        Self { limits }
    }

    fn open(&self, path: &Path) -> ArchonResult<(ZipArchive<File>, u64)> {
        let file = File::open(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ArchonError::NotFound(path.to_path_buf())
            } else {
                ArchonError::Io(format!("Cannot open {}: {error}", path.display()))
            }
        })?;
        let size = file
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        let archive = ZipArchive::new(file).map_err(|error| {
            if let zip::result::ZipError::InvalidArchive(message) = &error {
                ArchonError::InvalidArchive(format!(
                    "{} is not a valid ZIP archive: {message}",
                    path.display()
                ))
            } else {
                ArchonError::from(error)
            }
        })?;
        Ok((archive, size))
    }

    fn read_listing(&self, path: &Path) -> ArchonResult<ArchiveListing> {
        let (mut archive, file_size) = self.open(path)?;
        let mut entries = Vec::with_capacity(archive.len());
        let mut total_size = 0_u64;
        let mut total_compressed = 0_u64;
        let mut encrypted_entries = 0_usize;
        let mut unsafe_entries = 0_usize;
        let mut symlink_entries = 0_usize;
        let mut invalid_names = 0_usize;

        for index in 0..archive.len() {
            let file = archive.by_index_raw(index)?;
            let raw_name = match file.name() {
                Ok(name) => name.into_owned(),
                Err(_) => {
                    invalid_names += 1;
                    String::from_utf8_lossy(file.name_raw()).into_owned()
                }
            };
            let raw_name = raw_name.replace('\\', "/");
            let is_dir = file.is_dir() || raw_name.ends_with('/');
            let path_inside = raw_name.trim_end_matches('/').to_string();

            if path_inside.is_empty() {
                continue;
            }
            if sanitize_entry_path(&path_inside).is_err() {
                unsafe_entries += 1;
            }
            if file.is_symlink() {
                symlink_entries += 1;
            }
            if file.encrypted() {
                encrypted_entries += 1;
            }

            let name = path_inside
                .rsplit('/')
                .next()
                .unwrap_or(&path_inside)
                .to_string();
            let parent = path_inside
                .rfind('/')
                .map(|position| path_inside[..position].to_string())
                .unwrap_or_default();

            let size = file.size();
            let compressed_size = file.compressed_size();
            if !is_dir {
                total_size = total_size.saturating_add(size);
                total_compressed = total_compressed.saturating_add(compressed_size);
            }

            entries.push(ArchiveEntry {
                index,
                path: path_inside,
                name,
                parent,
                is_dir,
                is_symlink: file.is_symlink(),
                size,
                compressed_size,
                ratio: ratio(compressed_size, size),
                modified: file.last_modified().map(|value| value.to_string()),
                encrypted: file.encrypted(),
                crc32: file.crc32(),
                method: method_label(file.compression()).to_string(),
            });
        }

        let comment = {
            let raw = archive.comment();
            let decoded = String::from_utf8_lossy(raw)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if decoded.is_empty() {
                None
            } else {
                Some(decoded)
            }
        };

        let mut warnings = Vec::new();
        if unsafe_entries > 0 {
            warnings.push(format!(
                "{unsafe_entries} archive entr{} use unsafe paths and will be rejected during extraction",
                if unsafe_entries == 1 { "y" } else { "ies" }
            ));
        }
        if symlink_entries > 0 {
            warnings.push(format!(
                "{symlink_entries} symbolic link entr{} detected; links are never created",
                if symlink_entries == 1 { "y" } else { "ies" }
            ));
        }
        if invalid_names > 0 {
            warnings.push(format!(
                "{invalid_names} entr{} have names that are not valid UTF-8",
                if invalid_names == 1 { "y" } else { "ies" }
            ));
        }
        if is_partial_volume_name(path) {
            warnings.push(
                "This looks like a multi-volume part. Multi-volume ZIP is not implemented in Phase 1."
                    .into(),
            );
        }

        Ok(ArchiveListing {
            path: path.display().to_string(),
            format: ArchiveFormat::Zip,
            format_label: ArchiveFormat::Zip.label().to_string(),
            file_size,
            entries,
            total_size,
            total_compressed,
            encrypted_entries,
            comment,
            warnings,
        })
    }

    fn selected_indices(
        &self,
        archive: &mut ZipArchive<File>,
        selected: Option<&[String]>,
    ) -> ArchonResult<Vec<SelectedEntry>> {
        let Some(selected) = selected else {
            let mut all = Vec::with_capacity(archive.len());
            for index in 0..archive.len() {
                let file = archive.by_index_raw(index)?;
                all.push(selected_from_zipfile(index, &file)?);
            }
            return Ok(all);
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
        let mut output = Vec::new();

        for index in 0..archive.len() {
            let file = archive.by_index_raw(index)?;
            let entry = selected_from_zipfile(index, &file)?;
            let path = normalize_archive_path(&entry.path);
            let mut hit = false;
            for (position, candidate) in wanted.iter().enumerate() {
                if path == *candidate || path.starts_with(&format!("{candidate}/")) {
                    matched[position] = true;
                    hit = true;
                }
            }
            if hit {
                output.push(entry);
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

        Ok(output)
    }
}

struct SelectedEntry {
    index: usize,
    path: String,
    is_dir: bool,
    is_symlink: bool,
    size: u64,
    modified: Option<DateTime>,
    encrypted: bool,
}

fn selected_from_zipfile(
    index: usize,
    file: &zip::read::ZipFile<'_, File>,
) -> ArchonResult<SelectedEntry> {
    let raw_name = match file.name() {
        Ok(name) => name.into_owned(),
        Err(_) => String::from_utf8_lossy(file.name_raw()).into_owned(),
    };
    Ok(SelectedEntry {
        index,
        path: raw_name.trim_end_matches('/').to_string(),
        is_dir: file.is_dir() || raw_name.ends_with('/'),
        is_symlink: file.is_symlink(),
        size: file.size(),
        modified: file.last_modified(),
        encrypted: file.encrypted(),
    })
}

impl ArchiveEngine for ZipEngine {
    fn id(&self) -> &'static str {
        "zip-rs"
    }

    fn capabilities(&self) -> Vec<FormatCapability> {
        vec![FormatCapability {
            format: ArchiveFormat::Zip,
            label: "ZIP".into(),
            extensions: vec!["zip".into()],
            can_read: true,
            can_create: true,
            can_modify: false,
            can_test: true,
            encryption: true,
            multi_volume: false,
            status: CapabilityStatus::Supported,
            notes: "Read, create, extract and test. Store or Deflate compression. AES-256 (AE-2) encryption for new archives, and decryption of AES/ZipCrypto archives. Modifying existing archives and split ZIP volumes are not implemented in Phase 1.".into(),
        }]
    }

    fn list(&self, archive: &Path) -> ArchonResult<ArchiveListing> {
        self.read_listing(archive)
    }

    fn info(&self, archive: &Path) -> ArchonResult<ArchiveInfo> {
        let listing = self.read_listing(archive)?;
        let file_count = listing.entries.iter().filter(|entry| !entry.is_dir).count();
        let dir_count = listing.entries.iter().filter(|entry| entry.is_dir).count();
        Ok(ArchiveInfo {
            path: listing.path,
            format: listing.format,
            format_label: listing.format_label,
            archive_size: listing.file_size,
            entry_count: listing.entries.len(),
            file_count,
            dir_count,
            total_size: listing.total_size,
            total_compressed: listing.total_compressed,
            ratio: ratio(listing.total_compressed, listing.total_size),
            encrypted_entries: listing.encrypted_entries,
            comment: listing.comment,
            warnings: listing.warnings,
        })
    }

    fn create(
        &self,
        options: &CreateOptions,
        ctx: &OperationContext,
    ) -> ArchonResult<CreateReport> {
        let started = Instant::now();
        if options.format != ArchiveFormat::Zip {
            return Err(ArchonError::UnsupportedOperation(format!(
                "{} creation is not implemented; Phase 1 supports ZIP creation only",
                options.format.label()
            )));
        }
        if options.sources.is_empty() {
            return Err(ArchonError::Configuration(
                "Select at least one file or folder to compress".into(),
            ));
        }
        if options.output.as_os_str().is_empty() {
            return Err(ArchonError::Configuration(
                "Choose an output archive path".into(),
            ));
        }
        if options.output.is_dir() {
            return Err(ArchonError::Configuration(
                "The output path is a folder; choose a file name".into(),
            ));
        }
        let parent = options
            .output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            return Err(ArchonError::Configuration(format!(
                "The output folder does not exist: {}",
                parent.display()
            )));
        }
        if options.output.exists() && !options.overwrite_output {
            return Err(ArchonError::Conflict(format!(
                "The file already exists: {}. Enable overwrite to replace it.",
                options.output.display()
            )));
        }

        let (method, level, mut warnings) = resolve_compression(options)?;
        let exclude = build_exclude_set(&options.exclude)?;
        let output_absolute = absolute_path(&options.output)?;
        let password = options
            .password
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(|value| Zeroizing::new(value.clone()));

        ctx.check_cancelled()?;
        ctx.report(ProgressPhase::Scanning, 0, 0, None, 0, 0);

        let mut planned = Vec::new();
        let mut names = HashSet::new();
        let mut excluded = 0_usize;
        let mut symlinks = 0_usize;

        for source in &options.sources {
            ctx.check_cancelled()?;
            let metadata = fs::symlink_metadata(source).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ArchonError::NotFound(source.to_path_buf())
                } else {
                    ArchonError::Io(format!("Cannot inspect {}: {error}", source.display()))
                }
            })?;

            if metadata.file_type().is_symlink() {
                symlinks += 1;
                warnings.push(format!(
                    "Skipped symbolic link source: {} (links are not archived in Phase 1)",
                    source.display()
                ));
                continue;
            }

            let canonical = source.canonicalize().map_err(|error| {
                ArchonError::Io(format!("Cannot resolve {}: {error}", source.display()))
            })?;
            let parent = canonical.parent().ok_or_else(|| {
                ArchonError::Configuration(format!(
                    "Cannot archive a drive root directly: {}",
                    source.display()
                ))
            })?;

            for item in WalkDir::new(&canonical)
                .follow_links(false)
                .sort_by_file_name()
            {
                ctx.check_cancelled()?;
                let item = item.map_err(|error| {
                    ArchonError::Io(format!("Cannot scan {}: {error}", source.display()))
                })?;
                let item_path = item.path();

                if paths_equal(item_path, &output_absolute) {
                    continue;
                }

                if item.file_type().is_symlink() {
                    symlinks += 1;
                    continue;
                }

                let relative = item_path.strip_prefix(parent).map_err(|_| {
                    ArchonError::Configuration(format!(
                        "Cannot compute an archive path for {}",
                        item_path.display()
                    ))
                })?;
                let name = archive_name_from_relative(relative)?;
                let is_dir = item.file_type().is_dir();

                if is_excluded(&exclude, &name) {
                    excluded += 1;
                    continue;
                }

                sanitize_entry_path(&name)?;

                let size = if is_dir {
                    0
                } else {
                    item.metadata()
                        .map_err(|error| {
                            ArchonError::Io(format!(
                                "Cannot read metadata for {}: {error}",
                                item_path.display()
                            ))
                        })?
                        .len()
                };

                let modified = item_path
                    .metadata()
                    .ok()
                    .and_then(|value| value.modified().ok())
                    .map(zip_time_from_system_time);

                let key = name.to_lowercase();
                if !names.insert(key) {
                    return Err(ArchonError::Conflict(format!(
                        "Two selected files would be stored at the same archive path: {name}"
                    )));
                }

                planned.push(PlannedEntry {
                    name,
                    source: Some(item_path.to_path_buf()),
                    is_dir,
                    size,
                    modified,
                });
            }
        }

        if planned.is_empty() {
            return Err(ArchonError::Configuration(
                "There are no files to compress after applying filters".into(),
            ));
        }

        planned.sort_by(|left, right| left.name.cmp(&right.name));
        let file_count = planned.iter().filter(|entry| !entry.is_dir).count();
        let dir_count = planned.iter().filter(|entry| entry.is_dir).count();
        let total_size: u64 = planned.iter().map(|entry| entry.size).sum();
        let total_entries = planned.len();

        if excluded > 0 {
            warnings.push(format!(
                "{excluded} item{} excluded by filters",
                if excluded == 1 { "" } else { "s" }
            ));
        }
        if symlinks > 0 {
            warnings.push(format!(
                "{symlinks} symbolic link{} skipped",
                if symlinks == 1 { "" } else { "s" }
            ));
        }

        let (temp_file, temp_path) = create_temp_file(parent)?;
        let mut writer = ZipWriter::new(BufWriter::new(temp_file));
        let mut bytes_done = 0_u64;
        let mut processed = 0_usize;

        ctx.report(
            ProgressPhase::Compressing,
            0,
            total_entries,
            None,
            0,
            total_size,
        );

        let build_result = (|| -> ArchonResult<()> {
            for entry in &planned {
                ctx.check_cancelled()?;

                let mut options = SimpleFileOptions::default()
                    .compression_method(if entry.is_dir {
                        CompressionMethod::Stored
                    } else {
                        method
                    })
                    .last_modified_time(entry.modified.unwrap_or_default())
                    .large_file(entry.size > u32::MAX as u64);

                if !entry.is_dir {
                    options = options.compression_level(level);
                }

                if entry.is_dir {
                    writer.add_directory(&entry.name, options)?;
                } else {
                    let source = entry.source.as_ref().expect("file entry has a source");
                    let mut input = File::open(source).map_err(|error| {
                        ArchonError::Io(format!("Cannot open {}: {error}", source.display()))
                    })?;

                    if let Some(password) = password.as_deref() {
                        let encrypted = options.with_aes_encryption(AesMode::Aes256, password);
                        writer.start_file(&entry.name, encrypted)?;
                    } else {
                        writer.start_file(&entry.name, options)?;
                    }

                    copy_with_progress(
                        &mut input,
                        &mut writer,
                        ctx,
                        ProgressPhase::Compressing,
                        processed + 1,
                        total_entries,
                        &entry.name,
                        &mut bytes_done,
                        total_size,
                    )?;
                }

                processed += 1;
                ctx.report(
                    ProgressPhase::Compressing,
                    processed,
                    total_entries,
                    Some(&entry.name),
                    bytes_done,
                    total_size,
                );
            }
            Ok(())
        })();

        if let Err(error) = build_result {
            drop(writer);
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        ctx.report(
            ProgressPhase::Finalizing,
            total_entries,
            total_entries,
            None,
            bytes_done,
            total_size,
        );

        let buffered = match writer.finish() {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                return Err(ArchonError::from(error));
            }
        };
        let file = match buffered.into_inner() {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                return Err(ArchonError::Io(format!(
                    "Cannot finalise {}: {error}",
                    temp_path.display()
                )));
            }
        };
        if let Err(error) = file.sync_all() {
            drop(file);
            let _ = fs::remove_file(&temp_path);
            return Err(ArchonError::Io(format!(
                "Cannot flush {}: {error}",
                temp_path.display()
            )));
        }
        drop(file);

        if options.output.exists() {
            fs::remove_file(&options.output).map_err(|error| {
                let _ = fs::remove_file(&temp_path);
                ArchonError::Io(format!(
                    "Cannot replace {}: {error}",
                    options.output.display()
                ))
            })?;
        }
        if let Err(error) = fs::rename(&temp_path, &options.output) {
            let _ = fs::remove_file(&temp_path);
            return Err(ArchonError::Io(format!(
                "Cannot move the finished archive to {}: {error}",
                options.output.display()
            )));
        }

        let archive_size = fs::metadata(&options.output)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        Ok(CreateReport {
            output: options.output.display().to_string(),
            format: ArchiveFormat::Zip,
            entry_count: total_entries,
            file_count,
            dir_count,
            total_size,
            archive_size,
            ratio: ratio(archive_size, total_size),
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
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

        let (mut archive, _) = self.open(&options.archive)?;
        let selected = self.selected_indices(&mut archive, options.entries.as_deref())?;
        if selected.len() > self.limits.max_entries {
            return Err(ArchonError::LimitExceeded(format!(
                "This extraction contains {} entries; the safety limit is {}",
                selected.len(),
                self.limits.max_entries
            )));
        }

        let encrypted_entries = selected.iter().filter(|entry| entry.encrypted).count();
        let password = options
            .password
            .as_deref()
            .filter(|value| !value.is_empty());
        if encrypted_entries > 0 && password.is_none() {
            return Err(ArchonError::PasswordRequired);
        }

        let total_size =
            selected
                .iter()
                .filter(|entry| !entry.is_dir)
                .try_fold(0_u64, |sum, entry| {
                    sum.checked_add(entry.size).ok_or_else(|| {
                        ArchonError::LimitExceeded("Archive size metadata overflows".into())
                    })
                })?;
        if total_size > self.limits.max_total_uncompressed {
            return Err(ArchonError::LimitExceeded(format!(
                "This extraction would produce {total_size} bytes; the safety limit is {}",
                self.limits.max_total_uncompressed
            )));
        }
        if let Some(entry) = selected
            .iter()
            .find(|entry| !entry.is_dir && entry.size > self.limits.max_entry_uncompressed)
        {
            return Err(ArchonError::LimitExceeded(format!(
                "Entry {} declares {} bytes, above the per-file limit of {}",
                entry.path, entry.size, self.limits.max_entry_uncompressed
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

        // Validate every target path and resolve collisions *before* writing
        // anything to disk.
        let mut plan = Vec::with_capacity(selected.len());
        let mut file_keys = HashSet::new();
        let mut dir_keys = HashSet::new();
        let mut warnings = Vec::new();
        let mut skipped = 0_usize;
        let mut symlinks = 0_usize;

        for entry in &selected {
            if entry.is_symlink {
                symlinks += 1;
                continue;
            }
            let safe = sanitize_entry_path(&entry.path)?;
            let relative = if options.preserve_structure {
                safe
            } else {
                PathBuf::from(
                    safe.file_name()
                        .ok_or_else(|| ArchonError::PathTraversal {
                            entry: entry.path.clone(),
                        })?
                        .to_owned(),
                )
            };
            let key = archive_path_key(&relative);

            if entry.is_dir {
                dir_keys.insert(key.clone());
            } else {
                if !file_keys.insert(key.clone()) {
                    return Err(ArchonError::Conflict(format!(
                        "Two entries would extract to the same path: {}",
                        relative.display()
                    )));
                }
                let mut ancestor = relative.parent();
                while let Some(parent) = ancestor {
                    if !parent.as_os_str().is_empty()
                        && file_keys.contains(&archive_path_key(parent))
                    {
                        return Err(ArchonError::Conflict(format!(
                            "Archive entry {} conflicts with a file of the same name",
                            relative.display()
                        )));
                    }
                    ancestor = parent.parent();
                }
            }

            if entry.is_dir && file_keys.contains(&archive_path_key(&relative)) {
                return Err(ArchonError::Conflict(format!(
                    "Archive entry {} is both a file and a folder",
                    relative.display()
                )));
            }

            plan.push((entry, relative));
        }

        if symlinks > 0 {
            warnings.push(format!(
                "{symlinks} symbolic link entr{} skipped; Archon never creates links from archives",
                if symlinks == 1 { "y was" } else { "ies were" }
            ));
        }

        let file_total = selected.iter().filter(|entry| !entry.is_dir).count();
        let mut bytes_done = 0_u64;
        let mut extracted_files = 0_usize;
        let mut extracted_dirs = 0_usize;
        let mut processed = 0_usize;

        ctx.report(
            ProgressPhase::Extracting,
            0,
            file_total,
            None,
            0,
            total_size,
        );

        for (entry, relative) in plan {
            ctx.check_cancelled()?;
            let target = safe_join(&destination, &relative)?;

            if entry.is_dir {
                if options.preserve_structure {
                    fs::create_dir_all(&target).map_err(|error| {
                        ArchonError::Io(format!("Cannot create {}: {error}", target.display()))
                    })?;
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

            let mut final_path = target.clone();
            let mut decision = OverwritePolicy::Overwrite;
            if final_path.exists() {
                if options.overwrite == OverwritePolicy::Skip {
                    skipped += 1;
                    ctx.report(
                        ProgressPhase::Extracting,
                        processed,
                        file_total,
                        Some(&entry.path),
                        bytes_done,
                        total_size,
                    );
                    continue;
                }
                if final_path.is_dir() {
                    return Err(ArchonError::Conflict(format!(
                        "Cannot overwrite the folder {} with a file",
                        final_path.display()
                    )));
                }
                decision = options.overwrite;
                if decision == OverwritePolicy::Rename {
                    let name = final_path
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".to_string());
                    final_path = crate::fsutil::unique_path(parent, &name);
                }
            }

            let mut reader = if entry.encrypted {
                let password = password.ok_or(ArchonError::PasswordRequired)?;
                archive.by_index_decrypt(entry.index, password.as_bytes())?
            } else {
                archive.by_index(entry.index)?
            };

            let (temp_file, temp_path) = create_temp_file(parent)?;
            let mut temp_file = temp_file;
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
            drop(reader);
            drop(temp_file);

            if decision == OverwritePolicy::Overwrite && final_path.exists() {
                if let Err(error) = fs::remove_file(&final_path) {
                    let _ = fs::remove_file(&temp_path);
                    return Err(ArchonError::Io(format!(
                        "Cannot replace {}: {error}",
                        final_path.display()
                    )));
                }
            }
            if let Err(error) = fs::rename(&temp_path, &final_path) {
                let _ = fs::remove_file(&temp_path);
                return Err(ArchonError::Io(format!(
                    "Cannot write {}: {error}",
                    final_path.display()
                )));
            }

            apply_modified_time(&final_path, entry.modified);
            extracted_files += 1;
            ctx.report(
                ProgressPhase::Extracting,
                processed,
                file_total,
                Some(&entry.path),
                bytes_done,
                total_size,
            );
        }

        if warnings.len() > 100 {
            warnings.truncate(100);
            warnings.push("Additional warnings were omitted".into());
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
        archive_path: &Path,
        password: Option<&str>,
        ctx: &OperationContext,
    ) -> ArchonResult<TestReport> {
        let started = Instant::now();
        let (mut archive, _) = self.open(archive_path)?;
        let password = password.filter(|value| !value.is_empty());

        let mut entries = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let file = archive.by_index_raw(index)?;
            entries.push(selected_from_zipfile(index, &file)?);
        }

        if entries.len() > self.limits.max_entries {
            return Err(ArchonError::LimitExceeded(format!(
                "This archive has {} entries; the safety limit is {}",
                entries.len(),
                self.limits.max_entries
            )));
        }

        let total_size =
            entries
                .iter()
                .filter(|entry| !entry.is_dir)
                .try_fold(0_u64, |sum, entry| {
                    sum.checked_add(entry.size).ok_or_else(|| {
                        ArchonError::LimitExceeded("Archive size metadata overflows".into())
                    })
                })?;
        if total_size > self.limits.max_total_uncompressed {
            return Err(ArchonError::LimitExceeded(format!(
                "This archive declares {total_size} uncompressed bytes; the safety limit is {}",
                self.limits.max_total_uncompressed
            )));
        }

        let encrypted_entries = entries.iter().filter(|entry| entry.encrypted).count();
        if encrypted_entries > 0 && password.is_none() {
            return Err(ArchonError::PasswordRequired);
        }

        if let Some(first_encrypted) = entries.iter().find(|entry| entry.encrypted) {
            let password = password.ok_or(ArchonError::PasswordRequired)?;
            match archive.by_index_decrypt(first_encrypted.index, password.as_bytes()) {
                Ok(_) => {}
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err(ArchonError::InvalidPassword);
                }
                Err(error) => return Err(ArchonError::from(error)),
            }
        }

        let mut issues = Vec::new();
        let mut checked = 0_usize;
        let mut bytes_checked = 0_u64;
        let file_total = entries.iter().filter(|entry| !entry.is_dir).count();
        let mut processed = 0_usize;

        ctx.report(ProgressPhase::Testing, 0, file_total, None, 0, total_size);

        for entry in entries.iter().filter(|entry| !entry.is_dir) {
            ctx.check_cancelled()?;
            processed += 1;

            let mut reader = if entry.encrypted {
                let password = password.ok_or(ArchonError::PasswordRequired)?;
                match archive.by_index_decrypt(entry.index, password.as_bytes()) {
                    Ok(reader) => reader,
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err(ArchonError::InvalidPassword);
                    }
                    Err(error) => return Err(ArchonError::from(error)),
                }
            } else {
                archive.by_index(entry.index)?
            };

            let mut sink = std::io::sink();
            let result = copy_with_progress(
                &mut reader,
                &mut sink,
                ctx,
                ProgressPhase::Testing,
                processed,
                file_total,
                &entry.path,
                &mut bytes_checked,
                total_size,
            );

            match result {
                Ok(()) => checked += 1,
                Err(ArchonError::Cancelled) => return Err(ArchonError::Cancelled),
                Err(ArchonError::InvalidPassword) => return Err(ArchonError::InvalidPassword),
                Err(error) => {
                    checked += 1;
                    if issues.len() < self.limits.max_reported_issues {
                        issues.push(TestIssue {
                            entry: entry.path.clone(),
                            kind: error.code().to_string(),
                            message: error.to_string(),
                        });
                    }
                }
            }

            ctx.report(
                ProgressPhase::Testing,
                processed,
                file_total,
                Some(&entry.path),
                bytes_checked,
                total_size,
            );
        }

        let mut warnings = Vec::new();
        if issues.len() >= self.limits.max_reported_issues {
            warnings.push(format!(
                "Stopped reporting after {} issues",
                self.limits.max_reported_issues
            ));
        }

        Ok(TestReport {
            ok: issues.is_empty(),
            entries_checked: checked,
            bytes_checked,
            encrypted_entries,
            issues,
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
    }
}

struct PlannedEntry {
    name: String,
    source: Option<PathBuf>,
    is_dir: bool,
    size: u64,
    modified: Option<DateTime>,
}

fn resolve_compression(
    options: &CreateOptions,
) -> ArchonResult<(CompressionMethod, Option<i64>, Vec<String>)> {
    let mut warnings = Vec::new();
    match options.method {
        Some(ZipMethod::Store) => {
            if options.level.is_some() {
                return Err(ArchonError::Configuration(
                    "Compression level is not configurable when using Store".into(),
                ));
            }
            Ok((CompressionMethod::Stored, None, warnings))
        }
        Some(ZipMethod::Deflate) => {
            let level = options.level.unwrap_or(6);
            if !(1..=9).contains(&level) {
                return Err(ArchonError::Configuration(
                    "Deflate level must be between 1 and 9".into(),
                ));
            }
            Ok((CompressionMethod::Deflated, Some(level), warnings))
        }
        None => match options.preset {
            CompressionPreset::Store => Ok((CompressionMethod::Stored, None, warnings)),
            CompressionPreset::Fast => Ok((CompressionMethod::Deflated, Some(1), warnings)),
            CompressionPreset::Normal => Ok((CompressionMethod::Deflated, Some(6), warnings)),
            CompressionPreset::Maximum => Ok((CompressionMethod::Deflated, Some(9), warnings)),
            CompressionPreset::Ultra => {
                warnings
                    .push("Ultra maps to Deflate level 9; no higher ZIP tier is available".into());
                Ok((CompressionMethod::Deflated, Some(9), warnings))
            }
        },
    }
}

fn build_exclude_set(patterns: &[String]) -> ArchonResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        let glob = Glob::new(trimmed).map_err(|error| {
            ArchonError::Configuration(format!("Invalid exclude pattern '{trimmed}': {error}"))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|error| ArchonError::Configuration(format!("Invalid exclude patterns: {error}")))
}

fn is_excluded(exclude: &GlobSet, name: &str) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let normalized = name.replace('\\', "/");
    let file_name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    exclude.is_match(normalized.as_str()) || exclude.is_match(file_name)
}

fn absolute_path(path: &Path) -> ArchonResult<PathBuf> {
    std::path::absolute(path)
        .map_err(|error| ArchonError::Io(format!("Cannot resolve {}: {error}", path.display())))
}

fn method_label(method: CompressionMethod) -> &'static str {
    match method {
        CompressionMethod::Stored => "Store",
        CompressionMethod::Deflated => "Deflate",
        CompressionMethod::Aes => "AES",
        _ => "Other",
    }
}

fn is_partial_volume_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if let Some(extension) = name.rsplit('.').next() {
        if let Some(digits) = extension.strip_prefix('z') {
            return digits.len() == 2 && digits.chars().all(|value| value.is_ascii_digit());
        }
        if let Some(digits) = extension.strip_prefix("part") {
            return digits.len() == 3 && digits.chars().all(|value| value.is_ascii_digit());
        }
    }
    false
}
