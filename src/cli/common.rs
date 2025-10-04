//! Common utilities and traits for CLI commands

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::manifest::{Manifest, find_manifest};

/// Common trait for CLI command execution pattern
pub trait CommandExecutor: Sized {
    /// Execute the command, finding the manifest automatically
    fn execute(self) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Send,
    {
        async move {
            let manifest_path = if let Ok(path) = find_manifest() {
                path
            } else {
                // Check if legacy CCPM files exist
                if let Some(migration_msg) = check_for_legacy_ccpm_files() {
                    return Err(anyhow::anyhow!("{migration_msg}"));
                }
                return Err(anyhow::anyhow!(
                    "No agpm.toml found in current directory or any parent directory. \
                     Run 'agpm init' to create a new project."
                ));
            };
            self.execute_from_path(manifest_path).await
        }
    }

    /// Execute the command with a specific manifest path
    fn execute_from_path(
        self,
        manifest_path: PathBuf,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Common context for CLI commands that need manifest and project information
#[derive(Debug)]
pub struct CommandContext {
    /// Parsed project manifest (agpm.toml)
    pub manifest: Manifest,
    /// Path to the manifest file
    pub manifest_path: PathBuf,
    /// Project root directory (containing agpm.toml)
    pub project_dir: PathBuf,
    /// Path to the lockfile (agpm.lock)
    pub lockfile_path: PathBuf,
}

impl CommandContext {
    /// Create a new command context from a manifest path
    pub fn from_manifest_path(manifest_path: impl AsRef<Path>) -> Result<Self> {
        let manifest_path = manifest_path.as_ref();

        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "Manifest file {} not found",
                manifest_path.display()
            ));
        }

        let project_dir = manifest_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid manifest path"))?
            .to_path_buf();

        let manifest = Manifest::load(manifest_path).with_context(|| {
            format!("Failed to parse manifest file: {}", manifest_path.display())
        })?;

        let lockfile_path = project_dir.join("agpm.lock");

        Ok(Self {
            manifest,
            manifest_path: manifest_path.to_path_buf(),
            project_dir,
            lockfile_path,
        })
    }

    /// Load an existing lockfile if it exists
    pub fn load_lockfile(&self) -> Result<Option<crate::lockfile::LockFile>> {
        if self.lockfile_path.exists() {
            let lockfile =
                crate::lockfile::LockFile::load(&self.lockfile_path).with_context(|| {
                    format!("Failed to load lockfile: {}", self.lockfile_path.display())
                })?;
            Ok(Some(lockfile))
        } else {
            Ok(None)
        }
    }

    /// Save a lockfile to the project directory
    pub fn save_lockfile(&self, lockfile: &crate::lockfile::LockFile) -> Result<()> {
        lockfile
            .save(&self.lockfile_path)
            .with_context(|| format!("Failed to save lockfile: {}", self.lockfile_path.display()))
    }
}

/// Check for legacy CCPM files and return a migration message if found.
///
/// This function searches for ccpm.toml and ccpm.lock files in the current
/// directory and parent directories, similar to how `find_manifest` works.
/// If legacy files are found, it returns a helpful error message suggesting
/// to run the migration command.
///
/// # Returns
///
/// - `Some(String)` with migration instructions if legacy files are found
/// - `None` if no legacy files are detected
pub fn check_for_legacy_ccpm_files() -> Option<String> {
    check_for_legacy_ccpm_files_from(std::env::current_dir().ok()?)
}

