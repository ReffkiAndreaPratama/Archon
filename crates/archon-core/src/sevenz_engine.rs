//! Read-only 7-Zip engine built on `sevenz-rust2`.
//!
//! Listing, extraction and integrity testing are implemented for 7z archives,
//! including AES-256 encrypted archives. Creating or modifying 7z archives is
//! intentionally not implemented, and capabilities report that honestly.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sevenz_rust2::{
    ArchiveEntry as SevenEntry, ArchiveReader, EncoderMethod, Error as SevenError, Password,
};

use crate::engine::ArchiveEngine;
use crate::error::{ArchonError, ArchonResult};
use crate::fsutil::{available_space, copy_with_progress, create_temp_file, ratio, unique_path};
use crate::model::{
    ArchiveEntry, ArchiveFormat, ArchiveInfo, ArchiveListing, CapabilityStatus, CreateOptions,
    CreateReport, ExtractOptions, ExtractReport, FormatCapability, Limits, OverwritePolicy,
    TestIssue, TestReport,
};
use crate::progress::{OperationContext, ProgressPhase};
use crate::security::{normalize_archive_path, path_key, safe_join, sanitize_entry_path};

pub struct SevenZEngine {
    limits: Limits,
}

impl Default for SevenZEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SevenZEngine {
    pub fn new() -> Self {
        Self {
            limits: Limits::default(),
        }
    }

    fn password(value: Option<&str>) -> Password {
        match value {
            Some(value) if !value.is_empty() => Password::new(value),
            _ => Password::empty(),
        }
    }

    fn open(&self, path: &Path, password: Option<&str>) -> ArchonResult<ArchiveReader<fs::File>> {
        ArchiveReader::open(path, Self::password(password)).map_err(|error| map_error(error, path))
    }

    fn read_listing(&self, path: &Path, password: Option<&str>) -> ArchonResult<ArchiveListing> {
        let reader = self.open(path, password)?;
        let archive = reader.archive();
        let file_size = fs::metadata(path).map(|value| value.len()).unwrap_or(0);

        let mut entries = Vec::with_capacity(archive.files.len());
        let mut total_size = 0_u64;
        let mut total_compressed = 0_u64;
        let mut encrypted_entries = 0_usize;
        let mut unsafe_entries = 0_usize;
        let mut duplicates = 0_usize;
        let mut seen = HashSet::new();
        let mut methods: Vec<EncoderMethod> = Vec::new();

        for file in &archive.files {
            if file.is_anti_item {
                continue;
            }
            let raw = normalize_archive_path(&file.name);
            if raw.is_empty() {
                continue;
            }
            if entries.len() >= self.limits.max_entries {
                return Err(ArchonError::LimitExceeded(format!(
                    "This archive contains more than {} entries; the safety limit was reached",
                    self.limits.max_entries
                )));
            }
            if sanitize_entry_path(&raw).is_err() {
                unsafe_entries += 1;
            }
            if !seen.insert(raw.to_ascii_lowercase()) {
                duplicates += 1;
            }

            let is_dir = file.is_directory || file.name.ends_with('/');
            let size = if is_dir { 0 } else { file.size };
            if size > self.limits.max_entry_uncompressed {
                return Err(ArchonError::LimitExceeded(format!(
                    "Entry {raw} declares {size} bytes, above the per-file limit of {}",
                    self.limits.max_entry_uncompressed
                )));
            }

            let mut encrypted = false;
            if file.has_stream && !is_dir {
                methods.clear();
                if reader
                    .file_compression_methods(&file.name, &mut methods)
                    .is_ok()
                    && methods
                        .iter()
                        .any(|method| *method == EncoderMethod::AES256_SHA256)
                {
                    encrypted = true;
                    encrypted_entries += 1;
                }
            }

            let name = raw.rsplit('/').next().unwrap_or(&raw).to_string();
            let parent = raw
                .rfind('/')
                .map(|position| raw[..position].to_string())
                .unwrap_or_default();
            let compressed_size = if is_dir { 0 } else { file.compressed_size };
            if !is_dir {
                total_size = total_size.saturating_add(size);
                total_compressed = total_compressed.saturating_add(compressed_size);
            }

            entries.push(ArchiveEntry {
                index: entries.len(),
                path: raw,
                name,
                parent,
                is_dir,
                is_symlink: false,
                size,
                compressed_size,
                ratio: ratio(compressed_size, size),
                modified: if file.has_last_modified_date {
                    format_system_time(SystemTime::from(file.last_modified_date))
                } else {
                    None
                },
                encrypted,
                crc32: if file.has_crc { file.crc as u32 } else { 0 },
                method: if is_dir {
                    "Store".to_string()
                } else {
                    method_label(&methods)
                },
            });
        }

        let mut warnings = Vec::new();
        if archive.is_solid {
            warnings.push(
                "This is a solid 7z archive: extracting one file may require decompressing the data before it."
                    .into(),
            );
        }
        if unsafe_entries > 0 {
            warnings.push(format!(
                "{unsafe_entries} archive entr{} use unsafe paths and will be rejected during extraction",
                if unsafe_entries == 1 { "y" } else { "ies" }
            ));
        }
        if duplicates > 0 {
            warnings.push(format!(
                "{duplicates} duplicate entr{} detected; extraction will reject colliding paths",
                if duplicates == 1 { "y" } else { "ies" }
            ));
        }
        if encrypted_entries > 0 && password.is_none() {
            warnings.push(
                "This archive contains encrypted data. A password is needed to extract or verify it."
                    .into(),
            );
        }

        Ok(ArchiveListing {
            path: path.display().to_string(),
            format: ArchiveFormat::SevenZ,
            format_label: ArchiveFormat::SevenZ.label().to_string(),
            file_size,
            entries,
            total_size,
            total_compressed: if total_compressed == 0 {
                file_size
            } else {
                total_compressed
            },
            encrypted_entries,
            comment: None,
            warnings,
        })
    }

