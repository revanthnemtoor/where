use clap::Parser;
use colored::{Colorize, control};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "where")]
#[command(about = "A Linux where implementation with Windows-like behavior and Unix features")]
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

    /// Machine-readable output
    #[arg(long)]
    json: bool,

    /// Colored output
    #[arg(long)]
    color: bool,
    
    /// Compute SHA-256 hash
    #[arg(long)]
    hash: bool,
    
    /// Show execution time
    #[arg(long)]
    time: bool,

    /// Print about information
    #[arg(long)]
    about: bool,

    /// Commands to search for
    #[arg(required_unless_present = "about")]
    commands: Vec<String>,
}

#[derive(Serialize)]
struct Match {
    path: PathBuf,
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
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    let mut perm_str = String::new();
    perm_str.push(if mode & 0o170000 == 0o120000 { 'l' } else { '-' });
    for (mask, c) in rwx {
        perm_str.push(if mode & mask != 0 { c } else { '-' });
    }
    perm_str
}

fn main() {
    let cli = Cli::parse();

    if cli.about {
        println!("where 0.1.0\nA modern replacement for which/where.\n\nAuthor : Revanth Reddy Nemtoor\nLicense: MIT\nRepository: https://github.com/revanthnemtoor/where");
        std::process::exit(0);
    }

    if cli.color {
        control::set_override(true);
    } else if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        control::set_override(false);
    }

    let start_time = Instant::now();

    let path_var = env::var("PATH").unwrap_or_default();
    let paths: Vec<PathBuf> = env::split_paths(&path_var).collect();

    let mut results: HashMap<String, Vec<Match>> = HashMap::new();
    let mut missing_any = false;

    for cmd in &cli.commands {
        let mut matches_for_cmd: Vec<Match> = Vec::new();
        let mut seen_inodes: HashMap<(u64, u64), usize> = HashMap::new();
        
        for dir in &paths {
            let full_path = dir.join(cmd);
            if full_path.exists() {
                let executable = is_executable(&full_path);
                
                let sym_meta = fs::symlink_metadata(&full_path).ok();
                let target_meta = fs::metadata(&full_path).ok();
                
                if let Some(meta) = &target_meta {
                    let dev = meta.dev();
                    let ino = meta.ino();
                    
                    if let Some(&index) = seen_inodes.get(&(dev, ino)) {
                        matches_for_cmd[index].aliases.push(full_path);
                        continue;
                    }
                    
                    seen_inodes.insert((dev, ino), matches_for_cmd.len());
                }

                let mut m = Match {
                    path: full_path.clone(),
                    aliases: Vec::new(),
                    symlink_target: None,
                    size: None,
                    inode: None,
                    owner: None,
                    permissions: None,
                    hash: None,
                    executable,
                };


                if let Some(sym_meta) = &sym_meta {
                    if cli.show_symlink || cli.verbose {
                        if sym_meta.file_type().is_symlink() {
                            m.symlink_target = fs::read_link(&full_path).ok();
                        }
                    }
                }
                
                if let Some(meta) = &target_meta {
                    if cli.show_size || cli.verbose {
                        m.size = Some(meta.size());
                    }
                    if cli.verbose {
                        m.inode = Some(meta.ino());
                        m.owner = Some(get_user_name(meta.uid()));
                        m.permissions = Some(format_mode(meta.mode()));
                    }
                }

                if cli.hash {
                    if let Ok(bytes) = fs::read(&full_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        m.hash = Some(hex::encode(hasher.finalize()));
                    }
                }

                matches_for_cmd.push(m);

                if cli.first_only {
                    break;
                }
            }
        }

        let has_executable = matches_for_cmd.iter().any(|m| m.executable);
        if !has_executable {
            missing_any = true;
        }

        results.insert(cmd.clone(), matches_for_cmd);
    }

    if cli.json {
        let json = serde_json::to_string_pretty(&results).unwrap();
        println!("{}", json);
    } else {
        let mut first_cmd = true;
        for cmd in &cli.commands {
            if let Some(matches) = results.get(cmd) {
                if !matches.is_empty() {
                    if !first_cmd {
                        println!();
                    }
                    println!("{}", cmd.bold());
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
                        
                        if cli.verbose || cli.show_size || cli.hash || !m.aliases.is_empty() {
                            let mut extra = Vec::new();
                            if cli.verbose {
                                for alias in &m.aliases {
                                    extra.push(format!("also in PATH as: {}", alias.to_string_lossy()));
                                }
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

    if cli.time {
        let elapsed = start_time.elapsed();
        println!("Search completed in {:.2?}", elapsed);
    }

    if missing_any {
        std::process::exit(1);
    }
}
