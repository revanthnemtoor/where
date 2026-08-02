use std::fs;
use std::os::unix::fs::symlink;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_deduplication() {
    let dir = tempdir().unwrap();
    let bin_dir = dir.path().join("bin");
    let usr_bin_dir = dir.path().join("usr_bin");
    
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&usr_bin_dir).unwrap();
    
    let python_real = usr_bin_dir.join("python");
    fs::write(&python_real, b"print('hello')").unwrap();
    fs::set_permissions(&python_real, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    
    // Create an alias / symlink in bin_dir pointing to the same file
    let python_alias = bin_dir.join("python");
    symlink(&python_real, &python_alias).unwrap();

    let path_env = format!("{}:{}", bin_dir.display(), usr_bin_dir.display());

    let output = Command::new(env!("CARGO_BIN_EXE_where"))
        .env("PATH", path_env)
        .arg("--json")
        .arg("python")
        .output()
        .unwrap();

    assert!(output.status.success());
    let out_str = String::from_utf8_lossy(&output.stdout);
    
    // Parse the JSON
    let json: serde_json::Value = serde_json::from_str(&out_str).unwrap();
    
    // The results map should have a "python" key with a list of matches
    let matches = json.get("python").unwrap().as_array().unwrap();
    
    // Deduplication should ensure we only get 1 match because they point to the same inode
    assert_eq!(matches.len(), 1);
    
    let first_match = &matches[0];
    let aliases = first_match.get("aliases").unwrap().as_array().unwrap();
    assert_eq!(aliases.len(), 1); // One alias was recorded
}

#[test]
fn test_elf_parsing() {
    let output = Command::new(env!("CARGO_BIN_EXE_where"))
        .env("PATH", "/bin:/usr/bin")
        .arg("--json")
        .arg("--arch")
        .arg("--libs")
        .arg("--security")
        .arg("ls")
        .output()
        .unwrap();

    assert!(output.status.success());
    let out_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&out_str).unwrap();
    
    let matches = json.get("ls").unwrap().as_array().unwrap();
    assert!(!matches.is_empty());
    
    let first_match = &matches[0];
    assert!(first_match.get("arch").is_some(), "arch should be populated");
    assert!(first_match.get("security").is_some(), "security should be populated");
    assert!(first_match.get("libs").is_some(), "libs should be populated");
}

#[test]
fn test_env_path_and_deep() {
    let dir = tempdir().unwrap();
    let base_dir = dir.path().join("base");
    let deep_dir = base_dir.join("deep").join("nested");
    
    fs::create_dir_all(&deep_dir).unwrap();
    
    let python_real = deep_dir.join("python");
    fs::write(&python_real, b"print('hello')").unwrap();
    fs::set_permissions(&python_real, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_where"))
        .arg("--json")
        .arg("--env-path")
        .arg(base_dir.to_str().unwrap())
        .arg("--deep")
        .arg("3")
        .arg("python")
        .output()
        .unwrap();

    assert!(output.status.success());
    let out_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&out_str).unwrap();
    
    let matches = json.get("python").unwrap().as_array().unwrap();
    assert_eq!(matches.len(), 1);
    
    let path = matches[0].get("path").unwrap().as_str().unwrap();
    assert!(path.contains("deep/nested/python"));
}
