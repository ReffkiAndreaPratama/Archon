# Architecture

Archon is a Rust workspace with a React frontend. The archive engine is a library, so
the desktop app and the CLI share exactly the same behaviour and security checks.

```
┌──────────────────────────┐      ┌──────────────────────────┐
│  React UI (src/)         │      │  archon CLI              │
│  Tauri invoke / events   │      │  crates/archon-cli       │
└────────────┬─────────────┘      └────────────┬─────────────┘
             │                                  │
             │ Tauri commands + progress event  │ direct library calls
             ▼                                  ▼
┌────────────────────────────────────────────────────────────┐
│  archon-core                                               │
│  EngineRegistry → ArchiveEngine (trait) → ZipEngine        │
│  security · limits · progress · fs utilities               │
└────────────────────────────────────────────────────────────┘
```

## Crates

| Path | Responsibility |
| --- | --- |
| `crates/archon-core` | Format model, engine abstraction, ZIP engine, path security, limits, atomic writes, cancellation and progress. |
| `crates/archon-cli` | `archon-cli` binary: list/info/extract/create/test, JSON output, password acquisition, terminal progress. |
| `src-tauri` | Tauri 2 shell, command handlers, job registry, capability permissions, bundling. |
| `src` | React 19 + TypeScript UI: archive browser, creation form, capability matrix. |

Dependencies are pinned in `Cargo.lock` and `package-lock.json`.

## Engine abstraction

`ArchiveEngine` (`crates/archon-core/src/engine.rs`) defines:

```rust
fn id(&self) -> &'static str;
fn capabilities(&self) -> Vec<FormatCapability>;
fn list(&self, archive: &Path) -> ArchonResult<ArchiveListing>;
fn info(&self, archive: &Path) -> ArchonResult<ArchiveInfo>;
fn create(&self, options: &CreateOptions, ctx: &OperationContext) -> ArchonResult<CreateReport>;
fn extract(&self, options: &ExtractOptions, ctx: &OperationContext) -> ArchonResult<ExtractReport>;
fn test(&self, archive: &Path, password: Option<&str>, ctx: &OperationContext) -> ArchonResult<TestReport>;
```

`add_entries` and `remove_entries` exist with default implementations that return
`NotImplemented`; no build currently overrides them. This makes the Phase 1 boundary
explicit instead of silently doing nothing.

`EngineRegistry` owns the adapters and provides the capability registry. Formats with
no engine are still recognised (by magic bytes first, then extension) and produce a
typed `unsupported_format` error explaining which phase is planned. RAR creation is
permanently excluded.

### Error model

`ArchonError` has a stable machine-readable `code()`:

```
io, not_found, invalid_archive, unsupported_format, unsupported_operation,
password_required, invalid_password, path_traversal, limit_exceeded,
conflict, cancelled, insufficient_space, configuration, not_implemented, engine
```

The Tauri layer serialises errors as `{ "code": "...", "message": "..." }`. The UI
uses the code for behaviour (for example, prompting for a password) and the message
for display. The CLI prints the human message and exits non-zero.

## ZIP engine

- `zip` 9.2 with `deflate-flate2-zlib-rs` (zlib-rs backend), `aes-crypto` and `time`.
- **Create:** Store or Deflate, levels 1–9. Presets map to Store / 1 / 6 / 9 / 9
  (“Ultra” is Deflate 9 and emits a warning saying so). Directories are stored
  explicitly, including empty ones. Entry names always use `/`.
- **Encryption:** new archives use AES-256 (AE-2). Existing AES and ZipCrypto
  archives can be read and tested.
- **Listing:** reads the central directory only; no decompression. Unsafe names,
  symlinks, invalid UTF-8 and split-volume-looking file names become warnings.
- **Extraction:** fully plans and validates targets, checks limits and free space,
  then streams entries to temporary files.
- **Test:** streams every selected entry through the decompressor and checks the CRC.
- **Not implemented:** modifying existing archives, split volumes, data descriptors
  with unknown sizes beyond the crate's support, and every non-ZIP format.

## Data flow of a desktop operation

1. The UI builds a typed request (`src/services/api.ts`) and a fresh job ID.
2. `invoke(...)` calls a Tauri command in `src-tauri/src/commands.rs`.
3. The command registers an `AtomicBool` cancellation token in `AppState`, builds an
   `OperationContext` and runs the engine on `spawn_blocking`.
4. The engine reports `Progress` events; the command forwards them as
   `archon://progress`, tagged with the job ID.
5. The UI filters events by job ID, updates the progress panel and can call
   `cancel_job(job_id)`.
6. The command returns a typed report or a typed error; the job token is removed.

CPU-bound archive work never runs on the UI thread. A second operation is blocked by
the UI while one is active, and each view keeps its state so navigation does not lose
work.

## Frontend structure

```
src/App.tsx                 application state, operations, drag-and-drop, dialogs
src/components/ArchiveView  tree, breadcrumbs, search, sort, selection, inspector
src/components/CreateView   sources, destination, presets, encryption, excludes
src/components/FormatsView  live capability matrix
src/components/*Dialog      extract, password, test result, modal
src/lib/format.ts           byte/time/path formatting, defaults
src/lib/tree.ts             archive tree construction and filtering
src/services/api.ts         typed invoke/event wrappers
src/styles.css              design system and responsive layout
```

The capability matrix is fetched from the engine, so a format cannot be presented as
supported merely because a developer wrote it in the UI.

## Design decisions

- **Honest capabilities over visual parity.** Controls exist only when the backend can
  perform the operation. Unsupported formats are still openable so the user sees the
  real reason.
- **One engine, two frontends.** The CLI and desktop app cannot drift in validation or
  security behaviour.
- **Validate the plan before writing.** Failures happen before user files are touched
  wherever possible.
- **No hidden network or telemetry.** The app is local-only by construction.
- **No invented numbers.** Sizes, ratios and elapsed times come from real work.
