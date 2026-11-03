# Development

## Prerequisites

- Rust 1.88+ (MSVC toolchain) and Cargo.
- Node.js 22+ and npm.
- Visual Studio Build Tools with the C++ workload.
- WebView2 runtime for running the desktop app.
- PowerShell examples below assume Windows.

## First setup

```powershell
npm install
cargo fetch
```

## Everyday commands

| Task | Command |
| --- | --- |
| TypeScript check | `npm run check` |
| Production frontend build | `npm run build` |
| Desktop dev window | `npm run tauri dev` |
| Debug desktop build (no installers) | `npm run tauri build -- --debug --no-bundle` |
| Release build + installers | `npm run tauri build` |
| Core tests | `cargo test -p archon-core` |
| Whole workspace check | `cargo check --workspace --all-targets` |
| CLI build | `cargo build -p archon-cli` |
| CLI tests (manual) | see below |

The first `vite build` after a dependency change can take a minute because Tailwind v4
scans the source tree. Warm builds are much faster.

## Testing

### Automated

```powershell
cargo test -p archon-core
```

The current suite (18 tests) covers:

- **Unit (`security.rs`)** — traversal and absolute paths, Windows reserved names,
  alternate data streams, trailing dots/spaces, empty paths.
- **Integration (`tests/zip_engine.rs`)** — create/list/extract/test round-trips,
  empty folders, Store preset, excludes, Deflate levels, cancellation, AES and
  ZipCrypto password required/wrong/correct, corrupted entries, symlink entries,
  overwrite policies, selected entries, path traversal, duplicate/case-colliding
  entries, unsupported formats and honest errors.

Tests create their own temporary directories and do not touch user data.

### Manual CLI smoke test

```powershell
cargo build -p archon-cli
$cl = ".\target\debug\archon-cli.exe"
$work = Join-Path $env:TEMP "archon-smoke"
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path "$work\src\nested" -Force | Out-Null
"hello" | Set-Content "$work\src\a.txt"
"world" | Set-Content "$work\src\nested\b.txt"

& $cl create "$work\test.zip" "$work\src" -p normal
& $cl list "$work\test.zip"
& $cl test "$work\test.zip"
& $cl extract "$work\test.zip" -o "$work\out"
Get-ChildItem -Recurse "$work\out"

# Encrypted round-trip (password never appears in the command line)
$env:ARCHON_PASSWORD = "smoke-test-password"
& $cl create "$work\secret.zip" "$work\src" --encrypt
& $cl test "$work\secret.zip"
& $cl extract "$work\secret.zip" -o "$work\secret-out"
Remove-Item Env:\ARCHON_PASSWORD

# Failure paths
& $cl create "$work\test.zip" "$work\src"   # conflict: exists
& $cl list "$work\nope.zip"                 # not found
& $cl test "$work\secret.zip"                # password_required (non-interactive)
```

JSON output is available with `--json` for list, info, create, extract and test.

## Project conventions

- Rust 2024 edition, `rustfmt` defaults, warnings denied in review.
- Keep all archive behaviour in `archon-core`; the Tauri layer only adapts types,
  schedules work and emits progress.
- New user-visible operations need: a core method, a Tauri command, a typed frontend
  wrapper, UI controls, honest capability text and tests.
- Never add a capability flag for something without a working path and a test.
- No secrets in logs, reports, errors or command arguments.
- Frontend styling lives in `src/styles.css` using the existing design tokens; avoid
  one-off colors.
- Prefer icons from `lucide-react`; do not copy third-party app artwork.

### Adding a Tauri command

1. Implement the operation in `archon-core` behind the engine trait.
2. Add `#[tauri::command]` in `src-tauri/src/commands.rs`, running blocking work on
   `spawn_blocking`.
3. Register it in `src-tauri/src/lib.rs`.
4. Add the typed wrapper in `src/services/api.ts`.
5. Add permissions to `src-tauri/capabilities/default.json` only if a plugin needs them.
6. Test the engine path directly; the Tauri adapter should stay thin.

### Adding an engine (Phase 2+)

Implement `ArchiveEngine`, add it to `EngineRegistry`, provide real
`FormatCapability` values, extend detection, and add integration tests before exposing
it in the UI. Reject creation for proprietary formats.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `cargo` not found | Add `%USERPROFILE%\.cargo\bin` to `PATH` or restart the terminal. |
| `link.exe` not found | Install Visual Studio Build Tools with the C++ workload. |
| Blank desktop window | Install/repair the WebView2 runtime. |
| Port 1420 already in use | Stop the other Vite/Tauri dev process; `strictPort` is enabled. |
| Tailwind changes not applied | Ensure `src/main.tsx` imports `./styles.css` and the `@tailwindcss/vite` plugin is enabled. |
| Slow first build | Expected; `zip`, Tauri and WebView dependencies are large. |
| `cargo check` prints warnings in PowerShell and exits 1 | Cargo writes progress to stderr; check for the final `Finished` line rather than the PowerShell exit code. |

## Release checklist

1. `npm run check && npm run build`.
2. `cargo test -p archon-core`.
3. `cargo check --workspace --all-targets`.
4. Run the manual CLI smoke test.
5. `npm run tauri build`; launch the installed app and verify Archive/Create/Formats.
6. Update `CHANGELOG.md` and the version in `package.json`, `Cargo.toml` and
   `src-tauri/tauri.conf.json`.
7. Commit, tag and publish checksums.
