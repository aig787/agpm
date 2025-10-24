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
    /// Create a new command context from a manifest and project directory
    pub fn new(manifest: Manifest, project_dir: PathBuf) -> Result<Self> {
        let lockfile_path = project_dir.join("agpm.lock");
        Ok(Self {
            manifest,
            manifest_path: project_dir.join("agpm.toml"),
            project_dir,
            lockfile_path,
        })
    }

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

    /// Load an existing lockfile with automatic regeneration for invalid files
    ///
    /// If the lockfile exists but is invalid or corrupted, this method will
    /// offer to automatically regenerate it. This provides a better user
    /// experience by recovering from common lockfile issues.
    ///
    /// # Arguments
    ///
    /// * `can_regenerate` - Whether automatic regeneration should be offered
    /// * `operation_name` - Name of the operation for error messages (e.g., "list")
    ///
    /// # Returns
    ///
    /// * `Ok(Some(lockfile))` - Successfully loaded or regenerated lockfile
    /// * `Ok(None)` - No lockfile exists (not an error)
    /// * `Err` - Critical error that cannot be recovered from
    ///
    /// # Behavior
    ///
    /// - **Interactive mode** (TTY): Prompts user with Y/n confirmation
    /// - **Non-interactive mode** (CI/CD): Fails with helpful error message
    /// - **Backup strategy**: Copies invalid lockfile to `agpm.lock.invalid` before regeneration
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use anyhow::Result;
    /// # use agpm_cli::cli::common::CommandContext;
    /// # use agpm_cli::manifest::Manifest;
    /// # use std::path::PathBuf;
    /// # async fn example() -> Result<()> {
    /// let manifest = Manifest::load(&PathBuf::from("agpm.toml"))?;
    /// let project_dir = PathBuf::from(".");
    /// let ctx = CommandContext::new(manifest, project_dir)?;
    /// match ctx.load_lockfile_with_regeneration(true, "list") {
    ///     Ok(Some(lockfile)) => println!("Loaded lockfile"),
    ///     Ok(None) => println!("No lockfile found"),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_lockfile_with_regeneration(
        &self,
        can_regenerate: bool,
        operation_name: &str,
    ) -> Result<Option<crate::lockfile::LockFile>> {
        // If lockfile doesn't exist, that's not an error
        if !self.lockfile_path.exists() {
            return Ok(None);
        }

        // Try to load the lockfile
        match crate::lockfile::LockFile::load(&self.lockfile_path) {
            Ok(lockfile) => Ok(Some(lockfile)),
            Err(e) => {
                // Analyze the error to see if it's recoverable
                let error_msg = e.to_string();
                let can_auto_recover = can_regenerate
                    && (error_msg.contains("Invalid TOML syntax")
                        || error_msg.contains("Lockfile version")
                        || error_msg.contains("missing field")
                        || error_msg.contains("invalid type")
                        || error_msg.contains("expected"));

                if !can_auto_recover {
                    // Not a recoverable error, return the original error
                    return Err(e);
                }

                // This is a recoverable error, offer regeneration
                let backup_path = self.lockfile_path.with_extension("lock.invalid");

                // Create user-friendly message
                let regenerate_message = format!(
                    "The lockfile appears to be invalid or corrupted.\n\n\
                     Error: {}\n\n\
                     Note: The lockfile format is not yet stable as this is beta software.\n\n\
                     The invalid lockfile will be backed up to: {}",
                    error_msg,
                    backup_path.display()
                );

                // Check if we're in interactive mode
                if io::stdin().is_terminal() {
                    // Interactive mode: prompt user
                    println!("{}", regenerate_message);
                    print!("Would you like to regenerate the lockfile automatically? [Y/n] ");
                    io::stdout().flush().unwrap();

                    let mut input = String::new();
                    match io::stdin().read_line(&mut input) {
                        Ok(_) => {
                            let response = input.trim().to_lowercase();
                            if response.is_empty() || response == "y" || response == "yes" {
                                // User agreed to regenerate
                                self.backup_and_regenerate_lockfile(&backup_path, operation_name)?;
                                Ok(None) // Return None so caller creates new lockfile
                            } else {
                                // User declined, return the original error
                                Err(crate::core::AgpmError::InvalidLockfileError {
                                    file: self.lockfile_path.display().to_string(),
                                    reason: format!(
                                        "{} (User declined automatic regeneration)",
                                        error_msg
                                    ),
                                    can_regenerate: true,
                                }
                                .into())
                            }
                        }
                        Err(_) => {
                            // Failed to read input, fall back to non-interactive behavior
                            Err(self.create_non_interactive_error(&error_msg, operation_name))
                        }
                    }
                } else {
                    // Non-interactive mode: fail with helpful message
                    Err(self.create_non_interactive_error(&error_msg, operation_name))
                }
            }
        }
    }

    /// Backup the invalid lockfile and display regeneration instructions
    fn backup_and_regenerate_lockfile(
        &self,
        backup_path: &Path,
        operation_name: &str,
    ) -> Result<()> {
        // Backup the invalid lockfile
        if let Err(e) = std::fs::copy(&self.lockfile_path, backup_path) {
            eprintln!("Warning: Failed to backup invalid lockfile: {}", e);
        } else {
            println!("✓ Backed up invalid lockfile to: {}", backup_path.display());
        }

        // Remove the invalid lockfile
        if let Err(e) = std::fs::remove_file(&self.lockfile_path) {
            return Err(anyhow::anyhow!("Failed to remove invalid lockfile: {}", e));
        }

        println!("✓ Removed invalid lockfile");
        println!("Note: Run 'agpm install' to regenerate the lockfile");

        // If this is not an install command, suggest running install
        if operation_name != "install" {
            println!("Alternatively, run 'agpm {} --regenerate' if available", operation_name);
        }

        Ok(())
    }

    /// Create a non-interactive error message for CI/CD environments
    fn create_non_interactive_error(
        &self,
        error_msg: &str,
        _operation_name: &str,
    ) -> anyhow::Error {
        let backup_path = self.lockfile_path.with_extension("lock.invalid");

        crate::core::AgpmError::InvalidLockfileError {
            file: self.lockfile_path.display().to_string(),
            reason: format!(
                "{}\n\n\
                 To fix this issue:\n\
                 1. Backup the invalid lockfile: cp agpm.lock {}\n\
                 2. Remove the invalid lockfile: rm agpm.lock\n\
                 3. Regenerate it: agpm install\n\n\
                 Note: The lockfile format is not yet stable as this is beta software.",
                error_msg,
                backup_path.display()
            ),
            can_regenerate: true,
        }
        .into()
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

    #[cfg(test)]
    mod lockfile_regeneration_tests {
        use super::*;
        use crate::manifest::Manifest;
        use tempfile::TempDir;

        #[test]
        fn test_load_lockfile_with_regeneration_valid_lockfile() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");
            let lockfile_path = project_dir.join("agpm.lock");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"

[agents]
test = { source = "example", path = "test.md", version = "v1.0.0" }
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Create a valid lockfile
            let lockfile_content = r#"version = 1

[[sources]]
name = "example"
url = "https://github.com/example/repo.git"
commit = "abc123def456789012345678901234567890abcd"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "test"
source = "example"
path = "test.md"
version = "v1.0.0"
resolved_commit = "abc123def456789012345678901234567890abcd"
checksum = "sha256:examplechecksum"
installed_at = ".claude/agents/test.md"
"#;
            std::fs::write(&lockfile_path, lockfile_content).unwrap();

            // Test loading valid lockfile
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let result = ctx.load_lockfile_with_regeneration(true, "test").unwrap();
            assert!(result.is_some());
        }

        #[test]
        fn test_load_lockfile_with_regeneration_invalid_toml() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");
            let lockfile_path = project_dir.join("agpm.lock");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Create an invalid TOML lockfile
            std::fs::write(&lockfile_path, "invalid toml [[[").unwrap();

            // Test loading invalid lockfile in non-interactive mode
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            // This should return an error in non-interactive mode
            let result = ctx.load_lockfile_with_regeneration(true, "test");
            assert!(result.is_err());

            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Invalid or corrupted lockfile detected"));
            assert!(error_msg.contains("beta software"));
            assert!(error_msg.contains("cp agpm.lock"));
        }

        #[test]
        fn test_load_lockfile_with_regeneration_missing_lockfile() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Test loading non-existent lockfile
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let result = ctx.load_lockfile_with_regeneration(true, "test").unwrap();
            assert!(result.is_none()); // Should return None for missing lockfile
        }

        #[test]
        fn test_load_lockfile_with_regeneration_version_incompatibility() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");
            let lockfile_path = project_dir.join("agpm.lock");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Create a lockfile with future version
            let lockfile_content = r#"version = 999

