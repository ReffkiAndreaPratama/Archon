use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use archon_core::{
    ArchiveFormat, CompressionPreset, CreateOptions, EngineRegistry, ExtractOptions,
    OperationContext, OverwritePolicy,
};
use sevenz_rust2::encoder_options::AesEncoderOptions;
use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, Password};
use tempfile::TempDir;

fn registry() -> EngineRegistry {
    EngineRegistry::new()
}

fn extract_options(archive: &Path, destination: &Path) -> ExtractOptions {
    ExtractOptions {
        archive: archive.to_path_buf(),
        destination: destination.to_path_buf(),
        entries: None,
        password: None,
        overwrite: OverwritePolicy::Rename,
        preserve_structure: true,
    }
}

fn create_options(sources: Vec<PathBuf>, output: &Path) -> CreateOptions {
    CreateOptions {
        sources,
        output: output.to_path_buf(),
        format: ArchiveFormat::SevenZ,
        preset: CompressionPreset::Normal,
        method: None,
        level: None,
        password: None,
        exclude: Vec::new(),
        overwrite_output: true,
    }
}

fn write_archive(path: &Path, password: Option<&str>) {
    let mut writer = ArchiveWriter::create(path).unwrap();
    if let Some(password) = password {
        writer.set_content_methods(vec![
            AesEncoderOptions::new(Password::new(password)).into(),
            EncoderConfiguration::new(EncoderMethod::LZMA2),
        ]);
    }

    writer
        .push_archive_entry::<Cursor<Vec<u8>>>(ArchiveEntry::new_directory("docs/"), None)
        .unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("docs/readme.txt"),
            Some(Cursor::new(b"hello sevenz\n".to_vec())),
        )
        .unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("docs/nested/data.bin"),
            Some(Cursor::new(vec![0x5a; 70_000])),
        )
        .unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("empty.txt"),
            Some(Cursor::new(Vec::new())),
        )
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn sevenz_list_extract_and_test_roundtrip() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("sample.7z");
    write_archive(&archive, None);

    let engine = registry();
    let ctx = OperationContext::new("sevenz-roundtrip");

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::SevenZ);
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "docs/readme.txt")
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "docs" && entry.is_dir)
    );
    assert_eq!(listing.encrypted_entries, 0);

    let destination = temp.path().join("out");
    let report = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap();
    assert_eq!(report.extracted_files, 3);
    assert_eq!(
        fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
        "hello sevenz\n"
    );
    assert_eq!(
        fs::read(destination.join("docs/nested/data.bin"))
            .unwrap()
            .len(),
        70_000
    );
    assert!(destination.join("empty.txt").exists());

    let test = engine.test(&archive, None, &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.entries_checked, 4);

    let info = engine.info(&archive, None).unwrap();
    assert_eq!(info.file_count, 3);
    assert_eq!(info.dir_count, 1);
    assert_eq!(info.format, ArchiveFormat::SevenZ);
}

#[test]
fn encrypted_sevenz_requires_the_correct_password() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("secret.7z");
    write_archive(&archive, Some("correct horse"));

    let engine = registry();
    let ctx = OperationContext::new("sevenz-encrypted");

    let error = engine.list(&archive, None).unwrap_err();
    assert_eq!(error.code(), "password_required");

    let error = engine.list(&archive, Some("wrong")).unwrap_err();
    assert_eq!(error.code(), "invalid_password");

    let listing = engine.list(&archive, Some("correct horse")).unwrap();
    assert!(listing.encrypted_entries > 0);

    let destination = temp.path().join("out");
    let mut options = extract_options(&archive, &destination);
    options.password = Some("correct horse".into());
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 3);
    assert_eq!(
        fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
        "hello sevenz\n"
    );

    let test = engine.test(&archive, Some("correct horse"), &ctx).unwrap();
    assert!(test.ok);
}

#[test]
fn sevenz_rejects_path_traversal_before_writing() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("hostile.7z");
    let mut writer = ArchiveWriter::create(&archive).unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("../escape.txt"),
            Some(Cursor::new(b"owned".to_vec())),
        )
        .unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("safe.txt"),
            Some(Cursor::new(b"safe".to_vec())),
        )
        .unwrap();
    writer.finish().unwrap();

    let engine = registry();
    let listing = engine.list(&archive, None).unwrap();
    assert!(
        listing
            .warnings
            .iter()
            .any(|value| value.contains("unsafe paths"))
    );

    let destination = temp.path().join("out");
    let error = engine
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("traversal"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "path_traversal");
    assert!(!temp.path().join("escape.txt").exists());
    assert!(!destination.join("safe.txt").exists());
}

#[test]
fn sevenz_is_read_only_for_creation() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), b"a").unwrap();
    let output = temp.path().join("new.7z");

    let error = registry()
        .create(
            &create_options(vec![source], &output),
            &OperationContext::new("sevenz-create"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
    assert!(!output.exists());
}

#[test]
fn sevenz_capability_is_reported_as_read_only() {
    let capability = registry()
        .capabilities()
        .into_iter()
        .find(|capability| capability.format == ArchiveFormat::SevenZ)
        .expect("7z capability present");
    assert!(capability.can_read);
    assert!(capability.can_test);
    assert!(!capability.can_create);
    assert!(!capability.can_modify);
}
