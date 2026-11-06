use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use archon_core::{
    ArchiveFormat, CompressionPreset, CreateOptions, EngineRegistry, ExtractOptions,
    OperationContext, OverwritePolicy,
};
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
        format: ArchiveFormat::Tar,
        preset: CompressionPreset::Normal,
        method: None,
        level: None,
        password: None,
        exclude: Vec::new(),
        overwrite_output: true,
    }
}

/// Build a minimal ustar header without depending on the tar crate. This also
/// lets tests craft hostile names that a well-behaved writer would refuse.
fn tar_header(name: &str, size: u64, entry_type: u8, link: &str) -> [u8; 512] {
    let mut header = [0_u8; 512];
    let name_bytes = name.as_bytes();
    let copy = name_bytes.len().min(100);
    header[..copy].copy_from_slice(&name_bytes[..copy]);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_field = format!("{size:011o}\0");
    header[124..136].copy_from_slice(size_field.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = entry_type;
    let link_bytes = link.as_bytes();
    let link_copy = link_bytes.len().min(100);
    header[157..157 + link_copy].copy_from_slice(&link_bytes[..link_copy]);
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    let checksum_field = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum_field.as_bytes());
    header
}

fn push_entry(tar: &mut Vec<u8>, name: &str, contents: &[u8], entry_type: u8, link: &str) {
    tar.extend_from_slice(&tar_header(name, contents.len() as u64, entry_type, link));
    tar.extend_from_slice(contents);
    let padding = (512 - (contents.len() % 512)) % 512;
    tar.extend(std::iter::repeat_n(0_u8, padding));
}

fn finish_tar(mut tar: Vec<u8>) -> Vec<u8> {
    tar.extend(std::iter::repeat_n(0_u8, 1024));
    tar
}

fn sample_tar() -> Vec<u8> {
    let mut tar = Vec::new();
    push_entry(&mut tar, "docs/", b"", b'5', "");
    push_entry(&mut tar, "docs/readme.txt", b"hello tar\n", b'0', "");
    push_entry(&mut tar, "docs/nested/data.bin", &[0x5a; 70_000], b'0', "");
    push_entry(&mut tar, "docs/empty.txt", b"", b'0', "");
    finish_tar(tar)
}

fn compress(format: ArchiveFormat, bytes: &[u8]) -> Vec<u8> {
    match format {
        ArchiveFormat::TarGz => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::TarBz2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::TarXz => {
            let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::Zst => zstd::stream::encode_all(bytes, 3).unwrap(),
        _ => panic!("unsupported compression in test"),
    }
}

fn compress_single(format: ArchiveFormat, bytes: &[u8]) -> Vec<u8> {
    match format {
        ArchiveFormat::Gz => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::Bz2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::Xz => {
            let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveFormat::Zst => zstd::stream::encode_all(bytes, 3).unwrap(),
        _ => panic!("unsupported compression in test"),
    }
}

#[test]
fn tar_list_extract_and_test_roundtrip() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("sample.tar");
    fs::write(&archive, sample_tar()).unwrap();
    let engine = registry();
    let ctx = OperationContext::new("tar-roundtrip");

    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.format, ArchiveFormat::Tar);
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
        "hello tar\n"
    );
    assert_eq!(
        fs::read(destination.join("docs/nested/data.bin"))
            .unwrap()
            .len(),
        70_000
    );
    assert!(destination.join("docs/empty.txt").exists());

    let test = engine.test(&archive, None, &ctx).unwrap();
    assert!(test.ok);
    assert_eq!(test.entries_checked, 4);

    let info = engine.info(&archive, None).unwrap();
    assert_eq!(info.file_count, 3);
    assert_eq!(info.dir_count, 1);
}

#[test]
fn tar_is_read_only_for_creation() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("input");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), b"a").unwrap();
    let output = temp.path().join("new.tar");

    let error = registry()
        .create(
            &create_options(vec![source], &output),
            &OperationContext::new("tar-create"),
        )
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_operation");
    assert!(!output.exists());
}

