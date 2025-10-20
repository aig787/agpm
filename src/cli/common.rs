//! Common utilities and traits for CLI commands

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};

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
                // Check if legacy CCPM files exist and offer interactive migration
                match handle_legacy_ccpm_migration().await {
                    Ok(Some(path)) => path,
                    Ok(None) => {
                        return Err(anyhow::anyhow!(
                            "No agpm.toml found in current directory or any parent directory. \
                             Run 'agpm init' to create a new project."
                        ));
                    }
                    Err(e) => return Err(e),
                }
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
    ///
    /// # Errors
    /// Returns an error if the manifest file doesn't exist or cannot be read
    pub fn from_manifest_path(manifest_path: impl AsRef<Path>) -> Result<Self> {
        let manifest_path = manifest_path.as_ref();

        if !manifest_path.exists() {
            return Err(anyhow::anyhow!("Manifest file {} not found", manifest_path.display()));
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
    ///
    /// # Errors
    /// Returns an error if the lockfile exists but cannot be parsed
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
    ///
    /// # Errors
    /// Returns an error if the lockfile cannot be written
    pub fn save_lockfile(&self, lockfile: &crate::lockfile::LockFile) -> Result<()> {
        lockfile
            .save(&self.lockfile_path)
            .with_context(|| format!("Failed to save lockfile: {}", self.lockfile_path.display()))
    }
}

/// Handle legacy CCPM files by offering interactive migration.
///
/// This function searches for ccpm.toml and ccpm.lock files in the current
/// directory and parent directories. If found, it prompts the user to migrate
/// and performs the migration if they accept.
///
/// # Behavior
///
/// - **Interactive mode**: Prompts user with Y/n confirmation (stdin is a TTY)
/// - **Non-interactive mode**: Returns `Ok(None)` if stdin is not a TTY (e.g., CI/CD)
/// - **Search scope**: Traverses from current directory to filesystem root
///
/// # Returns
///
/// - `Ok(Some(PathBuf))` with the path to agpm.toml if migration succeeded
/// - `Ok(None)` if no legacy files were found OR user declined OR non-interactive mode
/// - `Err` if migration failed
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # async fn example() -> Result<()> {
/// use agpm_cli::cli::common::handle_legacy_ccpm_migration;
///
/// match handle_legacy_ccpm_migration().await? {
///     Some(path) => println!("Migrated to: {}", path.display()),
///     None => println!("No migration performed"),
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Unable to access current directory
/// - Unable to perform migration operations
pub async fn handle_legacy_ccpm_migration() -> Result<Option<PathBuf>> {
    let current_dir = std::env::current_dir()?;
    let legacy_dir = find_legacy_ccpm_directory(&current_dir);

    let Some(dir) = legacy_dir else {
        return Ok(None);
    };

    // Check if we're in an interactive terminal
    if !std::io::stdin().is_terminal() {
        // Non-interactive mode: Don't prompt, just inform and exit
        eprintln!("{}", "Legacy CCPM files detected (non-interactive mode).".yellow());
        eprintln!(
            "Run {} to migrate manually.",
            format!("agpm migrate --path {}", dir.display()).cyan()
        );
        return Ok(None);
    }

    // Found legacy files - prompt for migration
    let ccpm_toml = dir.join("ccpm.toml");
    let ccpm_lock = dir.join("ccpm.lock");

    let mut files = Vec::new();
    if ccpm_toml.exists() {
        files.push("ccpm.toml");
    }
    if ccpm_lock.exists() {
        files.push("ccpm.lock");
    }

    let files_str = files.join(" and ");

    println!("{}", "Legacy CCPM files detected!".yellow().bold());
    println!("{} {} found in {}", "→".cyan(), files_str, dir.display());
    println!();

    // Prompt user for migration
    print!("{} ", "Would you like to migrate to AGPM now? [Y/n]:".green());
    io::stdout().flush()?;

    // Use async I/O for proper integration with Tokio runtime
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    let response = response.trim().to_lowercase();

    if response.is_empty() || response == "y" || response == "yes" {
        println!();
        println!("{}", "🚀 Starting migration...".cyan());

        // Perform the migration with automatic installation
        let migrate_cmd = super::migrate::MigrateCommand::new(Some(dir.clone()), false, false);

        migrate_cmd.execute().await?;

        // Return the path to the newly created agpm.toml
        Ok(Some(dir.join("agpm.toml")))
    } else {
        println!();
        println!("{}", "Migration cancelled.".yellow());
        println!(
            "Run {} to migrate manually.",
            format!("agpm migrate --path {}", dir.display()).cyan()
        );
        Ok(None)
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
#[must_use]
pub fn check_for_legacy_ccpm_files() -> Option<String> {
    check_for_legacy_ccpm_files_from(std::env::current_dir().ok()?)
}

/// Find the directory containing legacy CCPM files.
///
/// Searches for ccpm.toml or ccpm.lock starting from the given directory
/// and walking up the directory tree.
///
/// # Returns
///
/// - `Some(PathBuf)` with the directory containing legacy files
/// - `None` if no legacy files are found
fn find_legacy_ccpm_directory(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir;

    loop {
        let ccpm_toml = dir.join("ccpm.toml");
        let ccpm_lock = dir.join("ccpm.lock");

        if ccpm_toml.exists() || ccpm_lock.exists() {
            return Some(dir.to_path_buf());
        }

        dir = dir.parent()?;
    }
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
                format!("agpm migrate --path {}", dir.display()).cyan().bold(),
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
        assert!(result.unwrap_err().to_string().contains("Failed to parse manifest"));
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

    #[test]
    fn test_find_legacy_ccpm_directory_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let result = find_legacy_ccpm_directory(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_find_legacy_ccpm_directory_in_current_dir() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("ccpm.toml"), "[sources]\n").unwrap();

        let result = find_legacy_ccpm_directory(temp_dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_find_legacy_ccpm_directory_in_parent() {
        let temp_dir = TempDir::new().unwrap();
        let parent = temp_dir.path();
        let child = parent.join("subdir");
        std::fs::create_dir(&child).unwrap();

        // Create legacy file in parent
        std::fs::write(parent.join("ccpm.toml"), "[sources]\n").unwrap();

        // Search from child directory
        let result = find_legacy_ccpm_directory(&child);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), parent);
    }

    #[test]
    fn test_find_legacy_ccpm_directory_finds_lock_file() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("ccpm.lock"), "# lock\n").unwrap();

        let result = find_legacy_ccpm_directory(temp_dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_handle_legacy_ccpm_migration_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory with no legacy files
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = handle_legacy_ccpm_migration().await;

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Note: Testing interactive behavior (user input) requires mocking stdin,
    // which is complex with tokio::io::stdin(). The non-interactive TTY check
    // will be automatically triggered in CI environments, providing implicit
    // integration testing.
}