    fn selected_plan(
        &self,
        listing: &ArchiveListing,
        selected: Option<&[String]>,
    ) -> ArchonResult<Vec<PlanEntry>> {
        let chosen: Vec<&ArchiveEntry> = if let Some(selected) = selected {
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
                    chosen.push(entry);
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
            chosen
        } else {
            listing.entries.iter().collect()
        };

        let mut plan = Vec::with_capacity(chosen.len());
        let mut file_keys = HashSet::new();
        let mut dir_keys = HashSet::new();
        for entry in chosen {
            let safe = sanitize_entry_path(&entry.path)?;
            if safe.as_os_str().is_empty() {
                return Err(ArchonError::PathTraversal {
                    entry: entry.path.clone(),
                });
            }
            let key = path_key(&safe);
            if entry.is_dir {
                if file_keys.contains(&key) {
                    return Err(ArchonError::Conflict(format!(
                        "Archive entry {} is both a file and a folder",
                        safe.display()
                    )));
                }
                dir_keys.insert(key);
            } else {
                if dir_keys.contains(&key) || !file_keys.insert(key.clone()) {
                    return Err(ArchonError::Conflict(format!(
                        "Two entries would extract to the same path: {}",
                        safe.display()
                    )));
                }
                let mut ancestor = safe.parent();
                while let Some(parent) = ancestor {
                    if !parent.as_os_str().is_empty() && file_keys.contains(&path_key(parent)) {
                        return Err(ArchonError::Conflict(format!(
                            "Archive entry {} conflicts with a file of the same name",
                            safe.display()
                        )));
                    }
                    ancestor = parent.parent();
                }
            }
            plan.push(PlanEntry {
                archive_path: normalize_archive_path(&entry.path),
                relative: safe,
                is_dir: entry.is_dir,
                size: entry.size,
                modified: entry.modified.clone(),
            });
        }
        Ok(plan)
    }
}

#[derive(Debug, Clone)]
struct PlanEntry {
    archive_path: String,
    relative: PathBuf,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
}

