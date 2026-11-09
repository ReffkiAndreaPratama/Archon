# Third-party notices

Archon is MIT-licensed. It depends on the open-source projects below. This list covers
the direct dependencies that shape the product; the authoritative, complete dependency
graph with exact versions is in `Cargo.lock` and `package-lock.json`.

## Rust crates

| Component | Version | License | Use |
| --- | --- | --- | --- |
| [`zip`](https://crates.io/crates/zip) | 9.2.x | MIT | ZIP reading, writing, Deflate and AES |
| [`sevenz-rust2`](https://crates.io/crates/sevenz-rust2) | 0.24.x | Apache-2.0 | 7Z reading, decompression and AES-256 |
| [`rars`](https://crates.io/crates/rars) | 0.12.x | Apache-2.0 | RAR/RAR5 reading, decompression and AES |
| [`tar`](https://crates.io/crates/tar) | 0.4.x | MIT OR Apache-2.0 | TAR listing and extraction |
| [`flate2`](https://crates.io/crates/flate2) | 1.x | MIT OR Apache-2.0 | GZIP and Deflate decompression |
| [`bzip2`](https://crates.io/crates/bzip2) | 0.6.x | MIT OR Apache-2.0 | BZIP2 decompression |
| [`xz2`](https://crates.io/crates/xz2) | 0.1.x | MIT OR Apache-2.0 | XZ/LZMA decompression |
| [`zstd`](https://crates.io/crates/zstd) | 0.14.x | BSD-3-Clause | Zstandard decompression |
| [`tauri`](https://crates.io/crates/tauri) | 2.x | MIT OR Apache-2.0 | Desktop shell and IPC |
| `tauri-build`, `tauri-codegen`, `tauri-macros`, `tauri-runtime*` | 2.x | MIT OR Apache-2.0 | Build and runtime support |
| `tauri-plugin-dialog` | 2.x | MIT OR Apache-2.0 | Native file and folder pickers |
| `tauri-plugin-opener` | 2.x | MIT OR Apache-2.0 | Reveal extracted files in Explorer |
| [`serde`](https://crates.io/crates/serde), `serde_json` | 1.x | MIT OR Apache-2.0 | Serialisation and JSON |
| [`thiserror`](https://crates.io/crates/thiserror) | 2.x | MIT OR Apache-2.0 | Typed errors |
| [`clap`](https://crates.io/crates/clap) | 4.x | MIT OR Apache-2.0 | CLI parsing |
| [`walkdir`](https://crates.io/crates/walkdir) | 2.x | Unlicense OR MIT | Source tree traversal |
| [`globset`](https://crates.io/crates/globset) | 0.4.x | Unlicense OR MIT | Exclude patterns |
| [`fs2`](https://crates.io/crates/fs2) | 0.4.x | MIT OR Apache-2.0 | Free-space query and file locking primitives |
| [`filetime`](https://crates.io/crates/filetime) | 0.2.x | MIT OR Apache-2.0 | Restoring modification times |
| [`time`](https://crates.io/crates/time) | 0.3.x | MIT OR Apache-2.0 | Timestamp conversion |
| [`zeroize`](https://crates.io/crates/zeroize) | 1.x | Apache-2.0 OR MIT | Wiping password memory |
| [`rpassword`](https://crates.io/crates/rpassword) | 7.x | Apache-2.0 OR MIT | Hidden terminal password prompt |
| [`tempfile`](https://crates.io/crates/tempfile) | 3.x | MIT OR Apache-2.0 | Test temporary directories (dev only) |

The `zip` crate's Deflate backend uses `flate2` and [`zlib-rs`](https://crates.io/crates/zlib-rs)
(licence: Zlib). AES support uses the RustCrypto [`aes`](https://crates.io/crates/aes) and
related crates (MIT OR Apache-2.0).

## JavaScript packages

| Component | Version | License | Use |
| --- | --- | --- | --- |
| [`react`](https://www.npmjs.com/package/react), `react-dom` | 19.x | MIT | UI runtime |
| [`@tauri-apps/api`](https://www.npmjs.com/package/@tauri-apps/api) | 2.x | MIT OR Apache-2.0 | IPC and events |
| `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-opener` | 2.x | MIT OR Apache-2.0 | Native dialogs and reveal |
| [`lucide-react`](https://www.npmjs.com/package/lucide-react) | 0.468.x | ISC | Interface icons |
| [`vite`](https://www.npmjs.com/package/vite) | 8.x | MIT | Frontend build |
| [`@vitejs/plugin-react`](https://www.npmjs.com/package/@vitejs/plugin-react) | 6.x | MIT | React transform |
| [`tailwindcss`](https://www.npmjs.com/package/tailwindcss), `@tailwindcss/vite` | 4.x | MIT | Styling engine and build plugin |
| [`typescript`](https://www.npmjs.com/package/typescript) | 6.x | Apache-2.0 | Type checking |
| [`@tauri-apps/cli`](https://www.npmjs.com/package/@tauri-apps/cli) | 2.x | MIT OR Apache-2.0 | Build tooling |

## Assets

All Archon artwork, including the application icon and brand mark, was created for this
project and is MIT-licensed with the rest of the repository. No icons, images, sounds,
translations or other assets from WinRAR or other proprietary archive tools are used.

## Notes

- License texts for each dependency are distributed inside their source packages and are
  available from the links above.
- If you redistribute a binary build, include this file together with `LICENSE`.
- Archon is not affiliated with, endorsed by or sponsored by RARLAB, WinRAR, 7-Zip,
  Microsoft or any other third party mentioned here.
