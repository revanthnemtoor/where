# where

A modern, fast replacement for the `which` and `where` commands on Linux, written in Rust.

The `where` command helps you locate executables in your system's `PATH`. It provides features similar to the Windows `where` command, combined with Unix-specific enhancements like symlink resolution, permissions viewing, and colorized output.

## Features

- **Deduplication:** Automatically groups aliased or symlinked paths (like `/bin/` and `/usr/bin/`) to provide clean output.
- **Detailed information:** View symlink targets, ownership, inode numbers, and file sizes.
- **Verification:** Instantly generate SHA-256 hashes of target executables.
- **Machine-readable formats:** Output results as JSON for easy scripting and automation.
- **Fast execution:** Written in Rust for minimal overhead. Tracks search completion time.
- **Visual cues:** Colors highlight valid executables (green) and files lacking execution permissions (red).

## Installation

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

### Basic search

To find the location of a single executable, run:

```bash
where python
```

Output:
```
python
 └─ /usr/bin/python
```

### Multiple searches

To search for multiple executables at once, separate them with spaces:

```bash
where git python cargo
```

### View verbose information

To view detailed information about the executable, such as its symlink targets, permissions, owner, and inode, use the `-v` or `--verbose` flag:

```bash
where -v python
```

Output:
```
python
 └─ /usr/bin/python -> python3
    also in PATH as: /bin/python
    executable: true
    owner: root
    permissions: -rwxr-xr-x
    inode: 1191509
```

### View SHA-256 hash

To compute and display the SHA-256 hash of the executable, use the `--hash` flag:

```bash
where --hash python
```

Output:
```
python
 └─ /usr/bin/python
    sha256: 25aa96d579c8dd8194dac632dc79511f055381da7f40ef1c45167c23d6867e95
```

### Output JSON

To return the results in JSON format for use in other scripts, use the `--json` flag:

```bash
where --json python
```

### Additional flags

- `-a, --all`: Print all matches (default behavior).
- `-1, --first-only`: Stop searching after finding the first match.
- `-l, --show-symlink`: Show symlink targets.
- `-s, --show-size`: Show the file size in bytes.
- `--time`: Print the duration it took to complete the search.
- `--color`: Force colored output.
- `--about`: Print information about the tool's version, author, and license.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
