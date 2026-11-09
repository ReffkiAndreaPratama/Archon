use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use archon_core::{
    ArchiveFormat, CompressionPreset, CreateOptions, EngineRegistry, ExtractOptions,
    OperationContext, OverwritePolicy, ZipMethod,
};
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn registry() -> EngineRegistry {
    EngineRegistry::new()
}

fn write_file(path: &Path, contents: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn sample_tree(root: &Path) {
    write_file(&root.join("hello.txt"), b"Hello, Archon!\n");
    write_file(&root.join("empty.bin"), b"");
    write_file(
        &root.join("nested/deep/unicode-\u{65e5}\u{672c}\u{8a9e}.txt"),
        "isi unicode \u{1f680}".as_bytes(),
    );
    write_file(&root.join("nested/data.bin"), vec![0x5a; 300_000]);
    fs::create_dir_all(root.join("empty-folder")).unwrap();
    let long_name = format!("{}.txt", "a".repeat(180));
    write_file(&root.join(&long_name), b"long name");
}

fn create_options(sources: Vec<PathBuf>, output: &Path, password: Option<&str>) -> CreateOptions {
    CreateOptions {
        sources,
        output: output.to_path_buf(),
        format: ArchiveFormat::Zip,
        preset: CompressionPreset::Normal,
        method: None,
        level: None,
        password: password.map(str::to_string),
        exclude: Vec::new(),
        overwrite_output: true,
    }
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

#[test]
fn create_list_extract_and_test_roundtrip() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    sample_tree(&source);
    let archive = temp.path().join("sample.zip");
    let engine = registry();
    let ctx = OperationContext::new("roundtrip");

    let report = engine
        .create(&create_options(vec![source.clone()], &archive, None), &ctx)
        .unwrap();
    assert!(archive.exists());
    assert!(report.entry_count >= 7);
    assert!(report.file_count >= 5);
    assert_eq!(report.format, ArchiveFormat::Zip);

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Zip);
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "input/hello.txt")
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "input/empty-folder" && entry.is_dir)
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path.contains("unicode-\u{65e5}\u{672c}\u{8a9e}.txt"))
    );
    assert_eq!(listing.encrypted_entries, 0);

    let destination = temp.path().join("output");
    let extracted = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap();
    assert_eq!(extracted.extracted_files, report.file_count);
    assert_eq!(
        fs::read_to_string(destination.join("input/hello.txt")).unwrap(),
        "Hello, Archon!\n"
    );
    assert!(
        destination
            .join("input/nested/deep/unicode-\u{65e5}\u{672c}\u{8a9e}.txt")
            .exists()
    );
    assert_eq!(
        fs::read(destination.join("input/empty.bin")).unwrap().len(),
        0
    );
    assert!(destination.join("input/empty-folder").is_dir());

    let test = engine.test(&archive, None, &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.issues.len(), 0);
    assert_eq!(test.entries_checked, report.file_count);

    let info = engine.info(&archive, None).unwrap();
    assert_eq!(info.entry_count, listing.entries.len());
    assert_eq!(info.file_count, report.file_count);
    assert_eq!(info.dir_count, report.dir_count);
    assert!(info.archive_size > 0);
}

#[test]
fn extraction_rejects_path_traversal_before_writing() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("evil.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for name in [
            "safe.txt",
            "../escape.txt",
            "..\\escape-backslash.txt",
            "/absolute.txt",
            "C:/windows.txt",
            "nested/../../escape.txt",
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(b"payload").unwrap();
        }
        writer.finish().unwrap();
    }

    let destination = temp.path().join("dest");
    let error = registry()
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("slip"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "path_traversal", "{error}");
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    assert!(!temp.path().join("escape.txt").exists());
}

#[test]
fn overwrite_policies_skip_rename_and_overwrite() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("a.txt"), b"new content");
    let archive = temp.path().join("a.zip");
    let engine = registry();
    let ctx = OperationContext::new("overwrite");
    engine
        .create(&create_options(vec![source], &archive, None), &ctx)
        .unwrap();

    for policy in [
        OverwritePolicy::Skip,
        OverwritePolicy::Rename,
        OverwritePolicy::Overwrite,
    ] {
        let destination = temp.path().join(format!("dest-{policy:?}"));
        fs::create_dir_all(destination.join("input")).unwrap();
        write_file(&destination.join("input/a.txt"), b"old content");
        let mut options = extract_options(&archive, &destination);
        options.overwrite = policy;
        let report = engine.extract(&options, &ctx).unwrap();

        match policy {
            OverwritePolicy::Skip => {
                assert_eq!(report.skipped, 1);
                assert_eq!(
                    fs::read_to_string(destination.join("input/a.txt")).unwrap(),
                    "old content"
                );
            }
            OverwritePolicy::Rename => {
                assert!(destination.join("input/a (1).txt").exists());
                assert_eq!(
                    fs::read_to_string(destination.join("input/a.txt")).unwrap(),
                    "old content"
                );
            }
            OverwritePolicy::Overwrite => {
                assert_eq!(
                    fs::read_to_string(destination.join("input/a.txt")).unwrap(),
                    "new content"
                );
            }
        }
    }
}

