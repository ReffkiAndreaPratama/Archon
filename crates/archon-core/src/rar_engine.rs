//! Read-only RAR and RAR5 engine built on `rars`.
//!
//! Listing, extraction and integrity testing are implemented for single-volume
//! RAR/RAR5 archives, including password-protected archives. Creating or
//! modifying RAR archives is intentionally not implemented: RAR is a
//! proprietary format and Archon only reads it.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rars::{
    Archive, ArchiveMemberDetail, ArchiveReadOptions, ArchiveReader, Error as RarError,
    ErrorKind as RarErrorKind, ReadCancellation,
};

use crate::engine::ArchiveEngine;
use crate::error::{ArchonError, ArchonResult};
use crate::fsutil::{available_space, create_temp_file, ratio, unique_path};
use crate::model::{
    ArchiveEntry, ArchiveFormat, ArchiveInfo, ArchiveListing, CapabilityStatus, CreateOptions,
    CreateReport, ExtractOptions, ExtractReport, FormatCapability, Limits, OverwritePolicy,
    TestIssue, TestReport,
};
use crate::progress::{OperationContext, ProgressPhase};
use crate::security::{normalize_archive_path, path_key, safe_join, sanitize_entry_path};

const REPORT_INTERVAL: u64 = 1024 * 1024;
const MULTI_VOLUME_MESSAGE: &str = "This is a multi-volume RAR set. Archon can list the first volume, but extracting or testing a split set is not supported yet. Extract it with WinRAR or UnRAR for now.";

pub struct RarEngine {
    limits: Limits,
}

impl Default for RarEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RarEngine {
    pub fn new() -> Self {
        Self {
            limits: Limits::default(),
        }
    }

    fn open(&self, path: &Path, password: Option<&str>) -> ArchonResult<Archive> {
        let password = password.filter(|value| !value.is_empty());
        let result = ArchiveReader::read_path_with_options(
            path,
            ArchiveReadOptions::with_optional_password(password.map(str::as_bytes)),
        );
        match result {
            Ok(archive) => Ok(archive),
            Err(error) => {
                // RAR 1.5-4.x header decryption validates the CRC of the
                // decrypted header, so a wrong password surfaces as a checksum
                // or invalid-header error. Probe without a password: if the
                // archive then reports that a password is required, the
                // failure came from trying the wrong password, not corruption.
                if password.is_some() {
                    let probe = ArchiveReader::read_path_with_options(
                        path,
                        ArchiveReadOptions::with_optional_password(None),
                    );
                    if matches!(
                        probe,
                        Err(ref probe_error)
                            if probe_error.kind() == RarErrorKind::PasswordRequired
                    ) {
                        return Err(ArchonError::InvalidPassword);
                    }
                }
                Err(map_error(error, path))
            }
        }
    }

