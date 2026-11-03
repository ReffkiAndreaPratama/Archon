# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x (Phase 1) | Yes |

Archon is pre-1.0 software. Security fixes are applied to the latest release only.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Use GitHub's private vulnerability reporting for this repository
(“Security” → “Advisories” → “Report a vulnerability”), or email the maintainer
listed on the repository profile. Include:

- Archon version and how it was installed/built.
- Windows version and architecture.
- A minimal archive or precise steps that reproduce the problem.
- The impact you believe is possible.
- Whether you are willing to be credited.

You can expect an initial acknowledgement within 7 days and a status update within
30 days. Please allow reasonable time for a fix before public disclosure.

## Scope

In scope:

- Archive parsing, path handling, extraction and creation logic.
- Password handling and leakage into logs, process arguments or UI storage.
- Command/event boundary between the React UI and the Rust backend.
- Filesystem writes performed by Archon, including temporary files.

Out of scope:

- Attacks that require an already-compromised machine or administrator rights.
- Denial of service through intentionally huge inputs beyond documented limits
  (report it anyway if a limit can be bypassed).
- Weaknesses inherent to the ZIP format (for example, encrypted ZIP entries do not
  hide file names).
- Vulnerabilities in third-party dependencies that have no exploitable path in
  Archon; please report those upstream.

## Security design summary

- Archives are untrusted input. Absolute paths, `..` traversal, drive/UNC paths,
  NTFS alternate data streams, control characters and Windows reserved device names
  are rejected before any write.
- Symbolic links are never created from archives and are skipped when scanning
  sources.
- Extraction enforces limits (200,000 entries, 50 GiB per file, 100 GiB total) and
  checks free disk space before writing.
- Files are written to a temporary sibling file and moved into place only after the
  complete entry has been read and validated; failed entries are deleted.
- Passwords are never accepted as CLI arguments. They come from `ARCHON_PASSWORD`,
  a single stdin line, or a hidden interactive prompt, are kept in zeroizing memory,
  and are never logged.
- The desktop app does not use a network client, telemetry or remote content. The
  Tauri content-security policy restricts scripts and connections to the app itself.

See [docs/security.md](docs/security.md) for the full threat model.
