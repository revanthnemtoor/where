use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use colored::{control, Colorize};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

#[derive(Parser)]
#[command(name = "where", version = "0.1.0", author = "Revanth Reddy Nemtoor")]
#[command(about = "A modern replacement for which/where.")]
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

    /// Show benchmark stats
    #[arg(long)]
    benchmark: bool,

    /// Generate shell completions
    #[arg(long, value_enum)]
    generate_completions: Option<Shell>,

    /// Print about information
    #[arg(long)]
    about: bool,

    /// Commands to search for
    #[arg(required_unless_present_any = ["about", "generate_completions"])]
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
    executable: bool,
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

fn main() {
    let cli = Cli::parse();

    if cli.color {
        control::set_override(true);
    } else if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        control::set_override(false);
    }

    if cli.about {
        println!("{} {}\n{}", "where".bold().green(), "0.1.0".cyan(), "A modern replacement for which/where.".italic());
        println!();
        println!("{:<10} : {}", "Author".bold(), "Revanth Reddy Nemtoor".yellow());
        println!("{:<10} : {}", "License".bold(), "MIT".yellow());
        println!("{:<10} : {}", "Repository".bold(), "https://github.com/revanthnemtoor/where".blue());
        std::process::exit(0);
    }

    if let Some(generator) = cli.generate_completions {
        let mut cmd = Cli::command();
        generate(generator, &mut cmd, "where", &mut io::stdout());
        std::process::exit(0);
    }

    let start_time = Instant::now();
    let mut dirs_searched = 0;
    let mut files_examined = 0;
    let mut duplicates_removed = 0;

    let path_var = env::var("PATH").unwrap_or_default();
    let paths: Vec<PathBuf> = env::split_paths(&path_var).collect();

    // Cache PATH contents
    let mut path_cache: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for dir in &paths {
        dirs_searched += 1;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    files_examined += 1;
                    if file_type.is_file() || file_type.is_symlink() {
                        if let Ok(name) = entry.file_name().into_string() {
                            path_cache.entry(name).or_default().push(entry.path());
                        }
                    }
                }
            }
        }
    }

    let mut results: HashMap<String, Vec<Match>> = HashMap::new();
    let mut missing_any = false;

    let is_structured = cli.json || cli.yaml || cli.csv;
    let fetch_all = is_structured;

    for cmd in &cli.commands {
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

        if let Some(cmd_paths) = path_cache.get(cmd) {
            for full_path in cmd_paths {
                let executable = is_executable(full_path);
                
                let sym_meta = fs::symlink_metadata(full_path).ok();
                let target_meta = fs::metadata(full_path).ok();
                
                if let Some(meta) = &target_meta {
                    let dev = meta.dev();
                    let ino = meta.ino();
                    
                    if let Some(&index) = seen_inodes.get(&(dev, ino)) {
                        matches_for_cmd[index].aliases.push(full_path.clone());
                        duplicates_removed += 1;
                        continue;
                    }
                    
                    seen_inodes.insert((dev, ino), matches_for_cmd.len());
                }

                let mut m = Match {
                    path: full_path.clone(),
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

                matches_for_cmd.push(m);

                if cli.first_only {
                    break;
                }
            }
        }

        if matches_for_cmd.is_empty() {
            missing_any = true;
            
            if !is_structured && !cli.plain {
                println!("{}: Command not found.", cmd.red());
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
            }
        }

        results.insert(cmd.clone(), matches_for_cmd);
    }

    if cli.json {
        let json = serde_json::to_string_pretty(&results).unwrap();
        println!("{}", json);
    } else if cli.yaml {
        let yaml = serde_yaml::to_string(&results).unwrap();
        println!("{}", yaml);
    } else if cli.csv {
        let mut wtr = csv::Writer::from_writer(io::stdout());
        wtr.write_record(&["command", "path", "canonical", "aliases", "owner", "permissions", "inode", "size", "sha256", "package", "version", "executable"]).unwrap();
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
    } else {
        let mut first_cmd = true;
        for cmd in &cli.commands {
            if let Some(matches) = results.get(cmd) {
                if !matches.is_empty() {
                    if !first_cmd && !cli.why {
                        println!();
                    }
                    if !cli.why {
                        println!("{}", cmd.bold());
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
                        
                        if cli.verbose || cli.show_size || cli.hash || !m.aliases.is_empty() || cli.package || cli.version_info {
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

    let elapsed = start_time.elapsed();

    if cli.benchmark {
        println!("Directories searched : {}", dirs_searched);
        println!("Files examined       : {}", files_examined);
        println!("Duplicates removed   : {}", duplicates_removed);
        println!("Elapsed              : {:?}", elapsed);
    } else if cli.time {
        println!("Search completed in {:?}", elapsed);
    }

    if missing_any {
        std::process::exit(1);
    }
}
