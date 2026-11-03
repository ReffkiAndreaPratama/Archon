# Installation

## Supported systems

- Windows 10 (1809 or newer) or Windows 11, 64-bit.
- The Microsoft Edge WebView2 runtime. It ships with Windows 11 and current Windows 10;
  if the app window is blank or fails to start, install the Evergreen WebView2 Runtime
  from Microsoft.
- No administrator rights are required for a per-user install.

Archon 0.1.0 is not code-signed. Windows SmartScreen may show “Windows protected your
PC” when running a locally built or downloaded executable. Verify the file against the
published checksum before choosing “More info → Run anyway”.

## Install a release build

Release builds produce installers under `target/release/bundle/`:

| Target | Typical file |
| --- | --- |
| MSI | `target/release/bundle/msi/Archon_0.1.0_x64_en-US.msi` |
| NSIS setup | `target/release/bundle/nsis/Archon_0.1.0_x64-setup.exe` |

Run the installer and launch **Archon** from the Start menu. Uninstall through
Windows Settings → Apps.

A portable build is also available: copy `target/release/archon.exe` (the desktop
binary embeds the frontend) to any folder and run it. The portable build stores its
WebView profile in the standard per-user location.

## Build from source

Prerequisites:

1. **Rust 1.88+** with the `x86_64-pc-windows-msvc` toolchain (`rustup default stable`).
2. **Visual Studio Build Tools** with “Desktop development with C++” (MSVC linker and
   Windows SDK).
3. **Node.js 22+** and npm.
4. **WebView2 runtime** (usually already present).

```powershell
git clone <repository-url> archon
cd archon
npm install
npm run tauri build
```

The first build is slow: it compiles Tauri, WebView bindings and the archive engine. A
warm build is much faster.

### Faster local builds

```powershell
# Debug desktop build without installers
npm run tauri build -- --debug --no-bundle

# Development window with hot reload
npm run tauri dev
```

Debug output: `target/debug/archon.exe`. Release output: `target/release/archon.exe`.
The Tauri CLI places desktop artifacts in `target/` at the repository root because the
workspace shares one target directory.

## Command-line binary

Build only the CLI:

```powershell
cargo build --release -p archon-cli
```

The binary is `target/release/archon-cli.exe`. Add that folder to `PATH` or copy the
executable somewhere on `PATH`. The `-cli` suffix keeps it distinct from the desktop
executable `archon.exe`; building either does not overwrite the other.

```powershell
archon-cli --help
archon-cli list C:\Users\me\Downloads\example.zip
```

The CLI does not require WebView2 or Node.js.

## First run

- The **Welcome** view offers “Open archive”, drag-and-drop and recent archives.
- The **Formats** view shows the live capability matrix.
- Recent archives are kept in the local WebView storage. Clear them from the Welcome
  view or by clearing the app's site data.
- No account, licence key or network connection is needed.

## Removing data

Uninstalling removes the application. Recent-archive history lives in the per-user
WebView profile; delete the app's local data from Windows Settings or the Welcome view
to remove it. Archon never stores archive passwords.
