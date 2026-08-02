use std::process::Command;

pub struct ContainerEngine {
    executable: String,
    image: String,
}

impl ContainerEngine {
    pub fn new(engine: Option<String>, image: String) -> Result<Self, String> {
        let executable = if let Some(eng) = engine {
            eng
        } else if which::which("docker").is_ok() {
            "docker".to_string()
        } else if which::which("podman").is_ok() {
            "podman".to_string()
        } else {
            return Err("Neither docker nor podman found in PATH".to_string());
        };
        Ok(Self { executable, image })
    }

    pub fn get_path_env(&self) -> Result<String, String> {
        let out = Command::new(&self.executable)
            .args(["run", "--rm", "--entrypoint", "sh", &self.image, "-c", "echo $PATH"])
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", self.executable, e))?;
        
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(format!("Container execution failed: {}", String::from_utf8_lossy(&out.stderr)))
        }
    }

    pub fn find_command_paths(&self, path_env: &str, cmd: &str) -> Vec<String> {
        let script = format!(r#"
            IFS=':'
            for d in $PATH; do
                if [ -x "$d/{cmd}" ] && [ ! -d "$d/{cmd}" ]; then
                    echo "$d/{cmd}"
                fi
            done
        "#, cmd = cmd);
        
        let out = Command::new(&self.executable)
            .args(["run", "--rm", "-e", &format!("PATH={}", path_env), "--entrypoint", "sh", &self.image, "-c", &script])
            .output();
            
        let mut paths = Vec::new();
        if let Ok(output) = out {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let path = line.trim();
                    if !path.is_empty() {
                        paths.push(path.to_string());
                    }
                }
            }
        }
        paths
    }

    pub fn extract_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let out = Command::new(&self.executable)
            .args(["run", "--rm", "--entrypoint", "cat", &self.image, path])
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", self.executable, e))?;

        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(format!("Failed to extract file: {}", String::from_utf8_lossy(&out.stderr)))
        }
    }
}
