use std::env;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use archon_core::split::{join_volumes, split_archive};
use archon_core::{
    ArchiveFormat, ArchiveInfo, ArchiveListing, CompressionPreset, CreateOptions, CreateReport,
    EngineRegistry, ExtractOptions, ExtractReport, OperationContext, OverwritePolicy, Progress,
    ProgressPhase, TestReport, ZipMethod,
};
use clap::{Parser, Subcommand, ValueEnum};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "archon-cli",
    version,
    about = "Archon - universal archive manager",
    long_about = "Archon is an archive manager with a shared engine core for the desktop app and CLI. \
This build browses and extracts ZIP, 7Z, TAR-family archives and GZ/BZ2/XZ/ZST streams. \
ZIP archives can be created, updated, pruned and tested, including AES-256 encrypted archives. \
Passwords are read from ARCHON_PASSWORD or stdin, never from the command line."
)]
struct Cli {
    /// Emit machine-readable JSON for list, info, create, extract and test.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the contents of an archive.
    #[command(alias = "ls")]
    List {
        archive: PathBuf,
        /// Read the password from stdin instead of ARCHON_PASSWORD.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Show archive properties and totals.
    Info {
        archive: PathBuf,
        /// Read the password from stdin instead of ARCHON_PASSWORD.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Extract files from an archive.
    #[command(alias = "x")]
    Extract {
        archive: PathBuf,
        /// Destination folder. Defaults to a folder named after the archive.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Extract only this archive path. May be repeated; folders include their children.
        #[arg(long = "entry")]
        entries: Vec<String>,
        /// What to do when a file already exists.
        #[arg(long, value_enum, default_value_t = OverwriteArg::Rename)]
        overwrite: OverwriteArg,
        /// Flatten folders and extract every selected file into the destination root.
        #[arg(long)]
        flat: bool,
        /// Read the password from stdin instead of ARCHON_PASSWORD.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Create a ZIP archive.
    #[command(alias = "a")]
    Create {
        archive: PathBuf,
        /// Files or folders to compress.
        sources: Vec<PathBuf>,
        #[arg(short, long, value_enum, default_value_t = PresetArg::Normal)]
        preset: PresetArg,
        /// Override the preset with an explicit compression method.
        #[arg(long, value_enum)]
        method: Option<MethodArg>,
        /// Compression level for Deflate (1-9).
        #[arg(long)]
        level: Option<i64>,
        /// Exclude a glob pattern. May be repeated.
        #[arg(short = 'x', long)]
        exclude: Vec<String>,
        /// Replace an existing output archive.
        #[arg(short, long)]
        force: bool,
        /// Encrypt file contents with AES-256.
        #[arg(long)]
        encrypt: bool,
        /// Read the password from stdin instead of ARCHON_PASSWORD.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Verify archive structure and checksums.
    #[command(alias = "t")]
    Test {
        archive: PathBuf,
        /// Read the password from stdin instead of ARCHON_PASSWORD.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Add files or folders to an existing ZIP archive, creating it when absent.
    #[command(alias = "u")]
    Add {
        archive: PathBuf,
        /// Files or folders to add.
        sources: Vec<PathBuf>,
    },
    /// Remove entries from a ZIP archive. Folders include their children.
    #[command(alias = "rm")]
    Remove {
        archive: PathBuf,
        /// Archive paths to remove.
        entries: Vec<String>,
    },
    /// Split a finished archive into numbered volumes.
    Split {
        archive: PathBuf,
        /// Maximum size of each volume in megabytes.
        #[arg(long)]
        size_mb: u64,
    },
    /// Join numbered .001, .002, ... volumes back into one file.
    Join {
        /// The first volume, ending in .001.
        first: PathBuf,
        /// Destination file for the joined archive.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Copy, Clone, ValueEnum)]
enum OverwriteArg {
    Overwrite,
    Skip,
    Rename,
}

impl From<OverwriteArg> for OverwritePolicy {
    fn from(value: OverwriteArg) -> Self {
        match value {
            OverwriteArg::Overwrite => OverwritePolicy::Overwrite,
            OverwriteArg::Skip => OverwritePolicy::Skip,
            OverwriteArg::Rename => OverwritePolicy::Rename,
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum PresetArg {
    Store,
    Fast,
    Normal,
    Maximum,
    Ultra,
}

impl From<PresetArg> for CompressionPreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Store => CompressionPreset::Store,
            PresetArg::Fast => CompressionPreset::Fast,
            PresetArg::Normal => CompressionPreset::Normal,
            PresetArg::Maximum => CompressionPreset::Maximum,
            PresetArg::Ultra => CompressionPreset::Ultra,
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum MethodArg {
    Store,
    Deflate,
}

impl From<MethodArg> for ZipMethod {
    fn from(value: MethodArg) -> Self {
        match value {
            MethodArg::Store => ZipMethod::Store,
            MethodArg::Deflate => ZipMethod::Deflate,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("archon: error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let registry = EngineRegistry::new();
    match cli.command {
        Command::List {
            archive,
            password_stdin,
        } => {
            let mut password = initial_password(password_stdin)?;
            let mut attempts = 0;
            let listing = loop {
                match registry.list(&archive, password.as_ref().map(|value| value.as_str())) {
                    Ok(listing) => break listing,
                    Err(archon_core::ArchonError::PasswordRequired)
                    | Err(archon_core::ArchonError::InvalidPassword)
                        if attempts < 3 && std::io::stdin().is_terminal() && !password_stdin =>
                    {
                        attempts += 1;
                        password = Some(Zeroizing::new(prompt_password("Password: ")?));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            if cli.json {
                print_json(&listing)?;
            } else {
                print_listing(&listing);
            }
        }
        Command::Info {
            archive,
            password_stdin,
        } => {
            let mut password = initial_password(password_stdin)?;
            let mut attempts = 0;
            let info = loop {
                match registry.info(&archive, password.as_ref().map(|value| value.as_str())) {
                    Ok(info) => break info,
                    Err(archon_core::ArchonError::PasswordRequired)
                    | Err(archon_core::ArchonError::InvalidPassword)
                        if attempts < 3 && std::io::stdin().is_terminal() && !password_stdin =>
                    {
                        attempts += 1;
                        password = Some(Zeroizing::new(prompt_password("Password: ")?));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            if cli.json {
                print_json(&info)?;
            } else {
                print_info(&info);
            }
        }
        Command::Extract {
            archive,
            output,
            entries,
            overwrite,
            flat,
            password_stdin,
        } => {
            if archive.extension().is_none() && !archive.exists() {
                return Err(format!("archive not found: {}", archive.display()).into());
            }
            let destination = output.unwrap_or_else(|| default_extract_dir(&archive));
            let mut password = initial_password(password_stdin)?;
            let mut attempts = 0;
            let report = loop {
                let options = ExtractOptions {
                    archive: archive.clone(),
                    destination: destination.clone(),
                    entries: if entries.is_empty() {
                        None
                    } else {
                        Some(entries.clone())
                    },
                    password: password.as_ref().map(|value| value.to_string()),
                    overwrite: overwrite.into(),
                    preserve_structure: !flat,
                };
                let ctx = operation_context();
                match registry.extract(&options, &ctx) {
                    Ok(report) => break report,
                    Err(archon_core::ArchonError::PasswordRequired)
                    | Err(archon_core::ArchonError::InvalidPassword)
                        if attempts < 3 && std::io::stdin().is_terminal() && !password_stdin =>
                    {
                        attempts += 1;
                        password = Some(Zeroizing::new(prompt_password("Password: ")?));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            if cli.json {
                print_json(&report)?;
            } else {
                print_extract(&report);
            }
        }
        Command::Create {
            archive,
            sources,
            preset,
            method,
            level,
            exclude,
            force,
            encrypt,
            password_stdin,
        } => {
            if sources.is_empty() {
                return Err("no input files or folders were provided".into());
            }
            let password = if encrypt {
                let value = acquire_new_password(password_stdin)?;
                Some(value)
            } else {
                None
            };
            let options = CreateOptions {
                sources,
                output: archive.clone(),
                format: ArchiveFormat::Zip,
                preset: preset.into(),
                method: method.map(Into::into),
                level,
                password: password.as_ref().map(|value| value.to_string()),
                exclude,
                overwrite_output: force,
            };
            let report = registry.create(&options, &operation_context())?;
            if cli.json {
                print_json(&report)?;
            } else {
                print_create(&report, "Created");
            }
        }
        Command::Test {
            archive,
            password_stdin,
        } => {
            let mut password = initial_password(password_stdin)?;
            let mut attempts = 0;
            let report: TestReport = loop {
                let ctx = operation_context();
                match registry.test(
                    &archive,
                    password.as_ref().map(|value| value.as_str()),
                    &ctx,
                ) {
                    Ok(report) => break report,
                    Err(archon_core::ArchonError::PasswordRequired)
                    | Err(archon_core::ArchonError::InvalidPassword)
                        if attempts < 3 && std::io::stdin().is_terminal() && !password_stdin =>
                    {
                        attempts += 1;
                        password = Some(Zeroizing::new(prompt_password("Password: ")?));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            if cli.json {
                print_json(&report)?;
            } else {
                print_test(&report);
            }
            if !report.ok {
                return Err(format!(
                    "integrity test failed with {} issue(s)",
                    report.issues.len()
                )
                .into());
            }
        }
        Command::Add { archive, sources } => {
            if sources.is_empty() {
                return Err("no input files or folders were provided".into());
            }
            let existed = archive.exists();
            let report = registry.add_entries(&archive, &sources, &operation_context())?;
            if cli.json {
                print_json(&report)?;
            } else {
                print_create(&report, if existed { "Updated" } else { "Created" });
            }
        }
        Command::Remove { archive, entries } => {
            if entries.is_empty() {
                return Err("no archive entries were provided".into());
            }
            registry.remove_entries(&archive, &entries, &operation_context())?;
            if cli.json {
                print_json(&serde_json::json!({
                    "archive": archive.display().to_string(),
                    "removed": entries,
                }))?;
            } else {
                println!(
                    "Removed {} entr{} from {}",
                    entries.len(),
                    if entries.len() == 1 { "y" } else { "ies" },
                    archive.display()
                );
            }
        }
        Command::Split { archive, size_mb } => {
            if size_mb == 0 {
                return Err("--size-mb must be greater than zero".into());
            }
            let report = split_archive(&archive, size_mb.saturating_mul(1024 * 1024))?;
            if cli.json {
                print_json(&report)?;
            } else {
                println!(
                    "Split {} into {} volume(s) of {} MiB",
                    report.source, report.part_count, size_mb
                );
                for part in &report.parts {
                    println!("  {part}");
                }
            }
        }
        Command::Join { first, output } => {
            let report = join_volumes(&first, &output)?;
            if cli.json {
                print_json(&report)?;
            } else {
                println!(
                    "Joined {} volume(s) into {} ({} bytes)",
                    report.part_count, report.output, report.total_bytes
                );
            }
        }
    }
    Ok(())
}

fn operation_context() -> OperationContext {
    let interactive = std::io::stderr().is_terminal();
    OperationContext::new("cli").with_reporter(move |progress| {
        if interactive {
            render_progress(&progress);
        }
    })
}

fn render_progress(progress: &Progress) {
    let phase = match progress.phase {
        ProgressPhase::Scanning => "Scanning",
        ProgressPhase::Compressing => "Compressing",
        ProgressPhase::Extracting => "Extracting",
        ProgressPhase::Testing => "Testing",
        ProgressPhase::Finalizing => "Finalising",
    };
    let percent = if progress.bytes_total > 0 {
        format!(
            "{:>3}%",
            (progress.bytes_done as f64 / progress.bytes_total as f64 * 100.0).min(100.0) as u32
        )
    } else {
        format!("{}/{}", progress.current, progress.total)
    };
    let name = progress.path.as_deref().unwrap_or("");
    eprint!(
        "\r{phase:<12} {percent:>4}  {}/{}  {:<50}",
        progress.current,
        progress.total,
        truncate(name, 50)
    );
    let _ = std::io::stderr().flush();
    if progress.current == progress.total {
        eprintln!();
    }
}

fn initial_password(
    from_stdin: bool,
) -> Result<Option<Zeroizing<String>>, Box<dyn std::error::Error>> {
    if from_stdin {
        let mut value = String::new();
        std::io::stdin().read_line(&mut value)?;
        let value = value.trim_end_matches(['\r', '\n']).to_string();
        return Ok(if value.is_empty() {
            None
        } else {
            Some(Zeroizing::new(value))
        });
    }
    match env::var("ARCHON_PASSWORD") {
        Ok(value) if !value.is_empty() => Ok(Some(Zeroizing::new(value))),
        _ => Ok(None),
    }
}

fn acquire_new_password(from_stdin: bool) -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
    if from_stdin {
        let mut value = String::new();
        std::io::stdin().read_line(&mut value)?;
        let value = value.trim_end_matches(['\r', '\n']).to_string();
        if value.is_empty() {
            return Err("the password read from stdin is empty".into());
        }
        return Ok(Zeroizing::new(value));
    }
    if let Ok(value) = env::var("ARCHON_PASSWORD") {
        if !value.is_empty() {
            return Ok(Zeroizing::new(value));
        }
    }
    if !std::io::stdin().is_terminal() {
        return Err("encryption requested but no password source is available; set ARCHON_PASSWORD or use --password-stdin".into());
    }
    let first = prompt_password("New password: ")?;
    if first.is_empty() {
        return Err("password cannot be empty".into());
    }
    let second = prompt_password("Confirm password: ")?;
    if first != second {
        return Err("passwords do not match".into());
    }
    Ok(Zeroizing::new(first))
}

fn prompt_password(label: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(rpassword::prompt_password(label)?)
}

fn default_extract_dir(archive: &Path) -> PathBuf {
    let parent = archive.parent().unwrap_or_else(|| Path::new("."));
    let stem = archive
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".into());
    parent.join(stem)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_listing(listing: &ArchiveListing) {
    println!(
        "{}  [{}]  {} entries, {} -> {}",
        listing.path,
        listing.format_label,
        listing.entries.len(),
        human_bytes(listing.total_size),
        human_bytes(listing.total_compressed)
    );
    if let Some(comment) = &listing.comment {
        println!("Comment: {comment}");
    }
    for warning in &listing.warnings {
        println!("warning: {warning}");
    }
    println!(
        "{:>12}  {:>12}  {:>7}  {:<10}  {:<3}  {}",
        "Size", "Compressed", "Saved", "Modified", "Enc", "Name"
    );
    println!("{}", "-".repeat(96));
    for entry in &listing.entries {
        let modified = entry.modified.as_deref().unwrap_or("-");
        let name = if entry.is_dir {
            format!("{}/", entry.path)
        } else {
            entry.path.clone()
        };
        println!(
            "{:>12}  {:>12}  {:>6.0}%  {:<10}  {:<3}  {}",
            human_bytes(entry.size),
            human_bytes(entry.compressed_size),
            entry.ratio,
            truncate(modified, 10),
            if entry.encrypted { "yes" } else { "" },
            name
        );
    }
}

fn print_info(info: &ArchiveInfo) {
    println!("Archive:       {}", info.path);
    println!("Format:        {}", info.format_label);
    println!("Archive size:  {}", human_bytes(info.archive_size));
    println!(
        "Entries:       {} ({} files, {} folders)",
        info.entry_count, info.file_count, info.dir_count
    );
    println!("Original size: {}", human_bytes(info.total_size));
    println!("Compressed:    {}", human_bytes(info.total_compressed));
    println!("Saved:         {:.1}%", info.ratio);
    println!(
        "Encrypted:     {} entr{}",
        info.encrypted_entries,
        if info.encrypted_entries == 1 {
            "y"
        } else {
            "ies"
        }
    );
    if let Some(comment) = &info.comment {
        println!("Comment:       {comment}");
    }
    for warning in &info.warnings {
        println!("warning: {warning}");
    }
}

fn print_create(report: &CreateReport, verb: &str) {
    println!(
        "{verb} {} ({} entries, {} -> {}, {:.1}% saved) in {:.2}s",
        report.output,
        report.entry_count,
        human_bytes(report.total_size),
        human_bytes(report.archive_size),
        report.ratio,
        report.elapsed_ms as f64 / 1000.0
    );
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
}

fn print_extract(report: &ExtractReport) {
    println!(
        "Extracted {} file(s) and {} folder(s) to {} ({}), {:.2}s",
        report.extracted_files,
        report.extracted_dirs,
        report.destination,
        human_bytes(report.total_bytes),
        report.elapsed_ms as f64 / 1000.0
    );
    if report.skipped > 0 {
        println!("Skipped {} existing file(s)", report.skipped);
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
}

fn print_test(report: &TestReport) {
    if report.ok {
        println!(
            "OK: {} file{} checked ({}), {:.2}s",
            report.entries_checked,
            if report.entries_checked == 1 { "" } else { "s" },
            human_bytes(report.bytes_checked),
            report.elapsed_ms as f64 / 1000.0
        );
    } else {
        println!("FAILED: {} issue(s)", report.issues.len());
        for issue in &report.issues {
            println!("  {}: {} ({})", issue.entry, issue.message, issue.kind);
        }
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let collected: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!(
            "{}...",
            collected
                .chars()
                .take(max.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        collected
    }
}
