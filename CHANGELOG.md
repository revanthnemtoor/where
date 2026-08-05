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

## [1.3.0] - 2026-08-03

### Added
- **Versions Flag (`--versions`)**: Discover other installed versions of a binary via sibling discovery.

## [1.2.12] - 2026-08-03

*(Includes features and fixes from v1.2.0 through v1.2.12)*

### Added
- **Search Flexibility**: Added `--match` (regex/wildcard matching), `--chain` (full symlink chains), and `--exclude` flags.
- **Shell Integration Hooks**: Automatically install shell hooks into RC files for Bash, Zsh, and Fish.
- **Shell Context**: Native shell integration for resolving aliases, functions, built-ins, abbreviations, and environment variables.
- **Package Manager Suggestions (`--suggest`)**: Multi-ecosystem package suggestions (supports `pacman`, `apt`, `dnf`, `brew`, `cargo`, `npm`).
- **Interactive TUI (`--interactive` / `-i`)**: Blazing-fast fuzzy finder powered by `skim`.
- **Custom Environments**: Added `--env-path` and `--deep` flags for flexible, recursive environment scanning.
- **ELF Diagnostics**: Implemented native ELF parsing to provide `--arch`, `--libs`, and `--security` diagnostics.
- **AUR Packages**: Added official `where` and `where-cmd` AUR PKGBUILDs.

### Fixed
- Fixed multiple bugs with correctly escaping newlines in Fish and Bash integration scripts.
- Fixed multi-line environment variable formatting and structured JSON for shell contexts.
- Added explicit non-ELF warning and filtered the TUI to only show executables.

## [1.1.1] - 2026-08-02

### Added
- **Quiet Mode (`-q` / `--quiet`)**: Suppress output and only return exit codes (useful for scripting).
- Added `crates.io` metadata for initial Cargo publishing.

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