impl ArchiveEngine for SevenZEngine {
    fn id(&self) -> &'static str {
        "sevenz"
    }

    fn capabilities(&self) -> Vec<FormatCapability> {
        vec![FormatCapability {
            format: ArchiveFormat::SevenZ,
            label: ArchiveFormat::SevenZ.label().to_string(),
            extensions: ArchiveFormat::SevenZ
                .extensions()
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            can_read: true,
            can_create: false,
            can_modify: false,
            can_test: true,
            encryption: true,
            multi_volume: false,
            status: CapabilityStatus::ReadOnly,
            notes: "7z listing, extraction and integrity testing are implemented, including AES-256 encrypted archives. Creating or modifying 7z archives is not implemented.".into(),
        }]
    }

    fn list(&self, archive: &Path, password: Option<&str>) -> ArchonResult<ArchiveListing> {
        self.read_listing(archive, password)
    }

    fn info(&self, archive: &Path, password: Option<&str>) -> ArchonResult<ArchiveInfo> {
        let listing = self.read_listing(archive, password)?;
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
            encrypted_entries: listing.encrypted_entries,
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
            "Creating 7z archives is not implemented. Archon creates ZIP archives only.".into(),
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

        let listing = self.read_listing(&options.archive, options.password.as_deref())?;
        let mut plan = self.selected_plan(&listing, options.entries.as_deref())?;
        if plan.len() > self.limits.max_entries {
            return Err(ArchonError::LimitExceeded(format!(
                "This extraction contains {} entries; the safety limit is {}",
                plan.len(),
                self.limits.max_entries
            )));
        }
        let total_size = plan.iter().try_fold(0_u64, |sum, entry| {
            sum.checked_add(entry.size)
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
        if !options.preserve_structure {
            for entry in plan.iter_mut() {
                let name = entry
                    .relative
                    .file_name()
                    .ok_or_else(|| ArchonError::PathTraversal {
                        entry: entry.relative.display().to_string(),
                    })?
                    .to_owned();
                entry.relative = PathBuf::from(name);
            }
            let mut seen = HashSet::new();
            for entry in &plan {
                if !seen.insert(path_key(&entry.relative)) {
                    return Err(ArchonError::Conflict(format!(
                        "Two entries would extract to the same file: {}",
                        entry.relative.display()
                    )));
                }
            }
        }

        let file_total = plan.iter().filter(|entry| !entry.is_dir).count();
        let mut by_path: HashMap<String, usize> = HashMap::new();
        for (position, entry) in plan.iter().enumerate() {
            if by_path
                .insert(entry.archive_path.clone(), position)
                .is_some()
            {
                return Err(ArchonError::Conflict(format!(
                    "Two archive entries share the path {}",
                    entry.archive_path
                )));
            }
        }

        ctx.report(
            ProgressPhase::Extracting,
            0,
            file_total,
            None,
            0,
            total_size,
        );

        let mut reader = self.open(&options.archive, options.password.as_deref())?;
        let mut failure: Option<ArchonError> = None;
        let mut done: HashSet<usize> = HashSet::new();
        let mut bytes_done = 0_u64;
        let mut processed = 0_usize;
        let mut extracted_files = 0_usize;
        let mut extracted_dirs = 0_usize;
        let mut skipped = 0_usize;

        let decoded = reader.for_each_entries(|entry, source| {
            if failure.is_some() {
                return Ok(false);
            }
            if let Err(error) = ctx.check_cancelled() {
                failure = Some(error);
                return Ok(false);
            }
            let key = normalize_archive_path(&entry.name);
            let Some(&position) = by_path.get(&key) else {
                return Ok(true);
            };
            if !done.insert(position) {
                return Ok(true);
            }
            let plan_entry = &plan[position];
            let target = match safe_join(&destination, &plan_entry.relative) {
                Ok(value) => value,
                Err(error) => {
                    failure = Some(error);
                    return Ok(false);
                }
            };

            if plan_entry.is_dir {
                if options.preserve_structure {
                    if let Err(error) = fs::create_dir_all(&target) {
                        failure = Some(ArchonError::Io(format!(
                            "Cannot create {}: {error}",
                            target.display()
                        )));
                        return Ok(false);
                    }
                    apply_system_time(&target, plan_entry.modified.as_deref());
                    extracted_dirs += 1;
                }
                return Ok(true);
            }

            processed += 1;
            let parent = match target.parent() {
                Some(value) => value,
                None => {
                    failure = Some(ArchonError::Configuration(format!(
                        "Cannot determine the parent folder for {}",
                        target.display()
                    )));
                    return Ok(false);
                }
            };
            if let Err(error) = fs::create_dir_all(parent) {
                failure = Some(ArchonError::Io(format!(
                    "Cannot create {}: {error}",
                    parent.display()
                )));
                return Ok(false);
            }
            let final_path = match resolve_overwrite(&target, options.overwrite, &mut skipped) {
                Ok(Some(value)) => value,
                Ok(None) => return Ok(true),
                Err(error) => {
                    failure = Some(error);
                    return Ok(false);
                }
            };
            let (mut temp_file, temp_path) = match create_temp_file(parent) {
                Ok(value) => value,
                Err(error) => {
                    failure = Some(error);
                    return Ok(false);
                }
            };
            let bytes_before = bytes_done;
            let copy_result = copy_with_progress(
                source,
                &mut temp_file,
                ctx,
                ProgressPhase::Extracting,
                processed,
                file_total,
                &plan_entry.archive_path,
                &mut bytes_done,
                total_size,
            )
            .and_then(|_| {
                std::io::Write::flush(&mut temp_file).map_err(|error| {
                    ArchonError::Io(format!("Cannot flush {}: {error}", plan_entry.archive_path))
                })
            });
            if let Err(error) = copy_result {
                drop(temp_file);
                let _ = fs::remove_file(&temp_path);
                failure = Some(error);
                return Ok(false);
            }
            drop(temp_file);
            let entry_bytes = bytes_done.saturating_sub(bytes_before);
            if entry_bytes > self.limits.max_entry_uncompressed {
                let _ = fs::remove_file(&temp_path);
                failure = Some(ArchonError::LimitExceeded(format!(
                    "Entry {} produced {entry_bytes} bytes, above the per-file limit of {}",
                    plan_entry.archive_path, self.limits.max_entry_uncompressed
                )));
                return Ok(false);
            }
            if let Err(error) = finish_file(&temp_path, &final_path, options.overwrite) {
                failure = Some(error);
                return Ok(false);
            }
            apply_system_time(&final_path, plan_entry.modified.as_deref());
            extracted_files += 1;
            Ok(true)
        });

        if let Some(error) = failure {
            return Err(error);
        }
        decoded.map_err(|error| map_error(error, &options.archive))?;

        if extracted_dirs == 0 && options.preserve_structure {
            for entry in plan.iter().filter(|entry| !entry.is_dir) {
                if let Some(parent) = entry.relative.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(destination.join(parent));
                    }
                }
            }
        }
        if skipped > 0 {
            warnings.push(format!(
                "{skipped} existing file{} skipped because the overwrite policy is set to skip",
                if skipped == 1 { " was" } else { "s were" }
            ));
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
        password: Option<&str>,
        ctx: &OperationContext,
    ) -> ArchonResult<TestReport> {
        let started = Instant::now();
        ctx.check_cancelled()?;
        let listing = self.read_listing(archive, password)?;
        let mut reader = self.open(archive, password)?;
        let mut failure: Option<ArchonError> = None;
        let mut issues = Vec::new();
        let mut entries_checked = 0_usize;
        let mut bytes_checked = 0_u64;
        let total = listing.entries.len();
        let total_size = listing.total_size;

        let decoded = reader.for_each_entries(|entry, source| {
            if failure.is_some() {
                return Ok(false);
            }
            if let Err(error) = ctx.check_cancelled() {
                failure = Some(error);
                return Ok(false);
            }
            let is_dir = entry.is_directory || entry.name.ends_with('/');
            if is_dir {
                entries_checked += 1;
                return Ok(true);
            }
            let mut sink = std::io::sink();
            let mut bytes = 0_u64;
            match copy_with_progress(
                source,
                &mut sink,
                ctx,
                ProgressPhase::Testing,
                entries_checked + 1,
                total,
                &entry.name,
                &mut bytes,
                total_size,
            ) {
                Ok(()) => {
                    entries_checked += 1;
                    bytes_checked = bytes_checked.saturating_add(bytes);
                    if entry.has_stream && entry.size != bytes {
                        if issues.len() < self.limits.max_reported_issues {
                            issues.push(TestIssue {
                                entry: entry.name.clone(),
                                kind: "size_mismatch".into(),
                                message: format!(
                                    "Expected {} bytes, read {bytes} bytes",
                                    entry.size
                                ),
                            });
                        }
                    }
                }
                Err(ArchonError::Cancelled) => {
                    failure = Some(ArchonError::Cancelled);
                    return Ok(false);
                }
                Err(error) => {
                    if issues.len() < self.limits.max_reported_issues {
                        issues.push(TestIssue {
                            entry: entry.name.clone(),
                            kind: "read_error".into(),
                            message: error.to_string(),
                        });
                    }
                    return Ok(false);
                }
            }
            Ok(true)
        });

        if let Some(error) = failure {
            return Err(error);
        }
        if let Err(error) = decoded {
            let mapped = map_error(error, archive);
            if matches!(
                mapped,
                ArchonError::PasswordRequired | ArchonError::InvalidPassword
            ) {
                return Err(mapped);
            }
            if issues.len() < self.limits.max_reported_issues {
                issues.push(TestIssue {
                    entry: "<archive>".into(),
                    kind: "archive_error".into(),
                    message: mapped.to_string(),
                });
            }
        }

        let mut warnings = listing.warnings;
        if issues.len() >= self.limits.max_reported_issues {
            warnings.push("The issue list was truncated at the safety limit".into());
        }
        Ok(TestReport {
            ok: issues.is_empty(),
            entries_checked,
            bytes_checked,
            encrypted_entries: listing.encrypted_entries,
            issues,
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
    }
}

fn method_label(methods: &[EncoderMethod]) -> String {
    const FILTERS: &[EncoderMethod] = &[
        EncoderMethod::BCJ_X86_FILTER,
        EncoderMethod::BCJ_PPC_FILTER,
        EncoderMethod::BCJ_IA64_FILTER,
        EncoderMethod::BCJ_ARM_FILTER,
        EncoderMethod::BCJ_ARM64_FILTER,
        EncoderMethod::BCJ_ARM_THUMB_FILTER,
        EncoderMethod::BCJ_SPARC_FILTER,
        EncoderMethod::BCJ2_FILTER,
        EncoderMethod::DELTA_FILTER,
    ];
    for method in methods {
        if *method != EncoderMethod::AES256_SHA256 && !FILTERS.contains(method) {
            return method.name().to_string();
        }
    }
    "7z".to_string()
}

fn format_system_time(value: SystemTime) -> Option<String> {
    let seconds = value.duration_since(UNIX_EPOCH).ok()?.as_secs();
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

fn apply_system_time(path: &Path, value: Option<&str>) {
    let Some(value) = value else {
        return;
    };
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

fn resolve_overwrite(
    target: &Path,
    policy: OverwritePolicy,
    skipped: &mut usize,
) -> ArchonResult<Option<PathBuf>> {
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

fn finish_file(temp_path: &Path, final_path: &Path, policy: OverwritePolicy) -> ArchonResult<()> {
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

fn map_error(error: SevenError, path: &Path) -> ArchonError {
    match error {
        SevenError::PasswordRequired => ArchonError::PasswordRequired,
        SevenError::MaybeBadPassword(_) => ArchonError::InvalidPassword,
        SevenError::BadSignature(_) => ArchonError::InvalidArchive(format!(
            "{} does not contain a valid 7z signature",
            path.display()
        )),
        SevenError::UnsupportedVersion { major, minor } => ArchonError::InvalidArchive(format!(
            "{} uses unsupported 7z version {major}.{minor}",
            path.display()
        )),
        SevenError::ChecksumVerificationFailed => ArchonError::InvalidArchive(format!(
            "Checksum verification failed in {}",
            path.display()
        )),
        SevenError::NextHeaderCrcMismatch => ArchonError::InvalidArchive(format!(
            "The 7z header checksum does not match in {}",
            path.display()
        )),
        SevenError::UnsupportedCompressionMethod(method) => ArchonError::UnsupportedOperation(
            format!("This 7z archive uses unsupported compression method {method}"),
        ),
        SevenError::MaxMemLimited { max_kb, actaul_kb } => ArchonError::LimitExceeded(format!(
            "7z decompression needs {actaul_kb} KiB of memory, above the engine limit of {max_kb} KiB"
        )),
        SevenError::ExternalUnsupported => ArchonError::UnsupportedOperation(
            "7z archives with external header streams are not supported".into(),
        ),
        SevenError::Unsupported(message) => ArchonError::UnsupportedOperation(message.into_owned()),
        SevenError::FileNotFound => ArchonError::InvalidArchive(format!(
            "The 7z archive {} is missing a referenced file stream",
            path.display()
        )),
        SevenError::FileOpen(error, message) => {
            ArchonError::Io(format!("Cannot open {message}: {error}"))
        }
        SevenError::Io(error, message) => ArchonError::Io(format!(
            "7z read failed for {} ({message}): {error}",
            path.display()
        )),
        SevenError::Other(message) => {
            ArchonError::Engine(format!("7z engine error for {}: {message}", path.display()))
        }
        other => ArchonError::InvalidArchive(format!(
            "{} could not be read as a 7z archive: {other}",
            path.display()
        )),
    }
}

#[allow(dead_code)]
fn _entry_name(entry: &SevenEntry) -> &str {
    &entry.name
}
