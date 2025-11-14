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

    /// Ensure we're on a specific branch, creating it if it doesn't exist
    /// This is useful when the default branch name is unknown (master vs main)
    pub fn ensure_branch(&self, branch_name: &str) -> Result<()> {
        // Try to checkout the branch first
        if self.checkout(branch_name).is_ok() {
            return Ok(());
        }

        // Branch doesn't exist, create it from current HEAD
        self.create_branch(branch_name)?;
        Ok(())
    }

    /// Return the repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Initialize a bare git repository
    pub fn init_bare(&self) -> Result<()> {
        self.run_git_command(&["init", "--bare"], "Failed to initialize bare git repository")?;
        Ok(())
    }

    /// Add a remote repository
    pub fn remote_add(&self, name: &str, url: &str) -> Result<()> {
        self.run_git_command(
            &["remote", "add", name, url],
            &format!("Failed to add remote: {}", name),
        )?;
        Ok(())
    }

    /// Fetch from remotes
    pub fn fetch(&self) -> Result<()> {
        self.run_git_command(&["fetch"], "Failed to fetch from remotes")?;
        Ok(())
    }

    /// Get current commit SHA
    pub fn rev_parse_head(&self) -> Result<String> {
        let output =
            self.run_git_command(&["rev-parse", "HEAD"], "Failed to get current commit SHA")?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Checkout a branch or commit
    pub fn checkout(&self, ref_name: &str) -> Result<()> {
        self.run_git_command(
            &["checkout", ref_name],
            &format!("Failed to checkout: {}", ref_name),
        )?;
        Ok(())
    }

    /// Create and checkout a branch
    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["checkout", "-b", branch_name],
            &format!("Failed to create branch: {}", branch_name),
        )?;
        Ok(())
    }

    /// Get current commit hash
    pub fn get_commit_hash(&self) -> Result<String> {
        let output = self.run_git_command(&["rev-parse", "HEAD"], "Failed to get commit hash")?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get HEAD SHA (alias for get_commit_hash for compatibility)
    pub fn get_head_sha(&self) -> Result<String> {
        self.get_commit_hash()
    }

    /// Get the current branch name
    pub fn get_current_branch(&self) -> Result<String> {
        let output = self
            .run_git_command(&["branch", "--show-current"], "Failed to get current branch name")?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the default branch name after initial commit
    /// This detects whether the default branch is "main", "master", or something else
    pub fn get_default_branch(&self) -> Result<String> {
        // Try to get current branch first
        if let Ok(branch) = self.get_current_branch() {
            if !branch.is_empty() {
                return Ok(branch);
            }
        }

        // Fallback for older git versions that don't support --show-current
        // Try common default branch names
        for branch in ["main", "master"] {
            if self.checkout(branch).is_ok() {
                return Ok(branch.to_string());
            }
        }

        // As a last resort, try to get it from git symbolic-ref
        let output = self.run_git_command(
            &["symbolic-ref", "--short", "HEAD"],
            "Failed to get default branch name from symbolic-ref",
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get porcelain status output
    pub fn status_porcelain(&self) -> Result<String> {
        let output =
            self.run_git_command(&["status", "--porcelain"], "Failed to get git status")?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Set HEAD to point to a branch (making it default branch)
    pub fn set_head(&self, branch_name: &str) -> Result<()> {
        self.run_git_command(
            &["symbolic-ref", "HEAD", &format!("refs/heads/{}", branch_name)],
            &format!("Failed to set HEAD to branch: {}", branch_name),
        )?;
        Ok(())
    }
}
