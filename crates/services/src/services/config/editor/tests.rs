#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::services::config::editor::{EditorConfig, EditorType};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_openvscode_server_command_resolution() {
        // Create a mock script that acts as openvscode-server
        let mut mock_script = NamedTempFile::new().unwrap();
        let script_content = r#"#!/bin/sh
echo "Mock server started with args: $@"
"#;
        std::io::Write::write_all(&mut mock_script, script_content.as_bytes()).unwrap();

        // Make it executable
        let mut perms = fs::metadata(mock_script.path()).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(mock_script.path(), perms).unwrap();

        // Get path and close file to avoid ETXTBSY (Text file busy) error
        let mock_path = mock_script.path().to_string_lossy().to_string();
        mock_script.keep().unwrap(); // Keep file to run it

        // Ensure cleanup of the temp file
        struct TempFileCleaner(String);
        impl Drop for TempFileCleaner {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleaner = TempFileCleaner(mock_path.clone());

        let config = EditorConfig {
            editor_type: EditorType::OpenVsCodeServer,
            custom_command: None,
            remote_ssh_host: None,
            remote_ssh_user: None,
            open_vscode_server_url: None,
            open_vscode_server_bin: Some(mock_path.clone()),
        };

        // Check availability (should be true because bin exists)
        assert!(config.check_availability().await);

        // Verify command resolution
        let (executable, args) = config.resolve_command().await.unwrap();
        assert_eq!(executable.to_string_lossy(), mock_path);

        // spawn_local should work
        let result = config.spawn_local(Path::new(".")).await;
        if let Err(e) = &result {
             println!("Spawn failed: {:?}", e);
        }
        assert!(result.is_ok());

        // Note: We can't easily capture the output of spawn_local since it spawns a detached process
        // and doesn't return stdout. But the fact that resolve_command returns the correct path
        // and spawn_local succeeds confirms that we are launching the configured binary.
        // This validates that if the test environment provides a binary, we can launch it.
    }
}