#[test]
fn aes_password_required_wrong_and_correct() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("secret");
    write_file(&source.join("secret.txt"), b"classified");
    let archive = temp.path().join("secret.zip");
    let engine = registry();
    let ctx = OperationContext::new("password");
    let password = "correct horse battery staple";

    engine
        .create(
            &create_options(vec![source], &archive, Some(password)),
            &ctx,
        )
        .unwrap();

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.encrypted_entries, 1);

    let destination = temp.path().join("no-password");
    let error = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "password_required");

    let mut wrong = extract_options(&archive, &temp.path().join("wrong"));
    wrong.password = Some("incorrect".into());
    let error = engine.extract(&wrong, &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password", "{error}");

    let mut correct = extract_options(&archive, &temp.path().join("correct"));
    correct.password = Some(password.into());
    let report = engine.extract(&correct, &ctx).unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read_to_string(temp.path().join("correct/secret/secret.txt")).unwrap(),
        "classified"
    );

    let error = engine.test(&archive, None, &ctx).unwrap_err();
    assert_eq!(error.code(), "password_required");
    let error = engine.test(&archive, Some("wrong"), &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password");
    assert!(engine.test(&archive, Some(password), &ctx).unwrap().ok);
}

#[test]
fn zipcrypto_archives_are_read_with_the_correct_password() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("legacy.zip");
    let engine = registry();
    let ctx = OperationContext::new("zipcrypto");
    let password = "legacy passphrase";

    // Write the fixture with the zip crate's ZipCrypto writer so the test proves
    // Archon can read archives produced by older tools, not just its own AES output.
    {
        use zip::unstable::write::FileOptionsExt;

        let writer = fs::File::create(&archive).unwrap();
        let mut zip = ZipWriter::new(writer);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .with_deprecated_encryption(password.as_bytes())
            .unwrap();
        zip.start_file("legacy/secret.txt", options).unwrap();
        zip.write_all(b"legacy zipcrypto payload").unwrap();
        zip.finish().unwrap();
    }

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.encrypted_entries, 1);
    assert!(listing.entries[0].encrypted);

    let error = engine
        .extract(
            &extract_options(&archive, &temp.path().join("missing")),
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.code(), "password_required");

    let mut wrong = extract_options(&archive, &temp.path().join("wrong"));
    wrong.password = Some("incorrect".into());
    let error = engine.extract(&wrong, &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password", "{error}");

    let mut correct = extract_options(&archive, &temp.path().join("correct"));
    correct.password = Some(password.into());
    let report = engine.extract(&correct, &ctx).unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read_to_string(temp.path().join("correct/legacy/secret.txt")).unwrap(),
        "legacy zipcrypto payload"
    );

    let error = engine.test(&archive, None, &ctx).unwrap_err();
    assert_eq!(error.code(), "password_required");
    let error = engine.test(&archive, Some("incorrect"), &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password");
    assert!(engine.test(&archive, Some(password), &ctx).unwrap().ok);
}

#[test]
fn exclude_patterns_and_store_preset() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("keep.txt"), b"keep");
    write_file(&source.join("drop.log"), b"drop");
    write_file(&source.join("nested/deep.log"), b"drop");
    write_file(&source.join("nested/keep.log.txt"), b"keep");
    let archive = temp.path().join("filtered.zip");
    let engine = registry();
    let ctx = OperationContext::new("exclude");

    let mut options = create_options(vec![source], &archive, None);
    options.preset = CompressionPreset::Store;
    options.exclude = vec!["*.log".into()];
    engine.create(&options, &ctx).unwrap();

    let listing = engine.list(&archive, None).unwrap();
    let paths: Vec<&str> = listing
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert!(paths.contains(&"input/keep.txt"));
    assert!(paths.contains(&"input/nested/keep.log.txt"));
    assert!(!paths.contains(&"input/drop.log"));
    assert!(!paths.contains(&"input/nested/deep.log"));
    let file = listing
        .entries
        .iter()
        .find(|entry| entry.path == "input/keep.txt")
        .unwrap();
    assert_eq!(file.method, "Store");
}

