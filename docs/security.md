# Security model

Archon's core assumption is that **every archive is hostile until proven otherwise**.
A ZIP can contain names designed for path traversal, device names, case collisions,
symlinks or decompression bombs. The engine validates the whole extraction plan before
it writes the first byte.

## Threats and mitigations

### Path traversal and absolute paths

`crates/archon-core/src/security.rs` normalises `\` to `/` and rejects:

- empty paths and NUL bytes;
- absolute paths (`/etc/passwd`, `\\server\share`, `C:\Windows`);
- drive-relative paths (`C:evil.txt`);
- any `..` component, including mixed separators such as `..\evil.txt`;
- components containing control characters.

`safe_join` is a second line of defence. It canonicalises the destination and the
nearest existing ancestor of the target, then verifies the resolved path still starts
with the destination root. This catches junctions, symlinks and other filesystem
redirections that pure string checks would miss.

### Windows-specific names

On Windows, components are rejected when they:

- contain `< > : " | ? *` (this also blocks NTFS alternate data streams);
- end with a space or a dot;
- use a reserved device name as their stem: `CON`, `PRN`, `AUX`, `NUL`,
  `COM1`–`COM9`, `LPT1`–`LPT9` (case-insensitive, including `nul.txt`).

### Symlinks

- Archive entries flagged as symlinks are never extracted. They are counted and
  reported as warnings.
- Source trees are walked with `follow_links(false)`; symlinked files and folders are
  skipped and reported.
- Archives therefore cannot create links that later redirect writes outside the
  destination.

### Collisions

Before writing, the engine builds a complete plan and rejects:

- two files mapping to the same path (case-insensitive on Windows);
- a file and a folder sharing a path;
- a file whose parent is also a file.

Case collisions in source trees are rejected during creation as well, so an archive
created by Archon extracts deterministically on Windows.

### Resource exhaustion

Default limits, applied to extraction and testing:

| Limit | Value |
| --- | --- |
| Entries per operation | 200,000 |
| Total uncompressed size | 100 GiB |
| Single entry uncompressed size | 50 GiB |
| Warnings retained in a report | 100 (then truncated with a notice) |

The declared size is validated before extraction, and each engine enforces the
decompressed-size limit while streaming. Data is streamed in 256 KiB chunks; files
are never buffered in memory.

Free space is checked with `fs2::available_space` before extraction. The check is
advisory (free space can change concurrently), so I/O errors are still handled.

### Partial writes and failures

- Creation writes to `.archon-<pid>-<n>.tmp` in the output folder. An existing output
  is deleted only after the new archive has been finished and flushed with `sync_all`.
- Adding or removing ZIP entries rewrites the whole archive into a temporary sibling
  file and swaps it in only after the rewrite succeeds, so a cancelled update leaves the
  original archive intact.
- Extraction writes each file to a temporary sibling, flushes it, then renames it into
  place. A corrupt or truncated entry is deleted instead of being left behind.
- If an operation is cancelled or fails, the current temporary file is removed and the
  error is returned as `cancelled` or the relevant typed error.

Replacing an existing file is not a single atomic swap: the destination is removed
immediately before the rename. On failure between those two steps, the destination may
be missing, but the new content is never partially written. Keep backups of valuable
data.

### Corruption and verification

`test` reads every selected entry, decrypts if needed and verifies checksums where the
format provides them. Corrupt entries are reported as typed issues; `test` exits
non-zero on failure. Listing only reads archive metadata and never executes archive
content.

### Passwords

- The CLI has **no `--password` flag**. Passwords come from `ARCHON_PASSWORD`, a single
  line on stdin (`--password-stdin`), or a hidden terminal prompt via `rpassword`.
- Environment values and stdin values are wrapped in `zeroize::Zeroizing` so they are
  wiped when dropped. Passwords are not included in progress events or reports.
- The desktop UI keeps passwords in React state only for the duration of the operation
  and never writes them to `localStorage` or disk. It prompts again on retry, up to
  three attempts for extract/test.
- ZIP AES-256 (AE-2) protects file contents but **not file names**, sizes or the
  archive structure. Legacy ZipCrypto is supported for reading but is cryptographically
  weak; do not rely on it.
- 7Z AES-256 passwords protect both content and (when enabled by the writer) headers.
- Password quality is the user's responsibility. Archon does not impose composition
  rules or store a recovery key.

### Application boundary

- Tauri commands are explicitly registered; the window capability only enables core,
  dialog and opener permissions.
- The CSP restricts scripts to the app bundle and connections to the local Tauri IPC
  and development server. There is no remote asset loading.
- Archive bytes never leave the machine. There is no telemetry, update check or
  account system.
- Recent archive paths are stored in browser `localStorage` on the local machine and
  can be cleared from the Welcome screen.

## Known limitations

- True multi-volume/spanned ZIP archives are not supported. Archon's split/join feature
  works on finished bytes (`.001`, `.002`, …) and does not claim archive-format
  compatibility.
- ZIP64 is handled by the underlying `zip` crate, but archives with unknown
  decompressed sizes may be rejected by the limit checks rather than guessed.
- No sandboxing beyond the OS process: a successfully validated path inside the chosen
  destination can still overwrite user data according to the selected overwrite policy.
- The application is not code-signed in this early build. Verify downloads against the
  published checksums.
- No security audit has been performed. Treat this build as early software.
