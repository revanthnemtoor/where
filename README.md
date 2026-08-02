# where

A modern, fast replacement for the `which` and `where` commands on Linux, written in Rust.

The `where` command helps you locate executables in your system's `PATH`. It provides features similar to the Windows `where` command, combined with Unix-specific enhancements like symlink resolution, permissions viewing, structural diagnostics, and structured JSON outputs.

## Features

- **Deduplication:** Automatically groups aliased or symlinked paths (like `/bin/` and `/usr/bin/`) to provide clean output.
- **Detailed information:** View symlink targets, ownership, inode numbers, file sizes, and package origins.
- **Diagnostics (`--doctor`):** Instantly audit your `$PATH` for missing directories, duplicates, dangerous relative paths, and shadowed binaries.
- **Provenance Tracing (`--trace`):** See exactly *how* and *why* a command resolved to a specific binary across all your `PATH` directories, complete with filesystem detection.
- **Machine-readable formats:** Output results as `JSON`, `YAML`, or `CSV` for easy scripting and automation.
- **Fast execution:** Written in Rust using `rayon` for parallel directory scanning. Tracks search completion time down to the millisecond.

## Installation

### From crates.io
```bash
cargo install where-cmd
```
*(The binary is installed as `where`)*

### From Source
To build and install the command from source, you must have [Rust and Cargo](https://rustup.rs/) installed.

1.  Clone this repository:
    ```bash
    git clone https://github.com/revanthnemtoor/where.git
    cd where
    ```

2.  Build the release binary:
    ```bash
    cargo build --release
    ```

3.  Move the compiled binary to a directory in your `PATH` (for example, `/usr/local/bin`):
    ```bash
    sudo mv target/release/where /usr/local/bin/
    ```

## Usage

Provide one or more command names as arguments to locate them. 

```bash
where <command> [command...]
```

### Basic Search
To find the location of a single executable, run:
```bash
$ where python
python
 └─ /usr/bin/python
```

### Verbose Information
To view detailed information about the executable, such as its symlink targets, permissions, owner, and inode, use the `-v` or `--verbose` flag:
```bash
$ where -v python
python
 └─ /usr/bin/python -> python3
    also in PATH as: /bin/python
    executable: true
    owner: root
    permissions: -rwxr-xr-x
    inode: 1191509
```

### Provenance Tracing
If you are debugging a broken environment, use `--trace` to see precisely how a binary was resolved:
```bash
$ where --trace cargo
PATH:
1. /usr/local/bin
2. /usr/bin                  ✓ selected
3. /bin                      ✓ alias

Executable:
/usr/bin/cargo

Filesystem:
btrfs

Scan
────
Directories : 12
Entries     : 13897
Workers     : 16
Elapsed     : 43.63 ms
```

### PATH Diagnostics
Audit your environment for errors:
```bash
$ where --doctor
PATH diagnostics

✓ PATH contains 14 directories

⚠ Duplicate:
/bin

⚠ Missing:
/fake/missing/dir

⚠ 6926 shadowed binaries detected
```

### Persistent Configuration
You can configure default flags by placing a `config.toml` file in `~/.config/where/config.toml`:
```toml
color = true
output = "json"
package = true
version_info = true
```

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success (all commands found, or `--doctor` found no issues) |
| `1` | At least one command missing, or `--doctor` found missing/unreadable directories |
| `2` | Invalid arguments (handled by `clap`) |

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