/// Check for legacy CCPM files starting from a specific directory.
///
/// This is the internal implementation that allows for testing without
/// changing the current working directory.
fn check_for_legacy_ccpm_files_from(start_dir: PathBuf) -> Option<String> {
    let current = start_dir;
    let mut dir = current.as_path();

    loop {
        let ccpm_toml = dir.join("ccpm.toml");
        let ccpm_lock = dir.join("ccpm.lock");

        if ccpm_toml.exists() || ccpm_lock.exists() {
            let mut files = Vec::new();
            if ccpm_toml.exists() {
                files.push("ccpm.toml");
            }
            if ccpm_lock.exists() {
                files.push("ccpm.lock");
            }

            let files_str = files.join(" and ");
            let location = if dir == current {
                "current directory".to_string()
            } else {
                format!("parent directory: {}", dir.display())
            };

            return Some(format!(
                "{}\n\n{} {} found in {}.\n{}\n  {}\n\n{}",
                "Legacy CCPM files detected!".yellow().bold(),
                "→".cyan(),
                files_str,
                location,
                "Run the migration command to upgrade:".yellow(),
                format!("agpm migrate --path {}", dir.display())
                    .cyan()
                    .bold(),
                "Or run 'agpm init' to create a new AGPM project.".dimmed()
            ));
        }

        dir = dir.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_command_context_from_manifest_path() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create a test manifest
        std::fs::write(
            &manifest_path,
            r#"
[sources]
test = "https://github.com/test/repo.git"

[agents]
"#,
        )
        .unwrap();

        let context = CommandContext::from_manifest_path(&manifest_path).unwrap();

        assert_eq!(context.manifest_path, manifest_path);
        assert_eq!(context.project_dir, temp_dir.path());
        assert_eq!(context.lockfile_path, temp_dir.path().join("agpm.lock"));
        assert!(context.manifest.sources.contains_key("test"));
    }

    #[test]
    fn test_command_context_missing_manifest() {
        let result = CommandContext::from_manifest_path("/nonexistent/agpm.toml");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_command_context_invalid_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create an invalid manifest
        std::fs::write(&manifest_path, "invalid toml {{").unwrap();

        let result = CommandContext::from_manifest_path(&manifest_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse manifest")
        );
    }

    #[test]
    fn test_load_lockfile_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        let lockfile_path = temp_dir.path().join("agpm.lock");

        // Create test files
        std::fs::write(&manifest_path, "[sources]\n").unwrap();
        std::fs::write(
            &lockfile_path,
            r#"
version = 1

[[sources]]
name = "test"
url = "https://github.com/test/repo.git"
commit = "abc123"
fetched_at = "2024-01-01T00:00:00Z"
"#,
        )
        .unwrap();

        let context = CommandContext::from_manifest_path(&manifest_path).unwrap();
        let lockfile = context.load_lockfile().unwrap();

        assert!(lockfile.is_some());
        let lockfile = lockfile.unwrap();
        assert_eq!(lockfile.sources.len(), 1);
        assert_eq!(lockfile.sources[0].name, "test");
    }

    #[test]
    fn test_load_lockfile_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        let context = CommandContext::from_manifest_path(&manifest_path).unwrap();
        let lockfile = context.load_lockfile().unwrap();

        assert!(lockfile.is_none());
    }

    #[test]
    fn test_save_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        std::fs::write(&manifest_path, "[sources]\n").unwrap();

        let context = CommandContext::from_manifest_path(&manifest_path).unwrap();

        let lockfile = crate::lockfile::LockFile {
            version: 1,
            sources: vec![],
            agents: vec![],
            snippets: vec![],
            commands: vec![],
            scripts: vec![],
            hooks: vec![],
            mcp_servers: vec![],
        };

        context.save_lockfile(&lockfile).unwrap();

        assert!(context.lockfile_path.exists());
        let saved_content = std::fs::read_to_string(&context.lockfile_path).unwrap();
        assert!(saved_content.contains("version = 1"));
    }

    #[test]
    fn test_check_for_legacy_ccpm_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let result = check_for_legacy_ccpm_files_from(temp_dir.path().to_path_buf());
        assert!(result.is_none());
    }

    #[test]
    fn test_check_for_legacy_ccpm_toml_only() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("ccpm.toml"), "[sources]\n").unwrap();

        let result = check_for_legacy_ccpm_files_from(temp_dir.path().to_path_buf());
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("Legacy CCPM files detected"));
        assert!(msg.contains("ccpm.toml"));
        assert!(msg.contains("agpm migrate"));
    }

    #[test]
    fn test_check_for_legacy_ccpm_lock_only() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("ccpm.lock"), "# lock\n").unwrap();

        let result = check_for_legacy_ccpm_files_from(temp_dir.path().to_path_buf());
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("ccpm.lock"));
    }

    #[test]
    fn test_check_for_legacy_ccpm_both_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("ccpm.toml"), "[sources]\n").unwrap();
        std::fs::write(temp_dir.path().join("ccpm.lock"), "# lock\n").unwrap();

        let result = check_for_legacy_ccpm_files_from(temp_dir.path().to_path_buf());
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("ccpm.toml and ccpm.lock"));
    }
}
