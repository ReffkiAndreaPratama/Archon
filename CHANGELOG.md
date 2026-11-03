# Changelog

All notable changes to Archon are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] — 2026-11-02

First public Phase 1 release. ZIP is the only supported archive format.

### Added

- Native desktop app built with Tauri 2, React 19, TypeScript and Vite, with a custom
  charcoal/graphite and amber-copper interface.
- Archive view: open ZIP files, folder tree, breadcrumbs, search, sorting, multi-select,
  file inspector, extract and integrity test.
- Create view: file/folder sources with drag-and-drop, destination picker, Store/Fast/
  Normal/Maximum/Ultra presets, explicit Deflate level, exclude globs, overwrite control
  and AES-256 (AE-2) encryption with confirmation.
- Formats view driven by the engine capability registry, including honest “planned”
  and “never” states for unsupported formats.
- Shared `archon-core` engine with an `ArchiveEngine` trait, typed errors and stable
  error codes.
- ZIP support: listing, info, creation, extraction, testing, ZIP64, Store/Deflate,
  AES and ZipCrypto decryption, AES-256 encryption, comments and timestamps.
- Security: path-traversal and absolute-path rejection, Windows reserved names,
  alternate-data-stream blocking, symlink skipping, case-collision detection,
  decompression limits, free-space checks and temp-file extraction.
- Cancellable operations with live progress over the `archon://progress` event.
- `archon` CLI with `list`, `info`, `extract`, `create` and `test`, JSON output,
  terminal progress and password input from `ARCHON_PASSWORD`, stdin or a hidden prompt.
  The binary is `archon-cli`, distinct from the desktop `archon.exe`.
- 18 automated tests (4 security unit tests, 14 ZIP integration tests).
- Documentation: architecture, formats, security, installation, development,
  contribution guide and third-party notices.

### Known limitations

- No 7Z, RAR, TAR, GZ, BZ2, XZ, ZST, WIM or ISO reading or extraction.
- No archive creation for any format other than ZIP.
- RAR creation will never be implemented.
- No adding, removing or updating entries in existing archives.
- No split/multi-volume archives.
- No Windows Explorer context-menu integration or file associations.
- Builds are not code-signed; no automated update mechanism.
