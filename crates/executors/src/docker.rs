use std::path::{Path, PathBuf};

use crate::env::ExecutionEnv;

pub struct DockerSettings {
    pub image: String,
    pub extra_mounts: Vec<(PathBuf, String)>, // (host_path, container_path)
    pub network_mode: Option<String>,
}

/// Returns a list of standard configuration files/directories to mount if they exist.
/// Includes .claude.json, .ssh, .gitconfig, and .config/amp.
pub fn get_standard_mounts() -> Vec<(PathBuf, String)> {
    let mut mounts = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let candidates = vec![
            (".claude.json", "/root/.claude.json"),
            (".ssh", "/root/.ssh"),
            (".gitconfig", "/root/.gitconfig"),
            (".config/amp", "/root/.config/amp"),
        ];

        for (rel, target) in candidates {
            let path = home.join(rel);
            if path.exists() {
                mounts.push((path, target.to_string()));
            }
        }
    }
    mounts
}

pub fn wrap_in_docker(
    program: &str,
    args: &[String],
    settings: &DockerSettings,
    current_dir: &Path,
    env: &ExecutionEnv,
) -> (String, Vec<String>) {
    let mut docker_args = vec![
        "run".to_string(),
        "--rm".to_string(), // Cleanup container after exit
        "-i".to_string(),   // Interactive (keep stdin open)
    ];

    // Mount workspace
    // We mount the current_dir (host) to /workspace (container)
    // Canonicalize to ensure absolute path for Docker mount
    let abs_current_dir = std::fs::canonicalize(current_dir).unwrap_or(current_dir.to_path_buf());
    let workspace_mount = format!("{}:/workspace", abs_current_dir.to_string_lossy());
    docker_args.push("-v".to_string());
    docker_args.push(workspace_mount);

    // Set working directory
    docker_args.push("-w".to_string());
    docker_args.push("/workspace".to_string());

    // Add extra mounts
    for (host_path, container_path) in &settings.extra_mounts {
        if host_path.exists() {
            let abs_host_path = std::fs::canonicalize(host_path).unwrap_or(host_path.to_path_buf());
            docker_args.push("-v".to_string());
            docker_args.push(format!("{}:{}", abs_host_path.to_string_lossy(), container_path));
        }
    }

    // Network mode (host networking is often useful for local dev servers)
    if let Some(net) = &settings.network_mode {
        docker_args.push(format!("--network={}", net));
    } else {
        // Default to host network to allow accessing local ports easily
        docker_args.push("--network=host".to_string());
    }

    // Pass environment variables
    for (key, value) in &env.vars {
        docker_args.push("-e".to_string());
        docker_args.push(format!("{}={}", key, value));
    }

    // Note: We deliberately do not set -u (user) here.
    // Docker runs as root by default. This ensures the container can read
    // any mounted config files (often mounted to /root/...) and avoids
    // HOME directory issues. File ownership on Linux host might need manual cleanup.

    // Image
    docker_args.push(settings.image.clone());

    // Program and args
    // If the program is an absolute path on host, we use just the filename.
    // We assume the program (e.g. npx) is available in the PATH inside the container.
    let program_name = Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program);

    docker_args.push(program_name.to_string());
    docker_args.extend_from_slice(args);

    ("docker".to_string(), docker_args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_in_docker_basic() {
        let settings = DockerSettings {
            image: "node:18".to_string(),
            extra_mounts: vec![],
            network_mode: None,
        };
        let current_dir = Path::new("/tmp"); // Must exist for canonicalize (on unix)
        let env = ExecutionEnv::new();
        let program = "npx";
        let args = vec!["-y".to_string(), "claude-code".to_string()];

        let (prog, result_args) = wrap_in_docker(program, &args, &settings, current_dir, &env);

        assert_eq!(prog, "docker");
        assert!(result_args.contains(&"run".to_string()));
        assert!(result_args.contains(&"--rm".to_string()));
        assert!(result_args.contains(&"-v".to_string()));
        // canonicalize of /tmp might resolve to /private/tmp on macOS, check ends with /workspace
        let mount_arg = result_args.iter().find(|a| a.ends_with(":/workspace")).unwrap();
        assert!(mount_arg.starts_with("/"));

        assert!(result_args.contains(&"node:18".to_string()));
        assert!(result_args.contains(&"npx".to_string()));
        assert!(result_args.contains(&"claude-code".to_string()));
    }

    #[test]
    fn test_wrap_in_docker_env() {
        let settings = DockerSettings {
            image: "alpine".to_string(),
            extra_mounts: vec![],
            network_mode: None,
        };
        let current_dir = std::env::current_dir().unwrap();
        let mut env = ExecutionEnv::new();
        env.insert("FOO", "BAR");

        let (_, result_args) = wrap_in_docker("echo", &[], &settings, &current_dir, &env);
        assert!(result_args.contains(&"-e".to_string()));
        assert!(result_args.contains(&"FOO=BAR".to_string()));
    }
}
