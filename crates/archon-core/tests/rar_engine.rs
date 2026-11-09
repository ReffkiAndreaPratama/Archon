use std::fs;
use std::path::{Path, PathBuf};

use archon_core::{
    ArchiveFormat, CapabilityStatus, CompressionPreset, CreateOptions, EngineRegistry,
    ExtractOptions, OperationContext, OverwritePolicy,
};
use tempfile::TempDir;

const RAR5_PASSWORD: &str = "correct horse";
const LEGACY_PASSWORD: &str = "password";

fn registry() -> EngineRegistry {
    EngineRegistry::new()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("rar")
        .join(name)
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

fn create_options(output: &Path, format: ArchiveFormat) -> CreateOptions {
    CreateOptions {
        sources: Vec::new(),
        output: output.to_path_buf(),
        format,
        preset: CompressionPreset::Normal,
        method: None,
        level: None,
        password: None,
        exclude: Vec::new(),
        overwrite_output: false,
    }
}

fn sample_data_bin() -> Vec<u8> {
    (0..70_000_usize)
        .map(|index| ((index * 31 + 7) % 251) as u8)
        .collect()
}

#[test]
fn rar5_list_extract_and_test_roundtrip() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("sample.rar");
    let engine = registry();
    let ctx = OperationContext::new("rar5-roundtrip");

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Rar5);
    assert_eq!(listing.entries.len(), 5);
    assert_eq!(listing.total_size, 70_010);

    let readme = listing
        .entries
        .iter()
        .find(|entry| entry.path == "docs/readme.txt")
        .expect("readme entry");
    assert_eq!(readme.size, 10);
    assert_eq!(readme.compressed_size, 10);
    assert_eq!(readme.method, "Store");
    assert_eq!(readme.crc32, 0xb180_d29d);
    assert!(!readme.encrypted);
    assert!(readme.modified.is_some());

    let data = listing
        .entries
        .iter()
        .find(|entry| entry.path == "docs/nested/data.bin")
        .expect("data entry");
    assert_eq!(data.size, 70_000);
    assert_eq!(data.method, "RAR5 m3");

    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "docs" && entry.is_dir)
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "docs/nested" && entry.is_dir)
    );
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "empty.txt" && entry.size == 0)
    );

    let destination = temp.path().join("out");
    let report = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap();
    assert_eq!(report.extracted_files, 3);
    assert_eq!(report.total_bytes, 70_010);
    assert_eq!(
        fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
        "hello rar\n"
    );
    assert_eq!(
        fs::read(destination.join("docs/nested/data.bin")).unwrap(),
        sample_data_bin()
    );
    assert_eq!(
        fs::metadata(destination.join("empty.txt")).unwrap().len(),
        0
    );

    let test = engine.test(&archive, None, &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.entries_checked, 5);
    assert_eq!(test.bytes_checked, 70_010);
    assert!(test.issues.is_empty());

    let info = engine.info(&archive, None).unwrap();
    assert_eq!(info.format, ArchiveFormat::Rar5);
    assert_eq!(info.file_count, 3);
    assert_eq!(info.dir_count, 2);
    assert_eq!(info.entry_count, 5);
}

#[test]
fn rar4_legacy_archives_list_extract_and_test() {
    let temp = TempDir::new().unwrap();
    let engine = registry();
    let ctx = OperationContext::new("rar4-legacy");

    let stored = fixture("legacy-stored.rar");
    let listing = engine.list(&stored, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Rar);
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "hello.txt");
    assert_eq!(listing.entries[0].size, 30);
    assert_eq!(listing.entries[0].method, "Store");
    assert_eq!(listing.entries[0].crc32, 0xa538_535e);

    let destination = temp.path().join("stored");
    let report = engine
        .extract(&extract_options(&stored, &destination), &ctx)
        .unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read(destination.join("hello.txt")).unwrap(),
        b"Hello, RAR 3.x fixture world.\n"
    );
    assert!(engine.test(&stored, None, &ctx).unwrap().ok);

    let compressed = fixture("legacy-compressed.rar");
    let listing = engine.list(&compressed, None).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "text.txt");
    assert_eq!(listing.entries[0].size, 2_400);
    assert_eq!(listing.entries[0].method, "RAR m3");
    assert_eq!(listing.entries[0].crc32, 0x6a0d_746d);

    let destination = temp.path().join("compressed");
    let report = engine
        .extract(&extract_options(&compressed, &destination), &ctx)
        .unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read(destination.join("text.txt")).unwrap(),
        "Hello, RAR 3.x fixture world.\n".repeat(80).into_bytes()
    );
    assert!(engine.test(&compressed, None, &ctx).unwrap().ok);

    let solid = fixture("legacy-solid.rar");
    let listing = engine.list(&solid, None).unwrap();
    assert_eq!(listing.entries.len(), 2);
    assert!(
        listing
            .warnings
            .iter()
            .any(|warning| warning.contains("solid RAR archive"))
    );

    let destination = temp.path().join("solid");
    let report = engine
        .extract(&extract_options(&solid, &destination), &ctx)
        .unwrap();
    assert_eq!(report.extracted_files, 2);
    assert_eq!(
        fs::read(destination.join("one.txt")).unwrap(),
        b"shared prefix shared prefix shared prefix alpha\n"
    );
    assert_eq!(
        fs::read(destination.join("two.txt")).unwrap(),
        b"shared prefix shared prefix shared prefix beta\n"
    );
    let test = engine.test(&solid, None, &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.entries_checked, 2);
}

