# Archon

Archon is an independent archive manager for Windows with a native desktop UI and a
scriptable command-line companion. It is built from scratch on Tauri 2, Rust and React —
no WinRAR code, branding, icons or other proprietary assets are used.

**Status: early access — ZIP is fully implemented and writable.** 7Z, RAR/RAR5 and the
TAR family (including GZ/BZ2/XZ/ZST streams) can be browsed, extracted and verified.
Every format Archon cannot yet read or create is labelled as such in the UI, the CLI
and this document. Nothing is faked.

## What works today

| Capability | ZIP | 7Z | RAR / RAR5 | TAR family | GZ / BZ2 / XZ / ZST | WIM / ISO |
| --- | --- | --- | --- | --- | --- | --- |
| Browse / list | Yes | Yes | Yes (single volume) | Yes | Yes | Recognised, planned |
| Extract | Yes | Yes | Yes (single volume) | Yes | Yes | No |
| Create | Yes | No | **Never** (proprietary) | No | No | No |
| Test / verify | Yes | Yes | Yes (single volume) | Yes | Yes | No |
| Add / remove entries | Yes (unencrypted) | No | No | No | No | No |
| Split / multi-volume | Split + join (application-level) | Split + join | Recognised; sets not extracted | Split + join | Split + join | Split + join |

ZIP support includes:

- Store and Deflate compression, with presets from Store to Ultra (Deflate 1–9).
- AES-256 (AE-2) encryption when creating archives.
- Reading AES and legacy ZipCrypto archives; passwords can be retried in the UI.
- Folder trees, empty folders, CRC verification and comment display.
- Adding files/folders to an existing archive and removing entries; the archive is
  rewritten atomically so an interrupted operation never leaves a half-written file.
- Application-level split/join into `.001`, `.002`, … volumes. Archon does not claim
  spanned-ZIP compatibility.
- Path-traversal protection, Windows reserved-name rejection, symlink skipping,
  decompression limits, disk-space checks and atomic (temp-file) extraction.
- Cancellable create, add, remove, extract and test jobs with live progress.

7Z support includes AES-256 encrypted archives (header and content), solid-archive
warnings and the same safety limits. RAR/RAR5 support uses the pure-Rust `rars` engine:
listing, extraction and CRC verification work for single-volume RAR 1.5–7 archives,
including encrypted entries and encrypted headers. TAR support covers TAR, TAR.GZ,
TAR.BZ2, TAR.XZ and the single-stream GZ/BZ2/XZ/ZST formats. These formats are
read-only in this build.


> Archon will never create RAR archives. RAR reading is performed by the independent,
> Apache-2.0 `rars` engine. Multi-volume RAR sets are recognised but not extracted yet;
> the metadata view tells you so instead of failing halfway through.

## Screenshots

Captured from the running Windows build (1220 × 749 client area):

| Welcome | Archive browser |
| --- | --- |
| ![Welcome screen with recent archives](docs/screenshots/welcome.png) | ![Archive view listing a folder inside a ZIP](docs/screenshots/archive.png) |

| Create | Formats |
| --- | --- |
| ![Create ZIP form with presets and encryption](docs/screenshots/create.png) | ![Live capability matrix](docs/screenshots/formats.png) |

![Extract dialog with selection, destination, overwrite policy and password field](docs/screenshots/extract-dialog.png)

The interface is a "brutal glass" workspace: translucent graphite surfaces with
backdrop blur, hard 1 px borders, offset shadows and amber accents across three views:
**Archive**, **Create** and **Formats**.

## Requirements

