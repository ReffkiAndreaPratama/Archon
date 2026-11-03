# Format support

This page reflects the code in `archon-core`. The **Formats** view in the desktop app
reads the same registry at runtime, so the UI cannot claim more than the engine does.

Legend: **Yes** = implemented and tested. **No** = not implemented. **Never** = will not
be implemented because the format is proprietary to create.

| Format | Read/list | Extract | Create | Test | Encryption | Multi-volume | Phase |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ZIP | Yes | Yes | Yes | Yes | Read AES + ZipCrypto; create AES-256 (AE-2) | No | 1 |
| 7Z | No | No | No | No | — | No | 2 (planned) |
| RAR | No | No | **Never** | No | — | No | 3 (extraction only) |
| RAR5 | No | No | **Never** | No | — | No | 3 (extraction only) |
| TAR | No | No | No | No | — | No | 2 (planned) |
| TAR.GZ / TGZ | No | No | No | No | — | No | 2 (planned) |
| TAR.BZ2 / TBZ2 | No | No | No | No | — | No | 2 (planned) |
| TAR.XZ / TXZ | No | No | No | No | — | No | 2 (planned) |
| GZ | No | No | No | No | — | No | 2 (planned) |
| BZ2 | No | No | No | No | — | No | 2 (planned) |
| XZ | No | No | No | No | — | No | 2 (planned) |
| ZST | No | No | No | No | — | No | later |
| WIM | No | No | No | No | — | No | later |
| ISO | No | No | No | No | — | No | later |

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
- Output without `.zip` is given the extension by the desktop UI; the CLI uses the
  path exactly as given.

### Not implemented

- Adding, deleting or updating entries in an existing archive.
- Split/multi-volume ZIP creation or extraction.
- ZIP archives using unsupported legacy compression methods.
- Preserving NTFS ACLs, alternate streams or symlinks.
- Encrypting file names (the ZIP format does not support this).

## Why RAR creation is out of scope

RAR is a proprietary format. Archon will never create RAR archives, and will only
extract them if a redistributable, legally compatible engine is integrated in a future
phase. Phase 1 provides no RAR extraction.

## Detection order

1. Magic bytes (ZIP, RAR/RAR5, 7Z, GZIP, BZIP2, XZ, Zstandard, TAR `ustar`, ISO
   `CD001`, WIM).
2. File extension for formats that share a container signature (`.tar.gz` vs `.gz`).

If neither recognises the file, the error is `unsupported_format` with a message that
the signature is unknown.
