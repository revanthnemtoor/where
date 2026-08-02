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