#[test]
fn corrupted_entry_is_detected_and_not_left_on_disk() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("corrupt.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("data.txt", options).unwrap();
        writer
            .write_all(b"hello world, this is the payload")
            .unwrap();
        writer.finish().unwrap();
    }

    let mut bytes = fs::read(&archive).unwrap();
    let position = bytes
        .windows(b"hello world".len())
        .position(|window| window == b"hello world")
        .unwrap();
    bytes[position] ^= 0xff;
    fs::write(&archive, bytes).unwrap();

    let engine = registry();
    let ctx = OperationContext::new("corrupt");
    let report = engine.test(&archive, None, &ctx).unwrap();
    assert!(!report.ok);
    assert!(!report.issues.is_empty());

    let destination = temp.path().join("dest");
    let error = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap_err();
    assert!(error.code() == "io" || error.code() == "invalid_archive");
    assert!(!destination.join("data.txt").exists());
}

#[test]
fn cancellation_stops_create_extract_and_test() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("file.txt"), b"content");
    let archive = temp.path().join("cancelled.zip");
    let engine = registry();

    let ctx = OperationContext::new("cancel-create");
    ctx.cancel();
    let error = engine
        .create(&create_options(vec![source], &archive, None), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "cancelled");
    assert!(!archive.exists());

    let ctx = OperationContext::new("make");
    engine
        .create(
            &create_options(vec![temp.path().join("input")], &archive, None),
            &ctx,
        )
        .unwrap();

    let cancel = OperationContext::new("cancel-extract");
    cancel.cancel();
    let destination = temp.path().join("dest");
    let error = engine
        .extract(&extract_options(&archive, &destination), &cancel)
        .unwrap_err();
    assert_eq!(error.code(), "cancelled");

    let cancel = OperationContext::new("cancel-test");
    cancel.cancel();
    let error = engine.test(&archive, None, &cancel).unwrap_err();
    assert_eq!(error.code(), "cancelled");
}

#[test]
fn unsupported_formats_are_reported_honestly() {
    let temp = TempDir::new().unwrap();
    let wim = temp.path().join("sample.wim");
    fs::write(&wim, b"MSWIM\0\0\0synthetic").unwrap();

    let error = registry().list(&wim, None).unwrap_err();
    assert_eq!(error.code(), "unsupported_format");
    assert!(error.to_string().contains("not available in this build"));

    let options = CreateOptions {
        sources: vec![wim],
        output: temp.path().join("out.wim"),
        format: ArchiveFormat::Wim,
        preset: CompressionPreset::Normal,
        method: None,
        level: None,
        password: None,
        exclude: Vec::new(),
        overwrite_output: false,
    };
    let error = registry()
        .create(&options, &OperationContext::new("wim"))
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
    assert!(!temp.path().join("out.wim").exists());
}

#[test]
fn existing_archives_can_be_updated_and_pruned() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("keep.txt"), b"keep");
    write_file(&source.join("nested/drop.txt"), b"drop");
    write_file(&source.join("nested/stay.txt"), b"stay");
    let archive = temp.path().join("modify.zip");
    let engine = registry();
    let ctx = OperationContext::new("modify");

    engine
        .create(&create_options(vec![source], &archive, None), &ctx)
        .unwrap();

    let extra = temp.path().join("added");
    write_file(&extra.join("new.txt"), b"new contents");
    write_file(&extra.join("keep.txt"), b"replacement contents");
    let report = engine
        .add_entries(&archive, std::slice::from_ref(&extra), &ctx)
        .unwrap();
    assert!(report.entry_count > 0);
    assert!(report.file_count >= 2);

    let listing = engine.list(&archive, None).unwrap();
    let files = listing.entries.iter().filter(|entry| !entry.is_dir).count();
    let dirs = listing.entries.iter().filter(|entry| entry.is_dir).count();
    assert_eq!(report.total_size, listing.total_size);
    assert_eq!(report.file_count, files);
    assert_eq!(report.dir_count, dirs);
    assert_eq!(report.entry_count, files + dirs);
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "added/new.txt")
    );
    assert_eq!(
        listing
            .entries
            .iter()
            .filter(|entry| entry.path == "added/keep.txt")
            .count(),
        1
    );

    let destination = temp.path().join("out");
    engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("added/keep.txt")).unwrap(),
        "replacement contents"
    );
    assert_eq!(
        fs::read_to_string(destination.join("input/keep.txt")).unwrap(),
        "keep"
    );

    engine
        .remove_entries(&archive, &["input/nested".into()], &ctx)
        .unwrap();
    let listing = engine.list(&archive, None).unwrap();
    assert!(
        !listing
            .entries
            .iter()
            .any(|entry| entry.path.starts_with("input/nested"))
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "input/keep.txt")
    );

    let error = engine
        .remove_entries(&archive, &["missing.txt".into()], &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "conflict");

    let test = engine.test(&archive, None, &ctx).unwrap();
    assert!(test.ok);
}