#[test]
fn rar5_encrypted_content_requires_the_correct_password() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("secret.rar");
    let engine = registry();
    let ctx = OperationContext::new("rar5-encrypted-content");

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.encrypted_entries, 3);
    assert!(
        listing
            .warnings
            .iter()
            .any(|warning| warning.contains("A password is needed"))
    );

    let destination = temp.path().join("out");
    let error = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "password_required");
    assert!(!destination.join("docs/readme.txt").exists());

    let mut options = extract_options(&archive, &destination);
    options.password = Some("wrong".into());
    let error = engine.extract(&options, &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password");
    assert!(!destination.join("docs/readme.txt").exists());

    options.password = Some(RAR5_PASSWORD.into());
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 3);
    assert_eq!(
        fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
        "hello rar\n"
    );

    let test = engine.test(&archive, Some(RAR5_PASSWORD), &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.encrypted_entries, 3);

    let error = engine.test(&archive, None, &ctx).unwrap_err();
    assert_eq!(error.code(), "password_required");

    let error = engine.test(&archive, Some("wrong"), &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password");
}

#[test]
fn rar5_encrypted_headers_require_the_correct_password() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("secret-headers.rar");
    let engine = registry();
    let ctx = OperationContext::new("rar5-encrypted-headers");

    let error = engine.list(&archive, None).unwrap_err();
    assert_eq!(error.code(), "password_required");

    let error = engine.list(&archive, Some("wrong")).unwrap_err();
    assert_eq!(error.code(), "invalid_password");

    let listing = engine.list(&archive, Some(RAR5_PASSWORD)).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Rar5);
    assert_eq!(listing.entries.len(), 5);
    assert_eq!(listing.encrypted_entries, 3);

    let destination = temp.path().join("out");
    let mut options = extract_options(&archive, &destination);
    options.password = Some(RAR5_PASSWORD.into());
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 3);
    assert_eq!(
        fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
        "hello rar\n"
    );
    assert!(engine.test(&archive, Some(RAR5_PASSWORD), &ctx).unwrap().ok);
}

#[test]
fn rar4_encrypted_archives_require_the_correct_password() {
    let temp = TempDir::new().unwrap();
    let engine = registry();
    let ctx = OperationContext::new("rar4-encrypted");

    let content = fixture("legacy-secret.rar");
    let listing = engine.list(&content, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Rar);
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.encrypted_entries, 1);

    let destination = temp.path().join("content");
    let error = engine
        .extract(&extract_options(&content, &destination), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "password_required");

    let mut options = extract_options(&content, &destination);
    options.password = Some("wrong-password".into());
    let error = engine.extract(&options, &ctx).unwrap_err();
    assert_eq!(error.code(), "invalid_password");

    options.password = Some(LEGACY_PASSWORD.into());
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read(destination.join("hello.txt")).unwrap(),
        b"Hello, RAR 3.x fixture world.\n"
    );
    assert!(
        engine
            .test(&content, Some(LEGACY_PASSWORD), &ctx)
            .unwrap()
            .ok
    );

    let headers = fixture("legacy-secret-headers.rar");
    assert_eq!(
        engine.list(&headers, None).unwrap_err().code(),
        "password_required"
    );
    assert_eq!(
        engine.list(&headers, Some("wrong")).unwrap_err().code(),
        "invalid_password"
    );
    let listing = engine.list(&headers, Some(LEGACY_PASSWORD)).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "hello.txt");

    let destination = temp.path().join("headers");
    let mut options = extract_options(&headers, &destination);
    options.password = Some(LEGACY_PASSWORD.into());
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read(destination.join("hello.txt")).unwrap(),
        b"Hello, RAR 3.x fixture world.\n"
    );
}

#[test]
fn rar_multi_volume_sets_are_listed_but_not_extracted() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("legacy-split.rar");
    let engine = registry();
    let ctx = OperationContext::new("rar-multivolume");

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Rar);
    assert!(
        listing
            .warnings
            .iter()
            .any(|warning| warning.contains("multi-volume"))
    );

    let destination = temp.path().join("out");
    let error = engine
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
    assert!(error.to_string().contains("multi-volume"));
    assert!(!destination.exists());

    let error = engine.test(&archive, None, &ctx).unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
    assert!(error.to_string().contains("multi-volume"));
}

