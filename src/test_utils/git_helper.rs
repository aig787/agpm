//! Git test helper utilities
//!
//! Provides a safe, testable wrapper around Git operations for unit tests.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git command builder for tests
///
/// Provides a safe wrapper around git commands with proper error handling
/// and test isolation. Use this instead of raw `std::process::Command` for
/// git operations in tests.
pub struct TestGit {
    repo_path: PathBuf,
}

impl TestGit {
    fn run_git_command(&self, args: &[&str], action: &str) -> Result<std::process::Output> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .with_context(|| action.to_string())?;

        if !output.status.success() {
            bail!("{} failed: {}", action, String::from_utf8_lossy(&output.stderr));
        }

        Ok(output)
    }

    /// Create a new TestGit instance for the given repository path
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Initialize a new git repository
    pub fn init(&self) -> Result<()> {
        self.run_git_command(&["init"], "Failed to initialize git repository")?;
        Ok(())
    }

    /// Configure git user for tests
    pub fn config_user(&self) -> Result<()> {
        self.run_git_command(
            &["config", "user.email", "test@agpm.example"],
            "Failed to configure git user email",
        )?;

        self.run_git_command(
            &["config", "user.name", "Test User"],
            "Failed to configure git user name",
        )?;
        Ok(())
    }

    /// Add all files to staging
    pub fn add_all(&self) -> Result<()> {
        self.run_git_command(&["add", "."], "Failed to add files to git")?;
        Ok(())
    }

    /// Create a commit with the given message
    pub fn commit(&self, message: &str) -> Result<()> {
        self.run_git_command(&["commit", "-m", message], "Failed to create git commit")?;
        Ok(())
    }

    /// Create a tag
    pub fn tag(&self, tag_name: &str) -> Result<()> {
        self.run_git_command(&["tag", tag_name], &format!("Failed to create tag: {}", tag_name))?;
        Ok(())
    }

    /// Ensure the current branch has the given name
    pub fn ensure_branch(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["branch", "-M", branch_name],
            &format!("Failed to rename branch to {}", branch_name),
        )?;
        Ok(())
    }

    /// Return the repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}
