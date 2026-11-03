# Contributing to Archon

Thanks for helping. Archon is an independent archive manager; contributions must not
include code, assets, icons or branding copied from WinRAR or other proprietary tools.

## Before you start

- Open an issue for significant features or behavioural changes so the design can be
  agreed first.
- Security issues follow [SECURITY.md](SECURITY.md), not the public issue tracker.
- By contributing you agree that your work is licensed under the repository's MIT
  licence.

## Development workflow

```powershell
npm install
npm run check
npm run build
cargo test -p archon-core
cargo check --workspace --all-targets
```

Run `npm run tauri dev` for UI work and the manual CLI smoke test in
[docs/development.md](docs/development.md) for engine changes.

## What makes a good change

- **Honesty first.** Do not add UI for an operation the engine cannot perform. Update
  the capability registry only when a real, tested path exists.
- **Engine-first.** Behaviour belongs in `crates/archon-core`. Keep Tauri commands and
  React components thin.
- **Tests.** Every bug fix needs a regression test. Every new archive behaviour needs an
  integration test, including failure paths.
- **Security.** Treat archive input as hostile. Validate before writing; never follow
  symlinks; never log passwords; do not accept passwords as CLI arguments.
- **Consistency.** Match the existing Rust module layout, naming, error codes and the
  amber/graphite design system.
- **No new network access or telemetry.** Archon is a local-only tool.

## Commit and pull request guidelines

- Keep commits focused and use imperative subjects (for example,
  `Reject drive-relative paths in extraction`).
- Describe the user-visible effect, the tests run and any limitation that remains.
- Include screenshots for UI changes.
- Update `CHANGELOG.md` under “Unreleased” for user-visible changes.
- Do not bump the version in a feature PR.

## Reporting bugs

Include:

- Archon version and build source (installer or commit).
- Windows version.
- Steps to reproduce, expected and actual behaviour.
- A minimal archive if the bug depends on content (remove sensitive data).
- Relevant error codes shown by the app or CLI.

## Code of conduct

Be respectful and constructive. Harassment, discrimination and hostile behaviour are
not tolerated. Maintainers may close contributions that ignore these expectations.