    /// Reads metadata plus the facts extraction needs before it writes anything.
    fn inspect(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> ArchonResult<(ArchiveListing, bool, bool)> {
        let archive = self.open(path, password)?;
        let file_size = fs::metadata(path).map(|value| value.len()).unwrap_or(0);
        let family = archive.family();
        let format = match family {
            rars::ArchiveFamily::Rar50Plus => ArchiveFormat::Rar5,
            _ => ArchiveFormat::Rar,
        };

        let mut entries = Vec::new();
        let mut total_size = 0_u64;
        let mut total_compressed = 0_u64;
        let mut encrypted_entries = 0_usize;
        let mut unsafe_entries = 0_usize;
        let mut duplicates = 0_usize;
        let mut redirections = 0_usize;
        let mut has_split = false;
        let mut solid = false;
        let mut seen = HashSet::new();

        for member in archive.members() {
            let meta = &member.meta;
            if meta.is_redirection {
                redirections += 1;
                continue;
            }
            if meta.is_split_before || meta.is_split_after {
                has_split = true;
            }
            if member.is_solid() {
                solid = true;
            }

            let raw = normalize_archive_path(&meta.name_lossy());
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

            let is_dir = meta.is_directory;
            let size = if is_dir { 0 } else { meta.unpacked_size };
            if size > self.limits.max_entry_uncompressed {
                return Err(ArchonError::LimitExceeded(format!(
                    "Entry {raw} declares {size} bytes, above the per-file limit of {}",
                    self.limits.max_entry_uncompressed
                )));
            }
            if meta.is_encrypted && !is_dir {
                encrypted_entries += 1;
            }

            let compressed_size = if is_dir { 0 } else { meta.packed_size };
            if !is_dir {
                total_size = total_size.saturating_add(size);
                total_compressed = total_compressed.saturating_add(compressed_size);
            }

            let (crc32, method) = entry_detail(&member);
            let name = raw.rsplit('/').next().unwrap_or(&raw).to_string();
            let parent = raw
                .rfind('/')
                .map(|position| raw[..position].to_string())
                .unwrap_or_default();

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
                modified: meta.modification_time().and_then(format_system_time),
                encrypted: meta.is_encrypted,
                crc32,
                method,
            });
        }

