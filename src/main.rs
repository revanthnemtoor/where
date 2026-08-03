use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use colored::{control, Colorize};
use directories::ProjectDirs;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

mod container;

#[derive(Deserialize, Default)]
struct Config {
    color: Option<bool>,
    output: Option<String>,
    package: Option<bool>,
    version_info: Option<bool>,
    all: Option<bool>,
}

#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), version = env!("CARGO_PKG_VERSION"), about = env!("CARGO_PKG_DESCRIPTION"), author = env!("CARGO_PKG_AUTHORS"))]
struct Cli {
    /// Print all matches (default behavior)
    #[arg(short = 'a', long)]
    all: bool,

    /// Stop after the first match
    #[arg(short = '1', long)]
    first_only: bool,

    /// Show symlink targets
    #[arg(short = 'l', long)]
    show_symlink: bool,

    /// Show file size
    #[arg(short = 's', long)]
    show_size: bool,

    /// Verbose output (permissions, owner, inode)
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Machine-readable output (JSON)
    #[arg(long)]
    json: bool,

    /// Pretty print JSON
    #[arg(long)]
    pretty: bool,

    /// Machine-readable output (YAML)
    #[arg(long)]
    yaml: bool,

    /// Machine-readable output (CSV)
    #[arg(long)]
    csv: bool,

    /// Plain output (paths only)
    #[arg(long)]
    plain: bool,

    /// Colored output
    #[arg(long)]
    color: bool,

    /// Compute SHA-256 hash
    #[arg(long)]
    hash: bool,

    /// Show execution time
    #[arg(long)]
    time: bool,

    /// Show Arch package owner
    #[arg(long)]
    package: bool,

    /// Show version information
    #[arg(long)]
    version_info: bool,

    /// Show why a command was found
    #[arg(long)]
    why: bool,

    /// Show command provenance and resolution flow
    #[arg(long)]
    trace: bool,

    /// Show benchmark stats
    #[arg(long)]
    benchmark: bool,

    /// Diagnose PATH issues
    #[arg(long)]
    doctor: bool,

    /// Generate shell completions
    #[arg(long, value_enum)]
    generate_completions: Option<Shell>,

    /// Print about information
    #[arg(long)]
    about: bool,

    /// Quiet mode: suppress all output, just return exit codes
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Verify binary architecture using ELF headers
    #[arg(long)]
    arch: bool,

    /// Check setuid/setgid and linkage
    #[arg(long)]
    security: bool,

    /// List dynamic library dependencies
    #[arg(long)]
    libs: bool,

    /// Simulate a custom PATH environment variable
    #[arg(long)]
    env_path: Option<String>,

    /// Recursively search subdirectories up to a depth limit
    #[arg(long)]
    deep: Option<usize>,

    /// Launch interactive TUI to fuzzy find an executable
    #[arg(short, long)]
    interactive: bool,

    /// Query package managers if command is not found
    #[arg(long)]
    suggest: bool,

    /// Show the resolution hierarchy (shell context vs filesystem)
    #[arg(long)]
    resolve: bool,

    /// Explain the selection logic for the resolved command
    #[arg(long)]
    explain: bool,

    /// Generate shell integration script (bash, zsh, fish)
    #[arg(long)]
    init: Option<String>,

    /// Search inside a Docker or Podman container image
    #[arg(long)]
    container: Option<String>,

    /// Container engine to use (docker, podman) - defaults to auto-detect
    #[arg(long)]
    engine: Option<String>,

    /// Commands to search for
    #[arg(required_unless_present_any = ["about", "generate_completions", "doctor", "interactive", "init"])]
    commands: Vec<String>,
}

#[derive(Serialize)]
struct Match {
    path: PathBuf,
    canonical: Option<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    aliases: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symlink_target: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filesystem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    security: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    libs: Option<Vec<String>>,
    executable: bool,
}

#[derive(Debug, Default, Serialize, Clone)]
struct ShellContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    shell_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_function: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_builtin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    abbreviation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_var: Option<String>,
}

impl ShellContext {
    fn parse(cmd: &str) -> Self {
        let env_var = std::env::var(cmd).ok();
        
        let shell_name = std::env::var("WHERE_SHELL").ok();
        
        let mut alias = None;
        if let Ok(aliases_env) = std::env::var("WHERE_ALIASES") {
            let aliases_env = aliases_env.replace("\\n", "\n");
            // Bash: alias cmd='...'
            // Zsh: cmd='...'
            // Fish: alias cmd '...'
            for line in aliases_env.lines() {
                if let Some(def) = line.strip_prefix(&format!("alias {}=", cmd)) {
                    alias = Some(def.trim_matches('\'').trim_matches('"').to_string());
                    break;
                }
                if let Some(def) = line.strip_prefix(&format!("alias {} ", cmd)) {
                    alias = Some(def.trim_matches('\'').trim_matches('"').to_string());
                    break;
                }
                if let Some(def) = line.strip_prefix(&format!("{}=", cmd)) {
                    alias = Some(def.trim_matches('\'').trim_matches('"').to_string());
                    break;
                }
            }
        }

        let mut is_function = false;
        if let Ok(funcs_env) = std::env::var("WHERE_FUNCTIONS") {
            let funcs_env = funcs_env.replace("\\n", "\n");
            for func in funcs_env.split_whitespace() {
                if func == cmd {
                    is_function = true;
                    break;
                }
            }
        }

        let mut is_builtin = false;
        if let Ok(builtins_env) = std::env::var("WHERE_BUILTINS") {
            let builtins_env = builtins_env.replace("\\n", "\n");
            for builtin in builtins_env.split_whitespace() {
                if builtin == cmd {
                    is_builtin = true;
                    break;
                }
            }
        }

        let mut abbreviation = None;
        if let Ok(abbrs_env) = std::env::var("WHERE_ABBRS") {
            let abbrs_env = abbrs_env.replace("\\n", "\n");
            // abbr --add cmd '...'
            for line in abbrs_env.lines() {
                if line.contains(&format!(" {} ", cmd)) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 && parts[2] == cmd {
                        abbreviation = Some(parts[3..].join(" ").trim_matches('\'').trim_matches('"').to_string());
                        break;
                    }
                }
            }
        }