- Windows 10 or 11 (WebView2 runtime is present on current Windows builds).
- [Rust](https://rustup.rs/) 1.89 or newer with the MSVC toolchain.
- [Node.js](https://nodejs.org/) 22 or newer (developed with Node 24) and npm.
- Visual Studio Build Tools with the “Desktop development with C++” workload.

## Build and run

```powershell
git clone <repository-url> archon
cd archon
npm install
npm run tauri dev        # development window with hot reload
npm run tauri build      # release build; installers land in target/release/bundle
```

The desktop application embeds the built frontend. Use the Tauri CLI so the production
`custom-protocol` feature is enabled; a plain `cargo build` targets the dev server
instead.

```powershell
npm run tauri build -- --debug --no-bundle   # runnable target/debug/archon.exe
```

### Frontend-only checks

```powershell
npm run check            # TypeScript, no emit
npm run build            # production frontend bundle
```

### Tests

```powershell
cargo test -p archon-core
```

The suite contains 49 tests: 4 unit tests for path validation, plus integration tests
for ZIP round-trips/modification (18), 7Z (5), RAR/RAR5 (10), TAR and single streams
(8) and split/join (4). They cover AES and ZipCrypto passwords, traversal, collisions,
corruption, cancellation, overwrite policies, encrypted-archive rejection, empty
folders and unsupported formats.

## Command-line usage

The CLI binary is `archon-cli` (`cargo run -p archon-cli -- …`, or
`cargo build --release -p archon-cli` to produce `target/release/archon-cli.exe`;
`npm run tauri build` builds only the desktop app). It is intentionally named
differently from the desktop executable `archon.exe` so the two builds cannot overwrite
each other in a shared Cargo target directory.

```powershell
archon-cli list photos.zip
archon-cli info photos.zip
archon-cli create backup.zip .\Documents -p maximum
archon-cli extract backup.zip -o .\restored --overwrite rename
archon-cli test backup.zip

# Update and prune an existing ZIP (add creates the archive when absent)
archon-cli add backup.zip .\extra --json
archon-cli remove backup.zip docs/old

# Application-level volumes: split, then join
archon-cli split backup.zip --size-mb 25
archon-cli join backup.zip.001 -o backup-restored.zip

archon-cli list photos.zip --json
```

Useful `create` options: `--preset store|fast|normal|maximum|ultra`,
`--method store|deflate`, `--level 1-9`, `-x/--exclude <glob>` (repeatable),
`--force` to replace an existing output, `--encrypt` for AES-256.

Useful `extract` options: `--entry <path>` (repeatable; folders include their
children), `--overwrite overwrite|skip|rename` (default `rename`), `--flat` to
drop folders, `-o/--output <dir>`.

`add` and `remove` work on unencrypted ZIP archives only. They rewrite the archive
atomically through a temporary file, so a cancelled update leaves the original intact.
`split` writes `.001`, `.002`, … parts next to the archive; `join` requires the `.001`
part and refuses to overwrite an existing output.

### Passwords

Passwords are **never accepted as command-line arguments**, because arguments are
visible to other processes and often end up in shell history.

```powershell
# Preferred: environment variable
$env:ARCHON_PASSWORD = "correct horse battery staple"
archon-cli create secret.zip .\private --encrypt
archon-cli extract secret.zip -o .\out
Remove-Item Env:\ARCHON_PASSWORD

# Or pipe a single line on stdin
Get-Content .\password.txt | archon-cli test secret.zip --password-stdin
```

On an interactive terminal, Archon prompts securely (input hidden) when a password
is needed and not supplied. Passwords are held in zeroizing memory and are never
written to logs or progress events.

## Desktop workflow

1. **Archive** — open ZIP, 7Z, RAR/RAR5, TAR-family archives or GZ/BZ2/XZ/ZST streams
   and browse the folder tree, search, select files and extract or test them. ZIP
   archives can also be updated in place: add files/folders, remove selections and
   split the result into numbered volumes. Encrypted archives prompt for a password,
   which can be retried. Formats that are not implemented are rejected with the
   engine's honest “planned” error message instead of pretending to work.
2. **Create** — add files/folders (picker or drag-and-drop), choose a destination,
   preset, optional Deflate level, excludes and AES-256 encryption.
3. **Formats** — a live capability matrix read from the engine, so the UI cannot
   drift away from what the backend actually supports.

The top bar also exposes **Join volumes**, which reassembles `.001`, `.002`, … parts
into a single archive.

Recent archives are stored locally in `localStorage`; there is no telemetry,
account system or network access.

## Project layout

```
crates/archon-core/   Engine abstraction, ZIP/7Z/RAR/TAR engines, split/join, security, limits
crates/archon-cli/    `archon-cli` command-line interface
src-tauri/            Tauri commands, state, capabilities, icons, bundling
src/                  React + TypeScript desktop UI
docs/                 Architecture, formats, security, installation, development
```

See [docs/architecture.md](docs/architecture.md) for the design and data flow.

## Roadmap

- **Phase 2** — ~~TAR/TAR.GZ/TAR.BZ2/TAR.XZ, raw GZ/BZ2/XZ and 7Z reading~~ (done);
  ~~adding/removing ZIP entries~~ (done). Remaining: writing 7Z/TAR archives.
- **Phase 3** — ~~RAR/RAR5 reading and extraction through the independent `rars`
  engine~~ (done, single-volume). Remaining: multi-volume RAR sets. RAR creation is
  permanently out of scope.
- **Phase 4** — Windows integration: Explorer context menu, file associations,
  installer polish and code signing.

Multi-volume **RAR** sets are recognised but not extracted yet. Multi-volume **ZIP**
archives are still not supported either. Archon instead offers application-level
split/join of any finished archive, which is portable and does not pretend to be a
spanned or split native archive.

## Security

Archon treats every archive as untrusted input. See [SECURITY.md](SECURITY.md) and
[docs/security.md](docs/security.md) for the threat model and safeguards. To report
a vulnerability, follow the private disclosure process in `SECURITY.md`.

## License

MIT — see [LICENSE](LICENSE). Third-party components are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Archon is not affiliated with RARLAB, WinRAR, 7-Zip or any other archive tool.