        let mut warnings = Vec::new();
        if solid {
            warnings.push(
                "This is a solid RAR archive: extracting one file may require decompressing the data before it."
                    .into(),
            );
        }
        if has_split {
            warnings.push(MULTI_VOLUME_MESSAGE.into());
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
        if redirections > 0 {
            warnings.push(format!(
                "{redirections} symbolic link or redirection entr{} are listed by neither this view nor extraction; they are skipped to keep paths inside the destination",
                if redirections == 1 { "y" } else { "ies" }
            ));
        }

        Ok((
            ArchiveListing {
                path: path.display().to_string(),
                format,
                format_label: format.label().to_string(),
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
            },
            has_split,
            solid,
        ))
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

struct StagedFile {
    temp: PathBuf,
    final_path: PathBuf,
    modified: Option<String>,
}

/// Writes one member into a staging file while reporting progress and honouring
/// cancellation. Publication happens only after `rars` has verified the archive.
struct StagingWriter {
    file: File,
    path: String,
    ctx: OperationContext,
    written: u64,
    bytes_total: u64,
    bytes_done: Arc<AtomicU64>,
    current: usize,
    total: usize,
    since_report: u64,
}

impl Write for StagingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.ctx.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        let written = self.file.write(buf)?;
        self.written += written as u64;
        self.since_report += written as u64;
        let total = self.bytes_done.fetch_add(written as u64, Ordering::Relaxed) + written as u64;
        if self.since_report >= REPORT_INTERVAL {
            self.ctx.report(
                ProgressPhase::Extracting,
                self.current,
                self.total,
                Some(&self.path),
                total,
                self.bytes_total,
            );
            self.since_report = 0;
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Bridges Archon's cancellation flag to `rars`, which checks its own token in
/// decoder loops. The bridge stops itself when dropped.
struct CancellationBridge {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CancellationBridge {
    fn new(ctx: &OperationContext, token: &ReadCancellation) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let ctx = ctx.clone();
        let token = token.clone();
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if ctx.is_cancelled() {
                    token.cancel();
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for CancellationBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl ArchiveEngine for RarEngine {
    fn id(&self) -> &'static str {
        "rar"
    }

    fn capabilities(&self) -> Vec<FormatCapability> {
        [ArchiveFormat::Rar, ArchiveFormat::Rar5]
            .into_iter()
            .map(|format| FormatCapability {
                format,
                label: format.label().to_string(),
                extensions: format
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
                notes: format!(
                    "{} listing, extraction and integrity testing are implemented, including encrypted archives. Multi-volume sets are recognised but split extraction is not supported yet. Creating RAR archives is never offered.",
                    format.label()
                ),
            })
            .collect()
    }

    fn list(&self, archive: &Path, password: Option<&str>) -> ArchonResult<ArchiveListing> {
        self.inspect(archive, password)
            .map(|(listing, _, _)| listing)
    }

    fn info(&self, archive: &Path, password: Option<&str>) -> ArchonResult<ArchiveInfo> {
        let listing = self.list(archive, password)?;
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
            "Creating RAR archives is not supported. RAR is a proprietary format; Archon only reads it."
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

        let (listing, has_split, _) =
            self.inspect(&options.archive, options.password.as_deref())?;
        if has_split {
            return Err(ArchonError::UnsupportedOperation(
                MULTI_VOLUME_MESSAGE.into(),
            ));
        }

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

        let archive = self.open(&options.archive, options.password.as_deref())?;
        let cancellation = ReadCancellation::new();
        let bridge = CancellationBridge::new(ctx, &cancellation);
        let read_options = ArchiveReadOptions::with_optional_password(
            options
                .password
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(str::as_bytes),
        )
        .with_cancellation(&cancellation)
        .with_max_member_output_bytes(self.limits.max_entry_uncompressed)
        .with_max_total_output_bytes(self.limits.max_total_uncompressed);

        let bytes_written = Arc::new(AtomicU64::new(0));
        let mut staged: Vec<StagedFile> = Vec::new();
        let mut directory_times: Vec<(PathBuf, Option<String>)> = Vec::new();
        let mut failure: Option<ArchonError> = None;
        let mut done: HashSet<usize> = HashSet::new();
        let mut processed = 0_usize;
        let mut extracted_dirs = 0_usize;
        let mut skipped = 0_usize;

        let decoded = archive.extract_to_with_options(read_options, |meta| {
            if failure.is_some() {
                return Err(RarError::Cancelled);
            }
            if let Err(error) = ctx.check_cancelled() {
                failure = Some(error);
                return Err(RarError::Cancelled);
            }
            let key = normalize_archive_path(&meta.name_lossy());
            let Some(&position) = by_path.get(&key) else {
                return Ok(Box::new(io::sink()) as Box<dyn Write>);
            };
            if !done.insert(position) {
                return Ok(Box::new(io::sink()) as Box<dyn Write>);
            }
            let plan_entry = &plan[position];
            let target = match safe_join(&destination, &plan_entry.relative) {
                Ok(value) => value,
                Err(error) => {
                    failure = Some(error);
                    return Err(RarError::Cancelled);
                }
            };

            if plan_entry.is_dir {
                if options.preserve_structure {
                    if let Err(error) = fs::create_dir_all(&target) {
                        failure = Some(ArchonError::Io(format!(
                            "Cannot create {}: {error}",
                            target.display()
                        )));
                        return Err(RarError::Cancelled);
                    }
                    directory_times.push((target, plan_entry.modified.clone()));
                    extracted_dirs += 1;
                }
                return Ok(Box::new(io::sink()) as Box<dyn Write>);
            }

            processed += 1;
            let parent = match target.parent() {
                Some(value) => value,
                None => {
                    failure = Some(ArchonError::Configuration(format!(
                        "Cannot determine the parent folder for {}",
                        target.display()
                    )));
                    return Err(RarError::Cancelled);
                }
            };
            if let Err(error) = fs::create_dir_all(parent) {
                failure = Some(ArchonError::Io(format!(
                    "Cannot create {}: {error}",
                    parent.display()
                )));
                return Err(RarError::Cancelled);
            }
            let final_path = match resolve_overwrite(&target, options.overwrite, &mut skipped) {
                Ok(Some(value)) => value,
                Ok(None) => return Ok(Box::new(io::sink()) as Box<dyn Write>),
                Err(error) => {
                    failure = Some(error);
                    return Err(RarError::Cancelled);
                }
            };
            let (file, temp_path) = match create_temp_file(parent) {
                Ok(value) => value,
                Err(error) => {
                    failure = Some(error);
                    return Err(RarError::Cancelled);
                }
            };
            staged.push(StagedFile {
                temp: temp_path,
                final_path,
                modified: plan_entry.modified.clone(),
            });
            Ok(Box::new(StagingWriter {
                file,
                path: plan_entry.archive_path.clone(),
                ctx: ctx.clone(),
                written: 0,
                bytes_total: total_size,
                bytes_done: Arc::clone(&bytes_written),
                current: processed,
                total: file_total,
                since_report: 0,
            }) as Box<dyn Write>)
        });

        drop(bridge);

        let bytes_done = bytes_written.load(Ordering::Relaxed);
        if let Some(error) = failure {
            cleanup_staged(&staged);
            return Err(error);
        }
        match decoded {
            Ok(()) => {}
            Err(error) => {
                cleanup_staged(&staged);
                if ctx.is_cancelled() {
                    return Err(ArchonError::Cancelled);
                }
                return Err(map_error(error, &options.archive));
            }
        }

        publish_staged(&staged, options.overwrite)?;
        for (path, modified) in directory_times {
            apply_system_time(&path, modified.as_deref());
        }

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

        ctx.report(
            ProgressPhase::Extracting,
            file_total,
            file_total,
            None,
            bytes_done,
            total_size,
        );

        Ok(ExtractReport {
            destination: options.destination.display().to_string(),
            extracted_files: processed.saturating_sub(skipped),
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
        let (listing, has_split, _) = self.inspect(archive, password)?;
        if has_split {
            return Err(ArchonError::UnsupportedOperation(
                MULTI_VOLUME_MESSAGE.into(),
            ));
        }
        let parsed = self.open(archive, password)?;

        let cancellation = ReadCancellation::new();
        let bridge = CancellationBridge::new(ctx, &cancellation);
        let read_options = ArchiveReadOptions::with_optional_password(
            password
                .filter(|value| !value.is_empty())
                .map(str::as_bytes),
        )
        .with_cancellation(&cancellation)
        .with_max_member_output_bytes(self.limits.max_entry_uncompressed)
        .with_max_total_output_bytes(self.limits.max_total_uncompressed);

        let mut failure: Option<ArchonError> = None;
        let mut entries_checked = 0_usize;
        let verified = parsed.test_with_options_and_callback(read_options, |_meta| {
            if let Err(error) = ctx.check_cancelled() {
                failure = Some(error);
                return Err(RarError::Cancelled);
            }
            entries_checked += 1;
            Ok(())
        });
        drop(bridge);

        if let Some(error) = failure {
            return Err(error);
        }

        let mut warnings = listing.warnings.clone();
        let mut issues = Vec::new();
        if let Err(error) = verified {
            if ctx.is_cancelled() {
                return Err(ArchonError::Cancelled);
            }
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
        if issues.len() >= self.limits.max_reported_issues {
            warnings.push("The issue list was truncated at the safety limit".into());
        }

        Ok(TestReport {
            ok: issues.is_empty(),
            entries_checked,
            bytes_checked: listing.total_size,
            encrypted_entries: listing.encrypted_entries,
            issues,
            elapsed_ms: started.elapsed().as_millis(),
            warnings,
        })
    }
}

fn entry_detail(member: &rars::ArchiveMember) -> (u32, String) {
    match &member.detail {
        ArchiveMemberDetail::Rar13 {
            method,
            file_checksum,
            ..
        } => (
            u32::from(*file_checksum),
            legacy_method_label(*method, member.meta.is_stored),
        ),
        ArchiveMemberDetail::Rar15To40 { method, crc32, .. } => {
            (*crc32, legacy_method_label(*method, member.meta.is_stored))
        }
        ArchiveMemberDetail::Rar50Plus {
            compression_info,
            crc32,
            ..
        } => (
            crc32.unwrap_or(0),
            rar5_method_label(*compression_info, member.meta.is_stored),
        ),
        _ => (
            0,
            if member.meta.is_stored {
                "Store".to_string()
            } else {
                "RAR".to_string()
            },
        ),
    }
}

fn legacy_method_label(method: u8, stored: bool) -> String {
    if stored || method == 0x30 {
        return "Store".to_string();
    }
    match method.checked_sub(0x30) {
        Some(level) if (1..=5).contains(&level) => format!("RAR m{level}"),
        _ => "RAR".to_string(),
    }
}

fn rar5_method_label(compression_info: u64, stored: bool) -> String {
    if stored {
        return "Store".to_string();
    }
    // RAR5 packs the compression method in bits 7-9 and a solid flag in bit 6.
    let method = (compression_info >> 7) & 0x07;
    if method == 0 {
        "Store".to_string()
    } else {
        format!("RAR5 m{method}")
    }
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

fn publish_staged(staged: &[StagedFile], overwrite: OverwritePolicy) -> ArchonResult<()> {
    for (position, item) in staged.iter().enumerate() {
        apply_system_time(&item.temp, item.modified.as_deref());
        if overwrite == OverwritePolicy::Overwrite && item.final_path.exists() {
            if let Err(error) = fs::remove_file(&item.final_path) {
                cleanup_staged(&staged[position..]);
                return Err(ArchonError::Io(format!(
                    "Cannot replace {}: {error}",
                    item.final_path.display()
                )));
            }
        }
        if let Err(error) = fs::rename(&item.temp, &item.final_path) {
            cleanup_staged(&staged[position..]);
            return Err(ArchonError::Io(format!(
                "Cannot write {}: {error}",
                item.final_path.display()
            )));
        }
    }
    Ok(())
}

fn cleanup_staged(staged: &[StagedFile]) {
    for item in staged {
        let _ = fs::remove_file(&item.temp);
    }
}

fn map_error(error: RarError, path: &Path) -> ArchonError {
    match error.kind() {
        RarErrorKind::PasswordRequired => ArchonError::PasswordRequired,
        RarErrorKind::BadPassword => ArchonError::InvalidPassword,
        RarErrorKind::ChecksumMismatch => ArchonError::InvalidArchive(format!(
            "Checksum verification failed in {}",
            path.display()
        )),
        RarErrorKind::UnsupportedFormat => ArchonError::InvalidArchive(format!(
            "{} does not contain a valid RAR signature",
            path.display()
        )),
        RarErrorKind::UnsupportedFeature => ArchonError::UnsupportedOperation(format!(
            "This RAR archive uses a feature the engine does not support: {error}"
        )),
        RarErrorKind::UnsafePath => ArchonError::PathTraversal {
            entry: error.to_string(),
        },
        RarErrorKind::ResourceLimit => ArchonError::LimitExceeded(error.to_string()),
        RarErrorKind::Cancelled => ArchonError::Cancelled,
        RarErrorKind::Io => {
            ArchonError::Io(format!("RAR read failed for {}: {error}", path.display()))
        }
        RarErrorKind::SourceChanged => ArchonError::InvalidArchive(format!(
            "{} changed while it was being read",
            path.display()
        )),
        RarErrorKind::InvalidArchive => ArchonError::InvalidArchive(format!(
            "{} could not be read as a RAR archive: {error}",
            path.display()
        )),
        RarErrorKind::EntryNotFound => {
            ArchonError::InvalidArchive("A referenced RAR entry was not found".into())
        }
        RarErrorKind::DuplicateEntry => {
            ArchonError::Conflict("The RAR archive contains duplicate entry names".into())
        }
        RarErrorKind::InvalidArgument => ArchonError::Engine(error.to_string()),
        RarErrorKind::WriterFailure => ArchonError::Engine(error.to_string()),
        _ => ArchonError::Engine(format!("RAR engine error for {}: {error}", path.display())),
    }
}