        Self {
            shell_name,
            alias,
            is_function,
            is_builtin,
            abbreviation,
            env_var,
        }
    }

    fn is_found(&self) -> bool {
        self.alias.is_some() || self.is_function || self.is_builtin || self.abbreviation.is_some() || self.env_var.is_some()
    }
}

#[derive(Serialize)]
struct TracePathInfo {
    index: usize,
    directory: PathBuf,
    matched: bool,
}

#[derive(Serialize)]
struct TraceTiming {
    directories: usize,
    entries: usize,
    workers: usize,
    elapsed_ms: f64,
}

#[derive(Serialize)]
struct TraceBlock {
    path: Vec<TracePathInfo>,
    timing: TraceTiming,
}

#[derive(Serialize)]
struct CommandResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    shell_context: Option<ShellContext>,
    matches: Vec<Match>,
}

#[derive(Serialize)]
struct RootJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    trace: Option<TraceBlock>,
    results: HashMap<String, CommandResult>,
}

fn is_executable(path: &Path) -> bool {
    let mut c_path = path.as_os_str().as_bytes().to_vec();
    c_path.push(0);
    unsafe { libc::access(c_path.as_ptr() as *const libc::c_char, libc::X_OK) == 0 }
}

fn get_user_name(uid: u32) -> String {
    unsafe {
        let passwd = libc::getpwuid(uid as libc::uid_t);
        if !passwd.is_null() {
            let c_str = std::ffi::CStr::from_ptr((*passwd).pw_name);
            if let Ok(s) = c_str.to_str() {
                return s.to_string();
            }
        }
    }
    uid.to_string()
}

fn format_mode(mode: u32) -> String {
    let rwx = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'), (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    let mut perm_str = String::new();
    perm_str.push(if mode & 0o170000 == 0o120000 { 'l' } else { '-' });
    for (mask, c) in rwx {
        perm_str.push(if mode & mask != 0 { c } else { '-' });
    }
    perm_str
}

fn get_package(path: &Path) -> Option<String> {
    if let Ok(output) = Command::new("pacman").arg("-Qo").arg(path).output() {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            if let Some(pkg) = out_str.split("is owned by ").nth(1) {
                return Some(pkg.trim().to_string());
            }
        }
    }
    None
}

