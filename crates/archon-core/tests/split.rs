use std::fs;
use std::path::Path;

use archon_core::split::{join_volumes, split_archive};
use tempfile::TempDir;

fn write_sample(path: &Path, size: usize) -> Vec<u8> {
    let bytes: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
    fs::write(path, &bytes).unwrap();
    bytes
}

#[test]
fn split_and_join_roundtrip() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("archive.zip");
    let original = write_sample(&archive, 25_000);

    let report = split_archive(&archive, 10_000).unwrap();
    assert_eq!(report.part_count, 3);
    assert_eq!(report.total_bytes, 25_000);
    assert_eq!(report.parts.len(), 3);
    assert_eq!(
        fs::metadata(temp.path().join("archive.zip.001").clone())
            .unwrap()
            .len(),
        10_000
    );
    assert_eq!(
        fs::metadata(temp.path().join("archive.zip.002").clone())
            .unwrap()
            .len(),
        10_000
    );
    assert_eq!(
        fs::metadata(temp.path().join("archive.zip.003").clone())
            .unwrap()
            .len(),
        5_000
    );

    let joined = temp.path().join("joined.zip");
    let report = join_volumes(&temp.path().join("archive.zip.001"), &joined).unwrap();
    assert_eq!(report.part_count, 3);
    assert_eq!(report.total_bytes, 25_000);
    assert_eq!(fs::read(&joined).unwrap(), original);
}

#[test]
fn split_refuses_to_overwrite_existing_parts() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("archive.zip");
    write_sample(&archive, 1_000);
    fs::write(temp.path().join("archive.zip.001"), b"existing").unwrap();

    let error = split_archive(&archive, 500).unwrap_err();
    assert_eq!(error.code(), "conflict");
    assert_eq!(
        fs::read(temp.path().join("archive.zip.001")).unwrap(),
        b"existing"
    );
    assert!(!temp.path().join("archive.zip.002").exists());
}

#[test]
fn join_requires_the_first_volume_and_missing_output() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("archive.zip");
    write_sample(&archive, 1_000);
    split_archive(&archive, 400).unwrap();

    let error = join_volumes(
        &temp.path().join("archive.zip.002"),
        &temp.path().join("out.zip"),
    )
    .unwrap_err();
    assert_eq!(error.code(), "configuration");

    let existing = temp.path().join("existing.zip");
    fs::write(&existing, b"keep").unwrap();
    let error = join_volumes(&temp.path().join("archive.zip.001"), &existing).unwrap_err();
    assert_eq!(error.code(), "conflict");
    assert_eq!(fs::read(&existing).unwrap(), b"keep");
}

#[test]
fn zero_sized_sources_split_into_one_empty_part() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("empty.zip");
    fs::write(&archive, b"").unwrap();

    let report = split_archive(&archive, 1_024).unwrap();
    assert_eq!(report.part_count, 1);
    assert_eq!(report.total_bytes, 0);
    assert_eq!(
        fs::metadata(temp.path().join("empty.zip.001"))
            .unwrap()
            .len(),
        0
    );
}
