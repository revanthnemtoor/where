# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.3.1] - 2026-08-06

### Fixed
- **AUR `where-git` Package**: Restored missing `!lto` option to match stable packages.
- **AUR `where-git` Package**: Fixed man page path pointing to the old root location instead of `docs/where.1`.
- **AUR `where-git` Package**: Restored installation of shell integration scripts (`where.bash`, `where.fish`, `where.zsh`) which were missing in the git package.
- **Gitignore**: Added `aur/*/where/` to ignore the bare git repositories created by `makepkg` during AUR git builds.

## [1.0.0] - 2026-08-02

### Added
- **Parallel Scanning**: Replaced sequential `PATH` parsing with `rayon`'s parallel iterators for near-instantaneous startup.
- **Persistent Configuration**: Reads default flags from `~/.config/where/config.toml`.
- **Command Provenance (`--trace`)**: Trace resolution flow and explicitly view why directories were selected or shadowed.
- **Structured Trace JSON**: Combine `--json` and `--trace` to get structured diagnostics with embedded trace loops and timings.
- **Pretty JSON (`--pretty`)**: Expand minified JSON for readability.
- **PATH Diagnostics (`--doctor`)**: Audit environment for duplicate entries, missing paths, dangerous relative directories, and shadowed binaries.
- **Filesystem Detection**: Native `statfs` parsing to identify the filesystem backing an executable.
- **Automated Tests**: Integration tests utilizing `tempfile` to mock directory structures and symlinks.
- **GitHub Actions**: Fully configured CI/CD for test coverage and cross-compiling release binaries.
- **Man Page**: Official `where(1)` manual installed with the package.

### Changed
- Replaced the basic `--why` flag output with the vastly superior `--trace` mode.
- Improved the `--about` screen with colored components.
- JSON output now defaults to minified for script efficiency unless `--pretty` is provided.

### Fixed
- Fixed deduplication logic strictly asserting that symlinked binaries resolving to the same underlying inode are correctly squashed into the `aliases` array instead of duplicated as primary matches.
