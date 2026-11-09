# Format support

This page reflects the code in `archon-core`. The **Formats** view in the desktop app
reads the same registry at runtime, so the UI cannot claim more than the engine does.

Legend: **Yes** = implemented and tested. **Read-only** = browse, extract and verify
only. **No** = not implemented. **Never** = will not be implemented because the format
is proprietary to create.

| Format | Read/list | Extract | Create | Test | Modify | Encryption | Multi-volume | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ZIP | Yes | Yes | Yes | Yes | Yes (unencrypted) | Read AES + ZipCrypto; create AES-256 (AE-2) | Split/join only | Supported |
| 7Z | Yes | Yes | No | Yes | No | Read AES-256 (headers and content) | No | Read-only |
| RAR | Yes | Yes | **Never** | Yes | No | Read encrypted content and headers | Recognised, not extracted | Read-only |
| RAR5 | Yes | Yes | **Never** | Yes | No | Read AES-256 encrypted content and headers | Recognised, not extracted | Read-only |
| TAR | Yes | Yes | No | Yes | No | — | No | Read-only |
| TAR.GZ / TGZ | Yes | Yes | No | Yes | No | — | No | Read-only |
| TAR.BZ2 / TBZ2 | Yes | Yes | No | Yes | No | — | No | Read-only |
| TAR.XZ / TXZ | Yes | Yes | No | Yes | No | — | No | Read-only |
| GZ | Yes | Yes | No | Yes | No | — | No | Read-only |
| BZ2 | Yes | Yes | No | Yes | No | — | No | Read-only |
| XZ | Yes | Yes | No | Yes | No | — | No | Read-only |
| ZST | Yes | Yes | No | Yes | No | — | No | Read-only |
| WIM | No | No | No | No | No | — | No | Planned |
| ISO | No | No | No | No | No | — | No | Planned |

"Split/join only" means Archon can split any finished file into numbered
`.001`, `.002`, … volumes and join them back. This is **application-level** splitting,
not the ZIP spec's multi-volume/spanned format, and is not claimed as such.

## ZIP details

### Reading

- Standard ZIP and ZIP64 central directories.
- Store, Deflate, AES (AE-1/AE-2) and legacy ZipCrypto entries.
- ZIP comments are decoded to UTF-8 when possible.
- Unsupported compression methods return a typed `unsupported_operation` error naming
  the method ID.
- The first 64 KiB is inspected for magic bytes. A ZIP renamed to `.bin` is still
  recognised; a text file named `.zip` is rejected.

### Creating

- Store or Deflate (levels 1–9).
- Presets:

  | Preset | Method | Level |
  | --- | --- | --- |
  | Store | Store | — |
  | Fast | Deflate | 1 |
  | Normal | Deflate | 6 |
  | Maximum | Deflate | 9 |
  | Ultra | Deflate | 9 (with a warning) |

- `--method` overrides the preset. Store with an explicit level is a configuration
  error; Deflate levels outside 1–9 are rejected.
- Explicit directory entries, including empty folders.
- Exclude globs are matched against both the full archive path and the file name.
- Existing output is replaced only with explicit overwrite permission (`--force` / UI
  checkbox).
- Missing output folders are created automatically.
- Output without `.zip` is given the extension by the desktop UI; the CLI uses the
  path exactly as given.

### Modifying

- Add files or folders to an existing ZIP; entries with the same archive path are
  replaced.
- Remove individual entries or whole folders (children included).
- `add` on a missing archive creates it, which matches WinRAR-style "update" behaviour.
- Every change is written to a temporary file next to the archive and then swapped in,
  so an interrupted or cancelled operation leaves the original archive intact.
- Encrypted archives (AES or ZipCrypto) are **not** modified in place; Archon returns
  `unsupported_operation` and suggests extracting and recreating instead.
- Unchanged entries are copied with their original compressed bytes, so modifying a
  large archive does not recompress everything.

### Not implemented

- Split/multi-volume **ZIP** creation or extraction (use application-level split/join).
- Modifying encrypted archives in place.
- ZIP archives using unsupported legacy compression methods.
- Preserving NTFS ACLs, alternate streams or symlinks.
- Encrypting file names (the ZIP format does not support this).

## 7Z details

- Listing, extraction and integrity testing via `sevenz-rust2`.
- AES-256 encrypted archives, including encrypted headers. A password is required to
  list such an archive and is prompted for in the UI and CLI.
- Wrong or missing passwords return `invalid_password` / `password_required`.
- Solid archives are listed with a warning: extracting a single member may decompress
  more data than its own size.
- Same path-traversal, reserved-name, decompression-limit and atomic-extraction
  safeguards as ZIP.
- Creating or modifying 7z archives is not implemented.

## TAR family and single streams

- TAR, TAR.GZ, TAR.BZ2, TAR.XZ: listing, extraction and integrity testing.
- GZ, BZ2, XZ, ZST: single-file streams are presented as a one-entry archive and
  extracted under the stream's base name.
- TAR stores no per-entry checksum; `test` verifies structure and decompression rather
  than CRCs, and the UI/CLI wording reflects that.
- No encryption support (the base formats have none).
- Creating or modifying these formats is not implemented.

## RAR details

- Listing, extraction and integrity testing via the pure-Rust `rars` engine.
- Covers RAR 1.5–4.x and RAR5 archives, including solid archives.
- Password-protected archives are read with AES-encrypted content and encrypted
  headers. Wrong or missing passwords return `invalid_password` / `password_required`.
- Multi-volume RAR sets are recognised and listed, but extraction and testing return an
  explicit `unsupported_operation` message rather than pretending to work.
- Same path-traversal, reserved-name, decompression-limit and atomic-extraction
  safeguards as ZIP.
- Creating or modifying RAR archives is not implemented.

## Why RAR creation is out of scope

RAR is a proprietary format. Archon will never create RAR archives. Reading is
performed by the independent, Apache-2.0 `rars` engine; no RARLAB code is used.

## Detection order

1. Magic bytes (ZIP, RAR/RAR5, 7Z, GZIP, BZIP2, XZ, Zstandard, TAR `ustar`, ISO
   `CD001`, WIM).
2. File extension for formats that share a container signature (`.tar.gz` vs `.gz`).

If neither recognises the file, the error is `unsupported_format` with a message that
the signature is unknown.