#[test]
fn adding_to_a_missing_archive_creates_it() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("a.txt"), b"a");
    let archive = temp.path().join("fresh.zip");
    let engine = registry();

    let report = engine
        .add_entries(
            &archive,
            std::slice::from_ref(&source),
            &OperationContext::new("fresh"),
        )
        .unwrap();
    assert!(archive.exists());
    assert_eq!(report.file_count, 1);
    assert!(
        engine
            .list(&archive, None)
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.path == "input/a.txt")
    );
}

#[test]
fn encrypted_archives_are_not_modified_in_place() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("a.txt"), b"a");
    let archive = temp.path().join("secret.zip");
    let engine = registry();
    let ctx = OperationContext::new("encrypted-modify");
    engine
        .create(
            &create_options(vec![source.clone()], &archive, Some("secret")),
            &ctx,
        )
        .unwrap();

    let error = engine
        .add_entries(&archive, std::slice::from_ref(&source), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");

    let error = engine
        .remove_entries(&archive, &["input/a.txt".into()], &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
}

#[test]
fn create_creates_missing_output_folders() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("a.txt"), b"a");
    let archive = temp.path().join("nested/new/folder/archive.zip");
    let engine = registry();

    engine
        .create(
            &create_options(vec![source], &archive, None),
            &OperationContext::new("auto-folder"),
        )
        .unwrap();

    assert!(archive.exists());
    assert!(temp.path().join("nested/new/folder").is_dir());
}

#[test]
fn selected_entries_and_missing_selection() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("a.txt"), b"a");
    write_file(&source.join("b.txt"), b"b");
    write_file(&source.join("folder/c.txt"), b"c");
    let archive = temp.path().join("selected.zip");
    let engine = registry();
    let ctx = OperationContext::new("selected");
    engine
        .create(&create_options(vec![source], &archive, None), &ctx)
        .unwrap();

    let destination = temp.path().join("only-folder");
    let mut options = extract_options(&archive, &destination);
    options.entries = Some(vec!["input/folder".into()]);
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 1);
    assert!(destination.join("input/folder/c.txt").exists());
    assert!(!destination.join("input/a.txt").exists());

    let mut missing = extract_options(&archive, &temp.path().join("missing"));
    missing.entries = Some(vec!["input/does-not-exist".into()]);
    let error = engine.extract(&missing, &ctx).unwrap_err();
    assert_eq!(error.code(), "conflict");
}

#[test]
fn symlink_entries_are_never_created() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("links.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .add_symlink("link", "../../outside.txt", SimpleFileOptions::default())
            .unwrap();
        writer
            .start_file("normal.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"normal").unwrap();
        writer.finish().unwrap();
    }

    let destination = temp.path().join("dest");
    let report = registry()
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("links"),
        )
        .unwrap();
    assert_eq!(report.extracted_files, 1);
    assert!(!destination.join("link").exists());
    assert!(!temp.path().join("outside.txt").exists());
    assert!(!report.warnings.is_empty());
}

#[test]
fn level_overrides_and_invalid_levels() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    write_file(&source.join("file.txt"), b"content");
    let archive = temp.path().join("level.zip");
    let engine = registry();
    let ctx = OperationContext::new("level");

    let mut options = create_options(vec![source.clone()], &archive, None);
    options.method = Some(ZipMethod::Deflate);
    options.level = Some(1);
    engine.create(&options, &ctx).unwrap();

    let mut invalid = create_options(vec![source], &temp.path().join("invalid.zip"), None);
    invalid.method = Some(ZipMethod::Deflate);
    invalid.level = Some(99);
    let error = engine.create(&invalid, &ctx).unwrap_err();
    assert_eq!(error.code(), "configuration");
}

#[test]
fn duplicate_and_case_colliding_entries_are_rejected() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("collision.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut writer = ZipWriter::new(file);
        for name in ["folder/File.txt", "folder/file.txt"] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"data").unwrap();
        }
        writer.finish().unwrap();
    }

    let error = registry()
        .extract(
            &extract_options(&archive, &temp.path().join("dest")),
            &OperationContext::new("collision"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "conflict");
}

#[test]
fn empty_archive_and_empty_folder_roundtrip() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("empty-folder");
    fs::create_dir_all(&source).unwrap();
    let archive = temp.path().join("empty.zip");
    let engine = registry();
    let ctx = OperationContext::new("empty");

    let report = engine
        .create(&create_options(vec![source], &archive, None), &ctx)
        .unwrap();
    assert_eq!(report.file_count, 0);
    assert_eq!(report.dir_count, 1);

    let destination = temp.path().join("dest");
    engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap();
    assert!(destination.join("empty-folder").is_dir());
}