#[test]
fn rar_rejects_path_traversal_before_writing() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("hostile.rar");
    fs::write(&archive, hostile_rar()).unwrap();

    let engine = registry();
    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "../hi.txt");
    assert!(
        listing
            .warnings
            .iter()
            .any(|warning| warning.contains("unsafe paths"))
    );

    let destination = temp.path().join("out");
    let error = engine
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("rar-traversal"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "path_traversal");
    assert!(!temp.path().join("hi.txt").exists());
    assert!(!destination.join("hi.txt").exists());
}

#[test]
fn rar_extraction_honours_skip_and_rename_overwrite_policies() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("sample.rar");
    let destination = temp.path().join("out");
    fs::create_dir_all(destination.join("docs")).unwrap();
    fs::write(destination.join("docs/readme.txt"), b"keep me").unwrap();

    let engine = registry();
    let ctx = OperationContext::new("rar-overwrite");

    let mut options = extract_options(&archive, &destination);
    options.overwrite = OverwritePolicy::Skip;
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.skipped, 1);
    assert_eq!(report.extracted_files, 2);
    assert_eq!(
        fs::read(destination.join("docs/readme.txt")).unwrap(),
        b"keep me"
    );

    options.overwrite = OverwritePolicy::Rename;
    let report = engine.extract(&options, &ctx).unwrap();
    assert_eq!(report.skipped, 0);
    assert_eq!(
        fs::read(destination.join("docs/readme.txt")).unwrap(),
        b"keep me"
    );
    assert!(destination.join("docs/readme (1).txt").exists());
}

#[test]
fn rar_capabilities_are_read_only_and_creation_is_rejected() {
    let engine = registry();
    for format in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
        let capability = engine
            .capabilities()
            .into_iter()
            .find(|capability| capability.format == format)
            .expect("RAR capability present");
        assert!(capability.can_read);
        assert!(capability.can_test);
        assert!(capability.encryption);
        assert!(!capability.can_create);
        assert!(!capability.can_modify);
        assert!(!capability.multi_volume);
        assert_eq!(capability.status, CapabilityStatus::ReadOnly);
    }

    let temp = TempDir::new().unwrap();
    for format in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
        let output = temp.path().join(format!("new.{}", format.extensions()[0]));
        let error = engine
            .create(
                &create_options(&output, format),
                &OperationContext::new("rar-create"),
            )
            .unwrap_err();
        assert_eq!(error.code(), "unsupported_operation");
        assert!(!output.exists());
    }
}

#[test]
fn rar_cancellation_is_honoured_before_writing() {
    let temp = TempDir::new().unwrap();
    let archive = fixture("sample.rar");
    let destination = temp.path().join("out");
    let ctx = OperationContext::new("rar-cancel");
    ctx.cancel();

    let error = registry()
        .extract(&extract_options(&archive, &destination), &ctx)
        .unwrap_err();
    assert_eq!(error.code(), "cancelled");
    assert!(!destination.exists());
}

/// Rewrites the stored `hello.txt` fixture name to `../hi.txt` and fixes up
/// both CRCs so the archive stays structurally valid while carrying an unsafe
/// path. CRC32 is implemented locally to avoid a test-only dependency.
fn hostile_rar() -> Vec<u8> {
    const FROM: &[u8] = b"hello.txt";
    const TO: &[u8] = b"../hi.txt";
    assert_eq!(FROM.len(), TO.len());

    let mut bytes = fs::read(fixture("legacy-stored.rar")).unwrap();
    let name_offset = bytes
        .windows(FROM.len())
        .position(|window| window == FROM)
        .expect("fixture name");
    bytes[name_offset..name_offset + FROM.len()].copy_from_slice(TO);

    let mut offset = 7usize;
    loop {
        let flags = u16::from_le_bytes([bytes[offset + 3], bytes[offset + 4]]);
        let head_size = u16::from_le_bytes([bytes[offset + 5], bytes[offset + 6]]) as usize;
        let payload_size = if flags & 0x8000 != 0 {
            u32::from_le_bytes([
                bytes[offset + 7],
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
            ]) as usize
        } else {
            0
        };
        if name_offset >= offset && name_offset < offset + head_size {
            let crc = crc32(&bytes[offset + 2..offset + head_size]) & 0xffff;
            bytes[offset..offset + 2].copy_from_slice(&(crc as u16).to_le_bytes());
            break;
        }
        let end = offset + head_size + payload_size;
        assert!(end > offset && end <= bytes.len(), "RAR block walk failed");
        offset = end;
    }

    bytes
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