#[test]
fn tar_rejects_path_traversal_before_writing() {
    let temp = TempDir::new().unwrap();
    let mut tar = Vec::new();
    push_entry(&mut tar, "../escape.txt", b"owned", b'0', "");
    push_entry(&mut tar, "safe.txt", b"safe", b'0', "");
    let archive = temp.path().join("hostile.tar");
    fs::write(&archive, finish_tar(tar)).unwrap();

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
fn tar_skips_the_root_entry_written_as_dot() {
    let temp = TempDir::new().unwrap();
    let mut tar = Vec::new();
    push_entry(&mut tar, "./", b"", b'5', "");
    push_entry(&mut tar, "./note.txt", b"hello\n", b'0', "");
    let archive = temp.path().join("dot-root.tar");
    fs::write(&archive, finish_tar(tar)).unwrap();

    let engine = registry();
    let listing = engine.list(&archive, None).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "./note.txt");
    assert!(listing.warnings.is_empty());

    let destination = temp.path().join("out");
    let report = engine
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("dot-root"),
        )
        .unwrap();
    assert_eq!(report.extracted_files, 1);
    assert_eq!(
        fs::read_to_string(destination.join("note.txt")).unwrap(),
        "hello\n"
    );

    let test = engine
        .test(&archive, None, &OperationContext::new("dot-root-test"))
        .unwrap();
    assert!(test.ok);
    assert_eq!(test.entries_checked, 1);
}

#[test]
fn tar_skips_symlinks_without_creating_them() {
    let temp = TempDir::new().unwrap();
    let mut tar = Vec::new();
    push_entry(&mut tar, "link", b"", b'2', "target.txt");
    push_entry(&mut tar, "target.txt", b"target", b'0', "");
    let archive = temp.path().join("links.tar");
    fs::write(&archive, finish_tar(tar)).unwrap();

    let engine = registry();
    let listing = engine.list(&archive, None).unwrap();
    assert!(
        listing
            .entries
            .iter()
            .any(|entry| entry.path == "link" && entry.is_symlink)
    );
    assert!(
        listing
            .warnings
            .iter()
            .any(|value| value.contains("link entr"))
    );

    let destination = temp.path().join("out");
    let report = engine
        .extract(
            &extract_options(&archive, &destination),
            &OperationContext::new("links"),
        )
        .unwrap();
    assert_eq!(report.extracted_files, 1);
    assert!(destination.join("target.txt").exists());
    assert!(!destination.join("link").exists());
}

#[test]
fn compressed_tar_variants_are_readable() {
    let temp = TempDir::new().unwrap();
    let engine = registry();
    let tar = sample_tar();

    for (format, name) in [
        (ArchiveFormat::TarGz, "sample.tar.gz"),
        (ArchiveFormat::TarBz2, "sample.tar.bz2"),
        (ArchiveFormat::TarXz, "sample.tar.xz"),
    ] {
        let archive = temp.path().join(name);
        fs::write(&archive, compress(format, &tar)).unwrap();

        let listing = engine.list(&archive, None).unwrap();
        assert_eq!(listing.format, format);
        assert!(
            listing
                .entries
                .iter()
                .any(|entry| entry.path == "docs/readme.txt")
        );

        let destination = temp.path().join(format!("out-{name}"));
        let report = engine
            .extract(
                &extract_options(&archive, &destination),
                &OperationContext::new("compressed-tar"),
            )
            .unwrap();
        assert_eq!(report.extracted_files, 3);
        assert_eq!(
            fs::read_to_string(destination.join("docs/readme.txt")).unwrap(),
            "hello tar\n"
        );
        assert!(
            engine
                .test(&archive, None, &OperationContext::new("t"))
                .unwrap()
                .ok
        );
    }
}

#[test]
fn single_stream_formats_decompress_to_named_entry() {
    let temp = TempDir::new().unwrap();
    let engine = registry();

    for (format, suffix) in [
        (ArchiveFormat::Gz, "gz"),
        (ArchiveFormat::Bz2, "bz2"),
        (ArchiveFormat::Xz, "xz"),
        (ArchiveFormat::Zst, "zst"),
    ] {
        let archive = temp.path().join(format!("note.txt.{suffix}"));
        fs::write(&archive, compress_single(format, b"stream contents")).unwrap();

        let listing = engine.list(&archive, None).unwrap();
        assert_eq!(listing.format, format);
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "note.txt");

        let destination = temp.path().join(format!("out-{suffix}"));
        let report = engine
            .extract(
                &extract_options(&archive, &destination),
                &OperationContext::new("stream"),
            )
            .unwrap();
        assert_eq!(report.extracted_files, 1);
        assert_eq!(
            fs::read_to_string(destination.join("note.txt")).unwrap(),
            "stream contents"
        );

        let test = engine
            .test(&archive, None, &OperationContext::new("stream-test"))
            .unwrap();
        assert!(test.ok);
        assert_eq!(test.entries_checked, 1);
    }
}

#[test]
fn truncated_tar_is_reported_as_invalid() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("broken.tar");
    let mut header = tar_header("broken.txt", 4, b'0', "");
    header[148] = b'9';
    header[149] = b'9';
    fs::write(&archive, header).unwrap();

    let error = registry().list(&archive, None).unwrap_err();
    assert_eq!(error.code(), "invalid_archive");
}