fn get_version(path: &Path) -> Option<String> {
    if let Ok(mut child) = Command::new(path).arg("--version").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn() {
        let timeout = Duration::from_millis(500);
        match child.wait_timeout(timeout).unwrap() {
            Some(status) => {
                if status.success() {
                    let mut out = String::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        use std::io::Read;
                        let _ = stdout.read_to_string(&mut out);
                    }
                    if out.trim().is_empty() {
                        if let Some(mut stderr) = child.stderr.take() {
                            use std::io::Read;
                            let _ = stderr.read_to_string(&mut out);
                        }
                    }
                    if !out.trim().is_empty() {
                        return Some(out.lines().next().unwrap_or("").trim().to_string());
                    }
                }
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    None
}

fn get_filesystem(path: &Path) -> Option<String> {
    unsafe {
        let mut stat = std::mem::zeroed();
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        if libc::statfs(c_path.as_ptr(), &mut stat) == 0 {
            let magic = stat.f_type as u64;
            let fs_name = match magic {
                0xEF53 => "ext4/ext3/ext2",
                0x9123683E => "btrfs",
                0x01021994 => "tmpfs",
                0x58465342 => "xfs",
                0x2FC12FC1 => "zfs",
                0x6969 => "nfs",
                0x4d44 => "vfat",
                0x52654973 => "reiserfs",
                0x00000187 => "autofs",
                0x00000027 => "minix",
                0x4244 => "hfs",
                0x65735546 => "fuse",
                _ => return Some(format!("Unknown (0x{:X})", magic)),
            };
            return Some(fs_name.to_string());
        }
    }
    None
}

fn main() {
    use std::io::IsTerminal;
    if std::env::var("WHERE_SHELL").is_err() && std::io::stdout().is_terminal() {
        ensure_shell_hooks();
    }

    let mut cli = Cli::parse();

    if let Some(proj_dirs) = ProjectDirs::from("", "", "where") {
        let config_file = proj_dirs.config_dir().join("config.toml");
        if let Ok(contents) = fs::read_to_string(config_file) {
            if let Ok(config) = toml::from_str::<Config>(&contents) {
                if config.color.unwrap_or(false) { cli.color = true; }
                if config.package.unwrap_or(false) { cli.package = true; }
                if config.version_info.unwrap_or(false) { cli.version_info = true; }
                if config.all.unwrap_or(false) { cli.all = true; }
                if let Some(out) = config.output {
                    match out.as_str() {
                        "json" => cli.json = true,
                        "yaml" => cli.yaml = true,
                        "csv" => cli.csv = true,
                        "plain" => cli.plain = true,
                        _ => {}
                    }
                }
            }
        }
    }

    if cli.color {
        control::set_override(true);
    } else if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        control::set_override(false);
    }

    if cli.about {
        println!("{} {}\n{}", "where".bold().green(), env!("CARGO_PKG_VERSION").cyan(), env!("CARGO_PKG_DESCRIPTION").italic());
        println!();
        println!("{:<10} : {}", "Author".bold(), env!("CARGO_PKG_AUTHORS").yellow());
        println!("{:<10} : {}", "License".bold(), env!("CARGO_PKG_LICENSE").yellow());
        println!("{:<10} : {}", "Repository".bold(), env!("CARGO_PKG_REPOSITORY").blue());
        std::process::exit(0);
    }

    if let Some(generator) = cli.generate_completions {
        let mut cmd = Cli::command();
        generate(generator, &mut cmd, "where", &mut io::stdout());
        std::process::exit(0);
    }

    let container_engine = if let Some(image) = &cli.container {
        match container::ContainerEngine::new(cli.engine.clone(), image.clone()) {
            Ok(engine) => Some(engine),
            Err(e) => {
                eprintln!("{}", e.red());
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let start_time = Instant::now();
    let path_var = if let Some(engine) = &container_engine {
        match engine.get_path_env() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", e.red());
                std::process::exit(1);
            }
        }
    } else if let Some(custom_path) = &cli.env_path {
        custom_path.clone()
    } else {
        env::var("PATH").unwrap_or_default()
    };
    let paths: Vec<PathBuf> = env::split_paths(&path_var).collect();
    
    let max_depth = cli.deep.unwrap_or(1);
    
    let mut shell_contexts: HashMap<String, ShellContext> = HashMap::new();

    // Cache PATH contents in parallel
    let (path_cache, dirs_searched, files_examined) = if container_engine.is_some() {
        (HashMap::new(), paths.len(), 0)
    } else {
        paths
            .par_iter()
            .map(|dir| {
            let mut local_cache: HashMap<String, Vec<PathBuf>> = HashMap::new();
            let mut local_files = 0;
            let mut local_dirs = 0;
            if dir.exists() {
                local_dirs = 1; // Count the base directory itself
                let walker = walkdir::WalkDir::new(dir)
                    .min_depth(1)
                    .max_depth(max_depth)
                    .into_iter()
                    .filter_map(Result::ok);
                
                for entry in walker {
                    if entry.file_type().is_dir() {
                        local_dirs += 1;
                    } else if entry.file_type().is_file() || entry.file_type().is_symlink() {
                        local_files += 1;
                        if let Some(name) = entry.file_name().to_str() {
                            local_cache.entry(name.to_string()).or_default().push(entry.path().to_path_buf());
                        }
                    }
                }
            }
            (local_cache, local_dirs, local_files)
        })
        .reduce(
            || (HashMap::new(), 0, 0),
            |(mut cache1, d1, f1), (cache2, d2, f2)| {
                for (k, v) in cache2 {
                    cache1.entry(k).or_default().extend(v);
                }
                (cache1, d1 + d2, f1 + f2)
            },
        )
    };

    let elapsed = start_time.elapsed();

    if cli.doctor {
        println!("PATH diagnostics\n");
        let mut seen_dirs = HashSet::new();
        let mut unreadable = 0;
        let mut missing = 0;
        
        let mut relatives = 0;
        
        println!("{} PATH contains {} directories\n", "✓".green(), paths.len());
        
        for dir in &paths {
            let mut ok = true;
            if !dir.is_absolute() {
                println!("{} Relative path:\n{}", "⚠".yellow(), dir.display());
                relatives += 1;
                ok = false;
            } else if !dir.exists() {
                println!("{} Missing:\n{}", "⚠".yellow(), dir.display());
                missing += 1;
                ok = false;
            } else {
                let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
                if !seen_dirs.insert(canonical.clone()) {
                    println!("{} Duplicate:\n{}", "⚠".yellow(), dir.display());
                    
                    ok = false;
                }
                
                if fs::read_dir(dir).is_err() {
                    println!("{} Unreadable:\n{}", "⚠".yellow(), dir.display());
                    unreadable += 1;
                    ok = false;
                }
            }
            if !ok { println!(); }
        }
        
        if unreadable == 0 { println!("{} All directories readable\n", "✓".green()); }
        
        let mut shadowed_binaries = 0;
        let mut first_seen: HashMap<String, PathBuf> = HashMap::new();
        for dir in &paths {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        if let Some(existing) = first_seen.get(&name) {
                            if existing != &entry.path() {
                                // it is technically shadowed, but only log it if we were doing a full dump.
                                // Actually, printing every shadowed binary is too verbose.
                                shadowed_binaries += 1;
                            }
                        } else {
                            first_seen.insert(name, entry.path());
                        }
                    }
                }
            }
        }
        if shadowed_binaries == 0 {
            println!("{} No shadowed binaries detected", "✓".green());
        } else {
            println!("{} {} shadowed binaries detected", "⚠".yellow(), shadowed_binaries);
        }
        
        let exit_code = if missing > 0 || unreadable > 0 || relatives > 0 { 1 } else { 0 };
        std::process::exit(exit_code);
    }

    let mut commands = cli.commands.clone();

    if cli.interactive {
        use skim::prelude::*;
        use std::io::Cursor;
        
        let mut keys: Vec<String> = path_cache.keys()
            .filter(|k| {
                if let Some(paths) = path_cache.get(*k) {
                    paths.iter().any(|p| is_executable(p))
                } else {
                    false
                }
            })
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        
        let items_str = keys.join("\n");
        let item_reader = SkimItemReader::default();
        let items = item_reader.of_bufread(Cursor::new(items_str));
        
        let mut options_builder = SkimOptionsBuilder::default();
        if !commands.is_empty() {
            options_builder.query(commands[0].clone());
        }
        let options = options_builder.build().unwrap();
        
        if let Ok(out) = Skim::run_with(options, Some(items)) {
            if !out.is_abort {
                commands.clear();
                for item in out.selected_items {
                    commands.push(item.output().to_string());
                }
            } else {
                std::process::exit(0);
            }
        } else {
            std::process::exit(1);
        }
    }

    let mut duplicates_removed = 0;
    let mut results: HashMap<String, Vec<Match>> = HashMap::new();
    let mut missing_any = false;

    if let Some(shell) = &cli.init {
        match shell.as_str() {
            "bash" => {
                println!(r#"where() {{
    export WHERE_SHELL="bash"
    export WHERE_ALIASES="$(alias)"
    export WHERE_FUNCTIONS="$(declare -F | awk '{{print $3}}')"
    export WHERE_BUILTINS="$(compgen -b)"
    command where "$@"
}}"#);
            }
            "zsh" => {
                println!(r#"where() {{
    export WHERE_SHELL="zsh"
    export WHERE_ALIASES="$(alias)"
    export WHERE_FUNCTIONS="$(print -l ${{(k)functions}})"
    export WHERE_BUILTINS="$(print -l ${{(k)builtins}})"
    command where "$@"
}}"#);
            }
            "fish" => {
                println!(r#"function where
    set -lx WHERE_SHELL "fish"
    set -lx WHERE_ALIASES (alias | string join \\n)
    set -lx WHERE_FUNCTIONS (functions -n | string join \\n)
    set -lx WHERE_BUILTINS (builtin -n | string join \\n)
    set -lx WHERE_ABBRS (abbr --show | string join \\n)
    command where $argv
end"#);
            }
            _ => {
                eprintln!("Unsupported shell: {}. Supported shells: bash, zsh, fish", shell);
                std::process::exit(1);
            }
        }
        return;
    }

    let is_structured = cli.json || cli.yaml || cli.csv;
    let fetch_all = is_structured || cli.trace;

    for cmd in &commands {
        let mut matches_for_cmd: Vec<Match> = Vec::new();
        let mut seen_inodes: HashMap<(u64, u64), usize> = HashMap::new();
        
        if cli.why {
            println!("{} Found because:", cmd.bold());
            println!("PATH contains:");
            for p in &paths {
                println!("{}", p.display());
            }
            println!("\nMatched:");
        }

        let mut trace_paths_info = Vec::new();

        if cli.trace || cli.json {
            for (i, p) in paths.iter().enumerate() {
                let mut matched = false;
                if let Some(cmd_paths) = path_cache.get(cmd) {
                    for cp in cmd_paths {
                        if cp.parent() == Some(p) {
                            matched = true;
                            break;
                        }
                    }
                }
                trace_paths_info.push(TracePathInfo {
                    index: i + 1,
                    directory: p.clone(),
                    matched,
                });
            }
        }
        
        if cli.trace && !cli.json {
            println!("PATH:");
            for tpi in &trace_paths_info {
                if tpi.matched {
                    
                    // Since trace paths are printed before matches are processed, we just print matched.
                    // But user wants "selected" vs "alias". We can determine this by processing matches first!
                }
            }
        }

        if let Some(engine) = &container_engine {
            let found_paths = engine.find_command_paths(&path_var, cmd);
            for p in found_paths {
                let mut m = Match {
                    path: PathBuf::from(&p),
                    canonical: None,
                    aliases: Vec::new(),
                    symlink_target: None,
                    size: None,
                    inode: None,
                    owner: None,
                    permissions: None,
                    hash: None,
                    package: None,
                    version: None,
                    filesystem: None,
                    arch: None,
                    security: None,
                    libs: None,
                    executable: true,
                };
                
                // Deep inspection
                if cli.hash || cli.show_size || cli.arch || cli.security || cli.libs || cli.verbose {
                    if let Ok(bytes) = engine.extract_file(&p) {
                        if cli.show_size || cli.verbose { m.size = Some(bytes.len() as u64); }
                        if cli.hash {
                            use sha2::{Sha256, Digest};
                            let mut hasher = Sha256::new();
                            hasher.update(&bytes);
                            m.hash = Some(hex::encode(hasher.finalize()));
                        }
                        
                        // Parse ELF from memory!
                        if let Ok(elf) = goblin::Object::parse(&bytes) {
                            if let goblin::Object::Elf(elf) = elf {
                                if cli.arch || cli.verbose {
                                    m.arch = Some(match elf.header.e_machine {
                                        goblin::elf::header::EM_X86_64 => "x86_64".to_string(),
                                        goblin::elf::header::EM_AARCH64 => "aarch64".to_string(),
                                        goblin::elf::header::EM_386 => "x86".to_string(),
                                        goblin::elf::header::EM_ARM => "arm".to_string(),
                                        _ => format!("unknown({})", elf.header.e_machine),
                                    });
                                }
                                if cli.security || cli.verbose {
                                    let mut sec = Vec::new();
                                    let is_pie = elf.header.e_type == goblin::elf::header::ET_DYN;
                                    if is_pie { sec.push("PIE"); }
                                    m.security = Some(sec.join(", "));
                                }
                                if cli.libs || cli.verbose {
                                    m.libs = Some(elf.libraries.iter().map(|s| s.to_string()).collect());
                                }
                            }
                        } else {
                            if cli.arch { m.arch = Some("Not an ELF binary".to_string()); }
                            if cli.security { m.security = Some("Not an ELF binary".to_string()); }
                            if cli.libs { m.libs = Some(vec!["(Not an ELF binary)".to_string()]); }
                        }
                    } else {
                        if cli.arch || cli.security || cli.libs || cli.hash || cli.show_size {
                            m.arch = Some("Extraction failed".to_string());
                        }
                    }
                }
                
                matches_for_cmd.push(m);
                if cli.first_only { break; }
            }
        } else if let Some(cmd_paths) = path_cache.get(cmd) {
            for full_path in cmd_paths {
                let executable = is_executable(full_path);
                
                let sym_meta = fs::symlink_metadata(full_path).ok();
                let target_meta = fs::metadata(full_path).ok();
                
                if let Some(meta) = &target_meta {
                    let dev = meta.dev();
                    let ino = meta.ino();
                    
                    if let Some(&index) = seen_inodes.get(&(dev, ino)) {
                        matches_for_cmd[index].aliases.push(full_path.clone().to_path_buf());
                        duplicates_removed += 1;
                        continue;
                    }
                    
                    seen_inodes.insert((dev, ino), matches_for_cmd.len());
                }

                let mut m = Match {
                    path: full_path.clone().to_path_buf().to_path_buf(),
                    canonical: None,
                    aliases: Vec::new(),
                    symlink_target: None,
                    size: None,
                    inode: None,
                    owner: None,
                    permissions: None,
                    hash: None,
                    package: None,
                    version: None,
                    filesystem: None,
                    arch: None,
                    security: None,
                    libs: None,
                    executable,
                };
                
                if fetch_all {
                    m.canonical = fs::canonicalize(full_path).ok();
                }

                if let Some(sym_meta) = &sym_meta {
                    if cli.show_symlink || cli.verbose || fetch_all {
                        if sym_meta.file_type().is_symlink() {
                            m.symlink_target = fs::read_link(full_path).ok();
                        }
                    }
                }
                
                if let Some(meta) = &target_meta {
                    if cli.show_size || cli.verbose || fetch_all {
                        m.size = Some(meta.size());
                    }
                    if cli.verbose || fetch_all {
                        m.inode = Some(meta.ino());
                        m.owner = Some(get_user_name(meta.uid()));
                        m.permissions = Some(format_mode(meta.mode()));
                    }
                }
                
                if cli.hash || fetch_all {
                    if let Ok(bytes) = fs::read(full_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        m.hash = Some(hex::encode(hasher.finalize()));
                    }
                }

                if cli.package || fetch_all {
                    m.package = get_package(full_path);
                }

                if cli.version_info || fetch_all {
                    m.version = get_version(full_path);
                }
                
                if cli.trace || fetch_all {
                    m.filesystem = get_filesystem(full_path);
                }

                if cli.arch || cli.security || cli.libs || fetch_all {
                    if let Ok(bytes) = fs::read(full_path) {
                        if let Ok(goblin::Object::Elf(elf)) = goblin::Object::parse(&bytes) {
                            if cli.arch || fetch_all {
                                m.arch = Some(goblin::elf::header::machine_to_str(elf.header.e_machine).to_string());
                            }
                            if cli.libs || fetch_all {
                                m.libs = Some(elf.libraries.iter().map(|&s| s.to_string()).collect());
                            }
                            if cli.security || fetch_all {
                                let is_dynamic = !elf.libraries.is_empty() || elf.interpreter.is_some();
                                let linkage = if is_dynamic { "dynamic" } else { "static" };
                                let mut sec = format!("linked: {}", linkage);
                                if let Some(meta) = &target_meta {
                                    let mode = meta.mode();
                                    if mode & 0o4000 != 0 {
                                        sec.push_str(", setuid");
                                    }
                                    if mode & 0o2000 != 0 {
                                        sec.push_str(", setgid");
                                    }
                                }
                                m.security = Some(sec);
                            }
                        } else {
                            if cli.arch {
                                m.arch = Some("Not an ELF binary".to_string());
                            }
                            if cli.security {
                                m.security = Some("Not an ELF binary".to_string());
                            }
                            if cli.libs {
                                m.libs = Some(vec!["(Not an ELF binary)".to_string()]);
                            }
                        }
                    } else {
                        if cli.arch || cli.security || cli.libs {
                            m.arch = Some("Unreadable".to_string());
                        }
                    }
                }

                matches_for_cmd.push(m);

                if cli.first_only {
                    break;
                }
            }
        }

        if cli.trace && !cli.json {
            // Re-evaluating trace path printing to include selected vs alias
            println!("PATH:");
            for tpi in &trace_paths_info {
                if tpi.matched {
                    let mut is_alias = false;
                    for m in &matches_for_cmd {
                        if m.aliases.iter().any(|a| a.parent() == Some(&tpi.directory)) {
                            is_alias = true;
                            break;
                        }
                    }
                    if is_alias {
                        println!("{}. {:<25} {} alias", tpi.index, tpi.directory.display(), "✓".green());
                    } else {
                        println!("{}. {:<25} {} selected", tpi.index, tpi.directory.display(), "✓".green());
                    }
                } else {
                    println!("{}. {}", tpi.index, tpi.directory.display());
                }
            }
            println!();
            println!("Scan");
            println!("────");
            println!("{:<11} : {}", "Directories", dirs_searched);
            println!("{:<11} : {}", "Entries", files_examined);
            println!("{:<11} : {}", "Workers", rayon::current_num_threads());
            println!("{:<11} : {:.2} ms\n", "Elapsed", elapsed.as_secs_f64() * 1000.0);
        }

        let shell_ctx = ShellContext::parse(cmd);
        let found_in_shell = shell_ctx.is_found();
        shell_contexts.insert(cmd.clone(), shell_ctx.clone());

        if matches_for_cmd.is_empty() && !found_in_shell {
            missing_any = true;
            
            if !is_structured && !cli.plain && !cli.trace && !cli.quiet {
                println!("{}: Command not found.", cmd.red());
                
                // If shell wrapper is not active, give a hint
                if std::env::var("WHERE_SHELL").is_err() && (cli.explain || cli.resolve || cmd == "cd" || cmd == "ll") {
                    println!("\n{}: Shell integration is not active. `where` cannot detect aliases or builtins.", "Hint".yellow());
                    println!("To enable it, add the following to your shell config:");
                    println!("  Bash/Zsh: eval \"$(where --init bash)\"");
                    println!("  Fish:     where --init fish | source");
                }
                
                // Fuzzy search
                let mut best_match = None;
                let mut best_score = 0.0;
                for cached_cmd in path_cache.keys() {
                    let score = strsim::jaro_winkler(cmd, cached_cmd);
                    if score > 0.85 && score > best_score {
                        best_score = score;
                        best_match = Some(cached_cmd);
                    }
                }
                if let Some(suggestion) = best_match {
                    println!("\nDid you mean?\n  {}", suggestion.green());
                }

                if cli.suggest {
                    suggest_packages(cmd);
                }
            }
        }

        results.insert(cmd.clone(), matches_for_cmd);
    }

    if cli.quiet {
        // Suppress all output
    } else if cli.json {
        let output_json = if cli.trace {
            let mut trace_paths = Vec::new();
            for (i, p) in paths.iter().enumerate() {
                let mut matched = false;
                for matches in results.values() {
                    for m in matches {
                        if m.path.parent() == Some(p) || m.aliases.iter().any(|a| a.parent() == Some(p)) {
                            matched = true;
                            break;
                        }
                    }
                    if matched { break; }
                }
                trace_paths.push(TracePathInfo {
                    index: i + 1,
                    directory: p.clone(),
                    matched,
                });
            }
            
            let mut formatted_results: HashMap<String, CommandResult> = HashMap::new();
            for (cmd, matches) in results {
                let shell_context = shell_contexts.get(&cmd).cloned().filter(|c| c.is_found());
                formatted_results.insert(cmd, CommandResult { shell_context, matches });
            }

            let root = RootJson {
                trace: Some(TraceBlock {
                    path: trace_paths,
                    timing: TraceTiming {
                        directories: dirs_searched,
                        entries: files_examined,
                        workers: rayon::current_num_threads(),
                        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
                    }
                }),
                results: formatted_results,
            };
            if cli.pretty {
                serde_json::to_string_pretty(&root).unwrap()
            } else {
                serde_json::to_string(&root).unwrap()
            }
        } else {
            let mut formatted_results: HashMap<String, CommandResult> = HashMap::new();
            for (cmd, matches) in results {
                let shell_context = shell_contexts.get(&cmd).cloned().filter(|c| c.is_found());
                formatted_results.insert(cmd, CommandResult { shell_context, matches });
            }
            
            if cli.pretty {
                serde_json::to_string_pretty(&formatted_results).unwrap()
            } else {
                serde_json::to_string(&formatted_results).unwrap()
            }
        };
        println!("{}", output_json);
    } else if cli.yaml {
        let yaml = serde_yaml::to_string(&results).unwrap();
        println!("{}", yaml);
    } else if cli.csv {
        let mut wtr = csv::Writer::from_writer(io::stdout());
        wtr.write_record(&["command", "path", "canonical", "aliases", "owner", "permissions", "inode", "size", "sha256", "package", "version", "filesystem", "executable"]).unwrap();
        for (cmd, matches) in &results {
            for m in matches {
                let aliases_str = m.aliases.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(";");
                wtr.write_record(&[
                    cmd.clone(),
                    m.path.to_string_lossy().into_owned(),
                    m.canonical.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                    aliases_str,
                    m.owner.clone().unwrap_or_default(),
                    m.permissions.clone().unwrap_or_default(),
                    m.inode.map(|i| i.to_string()).unwrap_or_default(),
                    m.size.map(|s| s.to_string()).unwrap_or_default(),
                    m.hash.clone().unwrap_or_default(),
                    m.package.clone().unwrap_or_default(),
                    m.version.clone().unwrap_or_default(),
                    m.filesystem.clone().unwrap_or_default(),
                    m.executable.to_string(),
                ]).unwrap();
            }
        }
        wtr.flush().unwrap();
    } else if cli.plain {
        for matches in results.values() {
            for m in matches {
                println!("{}", m.path.to_string_lossy());
            }
        }
    } else if cli.trace {
        for cmd in &commands {
            if let Some(matches) = results.get(cmd) {
                for (idx, m) in matches.iter().enumerate() {
                    if idx > 0 { println!("---"); }
                    println!("{}:", "Executable".bold());
                    println!("{}", m.path.to_string_lossy().green());
                    if let Some(ref owner) = m.owner {
                        println!("\n{}:", "Owner".bold());
                        println!("{}", owner.yellow());
                    }
                    if let Some(ref pkg) = m.package {
                        println!("\n{}:", "Package".bold());
                        println!("{}", pkg.cyan());
                    }
                    if let Some(ref ver) = m.version {
                        println!("\n{}:", "Version".bold());
                        println!("{}", ver.magenta());
                    }
                    if let Some(ref fs) = m.filesystem {
                        println!("\n{}:", "Filesystem".bold());
                        println!("{}", fs.cyan());
                    }
                    if let Some(ref hash) = m.hash {
                        println!("\n{}:", "SHA256".bold());
                        println!("{}", hash.blue());
                    }
                    if let Some(ref arch) = m.arch {
                        println!("\n{}:", "Architecture".bold());
                        println!("{}", arch.cyan());
                    }
                    if let Some(ref sec) = m.security {
                        println!("\n{}:", "Security".bold());
                        println!("{}", sec.yellow());
                    }
                    if let Some(ref libs) = m.libs {
                        println!("\n{}:", "Libraries".bold());
                        for lib in libs {
                            println!("{}", lib);
                        }
                    }
                    if let Some(ref canon) = m.canonical {
                        println!("\n{}:", "Canonical".bold());
                        println!("{}", canon.to_string_lossy());
                    }
                    if !m.aliases.is_empty() {
                        println!("\n{}:", "Aliases".bold());
                        for alias in &m.aliases {
                            println!("{}", alias.to_string_lossy());
                        }
                    }
                }
            }
        }
    } else {
        let mut first_cmd = true;
        for cmd in &commands {
            if let Some(matches) = results.get(cmd) {
                let shell_ctx = shell_contexts.get(cmd).unwrap();
                if !matches.is_empty() || shell_ctx.is_found() {
                    if !first_cmd && !cli.why {
                        println!();
                    }
                    if !cli.why {
                        println!("{}", cmd.bold());
                    }

                    if cli.resolve {
                        println!("Resolution order");
                        println!();
                        if shell_ctx.alias.is_some() {
                            println!("✓ {}", "alias".green());
                        } else {
                            println!("✗ {}", "alias".red());
                        }
                        if shell_ctx.is_function {
                            println!("✓ {}", "function".green());
                        } else {
                            println!("✗ {}", "function".red());
                        }
                        if shell_ctx.is_builtin {
                            println!("✓ {}", "builtin".green());
                        } else {
                            println!("✗ {}", "builtin".red());
                        }
                        if shell_ctx.abbreviation.is_some() {
                            println!("✓ {}", "abbreviation".green());
                        } else {
                            // Only print abbr if fish shell
                            if shell_ctx.shell_name.as_deref() == Some("fish") {
                                println!("✗ {}", "abbreviation".red());
                            }
                        }
                        if shell_ctx.env_var.is_some() {
                            println!("✓ {}", "env var".green());
                        } else {
                            println!("✗ {}", "env var".red());
                        }
                        if !matches.is_empty() {
                            println!("✓ {}", "executable".green());
                        } else {
                            println!("✗ {}", "executable".red());
                        }
                        println!();
                    }

                    if cli.explain {
                        println!("Shell searched:");
                        println!();
                        println!("{} alias", if shell_ctx.alias.is_some() { "✓" } else { "✗" });
                        println!("{} function", if shell_ctx.is_function { "✓" } else { "✗" });
                        println!("{} builtin", if shell_ctx.is_builtin { "✓" } else { "✗" });
                        println!("{} env var", if shell_ctx.env_var.is_some() { "✓" } else { "✗" });
                        println!("{} PATH", if !matches.is_empty() { "✓" } else { "✗" });
                        println!();
                        println!("Selected:");
                        if let Some(ref alias) = shell_ctx.alias {
                            println!("alias {}='{}'", cmd, alias);
                            println!("\nReason:\nMatched shell alias");
                        } else if shell_ctx.is_function {
                            println!("{} (function)", cmd);
                            println!("\nReason:\nNo alias\nMatched shell function");
                        } else if shell_ctx.is_builtin {
                            println!("{} (builtin)", cmd);
                            println!("\nReason:\nNo alias\nNo function\nMatched shell builtin");
                        } else if let Some(ref abbr) = shell_ctx.abbreviation {
                            println!("abbr {}='{}'", cmd, abbr);
                            println!("\nReason:\nMatched fish abbreviation");
                        } else if let Some(ref env_val) = shell_ctx.env_var {
                            println!("{}={}", cmd, env_val);
                            println!("\nReason:\nMatched environment variable");
                        } else if let Some(m) = matches.first() {
                            println!("{}", m.path.to_string_lossy());
                            println!("\nReason:\nNo alias\nNo function\nNot a builtin\nExecutable found in PATH");
                        }
                        println!();
                    } else if cli.resolve {
                        println!("Result:");
                        if let Some(ref alias) = shell_ctx.alias {
                            println!("alias {}='{}'", cmd, alias);
                        } else if shell_ctx.is_function {
                            println!("function {}", cmd);
                        } else if shell_ctx.is_builtin {
                            println!("builtin {}", cmd);
                        } else if let Some(ref abbr) = shell_ctx.abbreviation {
                            println!("abbr {}='{}'", cmd, abbr);
                        } else if let Some(ref env_val) = shell_ctx.env_var {
                            println!("{}={}", cmd, env_val);
                        } else if let Some(m) = matches.first() {
                            println!("{}", m.path.to_string_lossy());
                        }
                        println!();
                    }

                    if let Some(ref alias) = shell_ctx.alias {
                        println!(" └─ {}", "shell alias".cyan());
                        println!("    expands to: {}", alias);
                    }
                    if shell_ctx.is_function {
                        println!(" └─ {}", "shell function".cyan());
                    }
                    if shell_ctx.is_builtin {
                        println!(" └─ {}", "shell builtin".cyan());
                    }
                    if let Some(ref abbr) = shell_ctx.abbreviation {
                        println!(" └─ {}", "shell abbreviation".cyan());
                        println!("    expands to: {}", abbr);
                    }
                    if let Some(ref env_val) = shell_ctx.env_var {
                        println!(" └─ {}", "environment variable".cyan());
                        if env_val.contains(':') {
                            println!("    value:");
                            for part in env_val.split(':') {
                                println!("      {}", part);
                            }
                        } else {
                            println!("    value: {}", env_val);
                        }
                    }
                    for m in matches {
                        let path_str = m.path.to_string_lossy();
                        if m.executable {
                            print!(" └─ {}", path_str.green());
                        } else {
                            print!(" └─ {}", path_str.red());
                        }
                        
                        if let Some(ref target) = m.symlink_target {
                            print!(" -> {}", target.to_string_lossy().cyan());
                        }
                        println!();
                        
                        let mut extra = Vec::new();
                        
                        if cli.verbose || cli.show_size || cli.hash || !m.aliases.is_empty() || cli.package || cli.version_info || cli.arch || cli.security || cli.libs {
                            if !m.aliases.is_empty() {
                                extra.push(format!("aliases:"));
                                for alias in &m.aliases {
                                    extra.push(format!("  {}", alias.to_string_lossy()));
                                }
                            }
                            if cli.verbose {
                                extra.push(format!("executable: {}", m.executable));
                                if let Some(ref owner) = m.owner {
                                    extra.push(format!("owner: {}", owner.yellow()));
                                }
                                if let Some(ref perms) = m.permissions {
                                    extra.push(format!("permissions: {}", perms.yellow()));
                                }
                                if let Some(inode) = m.inode {
                                    extra.push(format!("inode: {}", inode));
                                }
                                if let Some(ref fs) = m.filesystem {
                                    extra.push(format!("filesystem: {}", fs));
                                }
                            }
                            if cli.show_size {
                                if let Some(size) = m.size {
                                    if !cli.verbose {
                                        extra.push(format!("size: {} bytes", size));
                                    } else {
                                        extra.push(format!("size: {}", size));
                                    }
                                }
                            }
                            if let Some(ref hash) = m.hash {
                                extra.push(format!("sha256: {}", hash.blue()));
                            }
                            
                            if let Some(ref pkg) = m.package {
                                extra.push(format!("package: {}", pkg.cyan()));
                            }
                            if let Some(ref ver) = m.version {
                                extra.push(format!("{}", ver.magenta()));
                            }
                            if let Some(ref arch) = m.arch {
                                extra.push(format!("arch: {}", arch.cyan()));
                            }
                            if let Some(ref sec) = m.security {
                                extra.push(format!("security: {}", sec.yellow()));
                            }
                            if let Some(ref libs) = m.libs {
                                extra.push(format!("libraries: {}", libs.join(", ")));
                            }

                            for line in extra {
                                println!("    {}", line);
                            }
                        }
                    }
                    first_cmd = false;
                }
            }
        }
    }

    if cli.benchmark && !cli.trace {
        println!("Directories searched : {}", dirs_searched);
        println!("Files examined       : {}", files_examined);
        println!("Duplicates removed   : {}", duplicates_removed);
        println!("Elapsed              : {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    } else if cli.time && !cli.trace {
        println!("Search completed in {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    }

    if missing_any {
        std::process::exit(1);
    }
}

fn suggest_packages(cmd: &str) {
    use std::process::Command;

    // pacman (Arch Linux)
    if let Ok(out) = Command::new("pacman").args(["-Fq", cmd]).output() {
        if out.status.success() {
            let packages = String::from_utf8_lossy(&out.stdout);
            let mut pkgs: Vec<&str> = packages
                .lines()
                .map(|line| line.split('/').last().unwrap_or(line))
                .collect();
            pkgs.sort();
            pkgs.dedup();
            if !pkgs.is_empty() {
                println!("\n{} Available in packages (via pacman):", "📦".cyan());
                for pkg in pkgs {
                    println!("    sudo pacman -S {}", pkg.bold());
                }
            }
        }
    }

    // apt-file (Debian/Ubuntu)
    if let Ok(out) = Command::new("apt-file").args(["search", "-x", &format!("^/bin/{}$", cmd)]).output() {
        if out.status.success() {
            let packages = String::from_utf8_lossy(&out.stdout);
            let mut pkgs: Vec<&str> = packages
                .lines()
                .filter_map(|line| line.split(':').next())
                .collect();
            pkgs.sort();
            pkgs.dedup();
            if !pkgs.is_empty() {
                println!("\n{} Available in packages (via apt):", "📦".cyan());
                for pkg in pkgs {
                    println!("    sudo apt install {}", pkg.bold());
                }
            }
        }
    }

    // dnf (Fedora/RHEL)
    if let Ok(out) = Command::new("dnf").args(["provides", &format!("*/bin/{}", cmd)]).output() {
        if out.status.success() {
            let packages = String::from_utf8_lossy(&out.stdout);
            let mut pkgs: Vec<&str> = packages
                .lines()
                .filter(|line| !line.contains("Repo") && !line.contains("Matched") && line.contains(':'))
                .filter_map(|line| line.split('-').next())
                .collect();
            pkgs.sort();
            pkgs.dedup();
            if !pkgs.is_empty() {
                println!("\n{} Available in packages (via dnf):", "📦".cyan());
                for pkg in pkgs {
                    println!("    sudo dnf install {}", pkg.bold());
                }
            }
        }
    }

    // brew (macOS)
    if let Ok(out) = Command::new("brew").args(["which-formula", cmd]).output() {
        if out.status.success() {
            let pkg = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !pkg.is_empty() && !pkg.contains("Error") {
                println!("\n{} Available in packages (via brew):", "📦".cyan());
                println!("    brew install {}", pkg.bold());
            }
        }
    }

    // cargo
    if let Ok(out) = Command::new("cargo").args(["search", "--limit", "1", cmd]).output() {
        if out.status.success() {
            let output = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = output.lines().next() {
                if line.starts_with(&format!("{} =", cmd)) {
                    println!("\n{} Available in crates.io (via cargo):", "📦".cyan());
                    println!("    cargo install {}", cmd.bold());
                }
            }
        }
    }

    // npm
    if let Ok(out) = Command::new("npm").args(["view", cmd, "name"]).output() {
        if out.status.success() {
            let pkg = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !pkg.is_empty() && pkg == cmd {
                println!("\n{} Available in npm (via Node.js):", "📦".cyan());
                println!("    npm install -g {}", pkg.bold());
            }
        }
    }
}

fn ensure_shell_hooks() {
    let home = env::var("HOME").unwrap_or_default();
    if home.is_empty() { return; }
    
    let mut installed_any = false;
    
    let bashrc = PathBuf::from(&home).join(".bashrc");
    if bashrc.exists() {
        if let Ok(content) = fs::read_to_string(&bashrc) {
            if !content.contains("where --init bash") {
                if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&bashrc) {
                    let _ = writeln!(file, "\n# where shell integration\neval \"$(where --init bash)\"");
                    installed_any = true;
                }
            }
        }
    }
    
    let zshrc = PathBuf::from(&home).join(".zshrc");
    if zshrc.exists() {
        if let Ok(content) = fs::read_to_string(&zshrc) {
            if !content.contains("where --init zsh") {
                if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&zshrc) {
                    let _ = writeln!(file, "\n# where shell integration\neval \"$(where --init zsh)\"");
                    installed_any = true;
                }
            }
        }
    }
    
    let fish_config = PathBuf::from(&home).join(".config/fish/config.fish");
    if fish_config.exists() {
        if let Ok(content) = fs::read_to_string(&fish_config) {
            if !content.contains("where --init fish") {
                if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&fish_config) {
                    let _ = writeln!(file, "\n# where shell integration\nwhere --init fish | source");
                    installed_any = true;
                }
            }
        }
    }
    
    if installed_any {
        println!("{}", "[where] Automatically installed shell hooks into your RC files.".cyan());
        println!("{}", "[where] Please restart your terminal to enable shell integration!".cyan());
    }
}