[[sources]]
name = "example"
url = "https://github.com/example/repo.git"
commit = "abc123def456789012345678901234567890abcd"
fetched_at = "2024-01-01T00:00:00Z"
"#;
            std::fs::write(&lockfile_path, lockfile_content).unwrap();

            // Test loading future version lockfile
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let result = ctx.load_lockfile_with_regeneration(true, "test");
            assert!(result.is_err());

            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("version") || error_msg.contains("newer"));
        }

        #[test]
        fn test_load_lockfile_with_regeneration_cannot_regenerate() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");
            let lockfile_path = project_dir.join("agpm.lock");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Create an invalid TOML lockfile
            std::fs::write(&lockfile_path, "invalid toml [[[").unwrap();

            // Test with can_regenerate = false
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let result = ctx.load_lockfile_with_regeneration(false, "test");
            assert!(result.is_err());

            // Should return the original error, not the enhanced one
            let error_msg = result.unwrap_err().to_string();
            assert!(!error_msg.contains("Invalid or corrupted lockfile detected"));
            assert!(
                error_msg.contains("Failed to load lockfile")
                    || error_msg.contains("Invalid TOML syntax")
            );
        }

        #[test]
        fn test_backup_and_regenerate_lockfile() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");
            let lockfile_path = project_dir.join("agpm.lock");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Create an invalid lockfile
            std::fs::write(&lockfile_path, "invalid content").unwrap();

            // Test backup and regeneration
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let backup_path = lockfile_path.with_extension("lock.invalid");

            // This should backup the file and remove the original
            ctx.backup_and_regenerate_lockfile(&backup_path, "test").unwrap();

            // Check that backup was created
            assert!(backup_path.exists());
            assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), "invalid content");

            // Check that original was removed
            assert!(!lockfile_path.exists());
        }

        #[test]
        fn test_create_non_interactive_error() {
            let temp_dir = TempDir::new().unwrap();
            let project_dir = temp_dir.path();
            let manifest_path = project_dir.join("agpm.toml");

            // Create a minimal manifest
            let manifest_content = r#"[sources]
example = "https://github.com/example/repo.git"
"#;
            std::fs::write(&manifest_path, manifest_content).unwrap();

            // Test non-interactive error creation
            let manifest = Manifest::load(&manifest_path).unwrap();
            let ctx = CommandContext::new(manifest, project_dir.to_path_buf()).unwrap();

            let error = ctx.create_non_interactive_error("Invalid TOML syntax", "test");
            let error_msg = error.to_string();

            assert!(error_msg.contains("Invalid TOML syntax"));
            assert!(error_msg.contains("beta software"));
            assert!(error_msg.contains("cp agpm.lock"));
            assert!(error_msg.contains("rm agpm.lock"));
            assert!(error_msg.contains("agpm install"));
        }
    }

    // Note: Testing interactive behavior (user input) requires mocking stdin,
    // which is complex with tokio::io::stdin(). The non-interactive TTY check
    // will be automatically triggered in CI environments, providing implicit
    // integration testing.
}
