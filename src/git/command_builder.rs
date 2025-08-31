//! Type-safe Git command builder for consistent command execution
//!
//! This module provides a fluent API for building and executing Git commands,
//! eliminating duplication and ensuring consistent error handling across the codebase.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, trace, warn};

use crate::core::CcpmError;
use crate::utils::platform::get_git_command;

/// Builder for constructing and executing Git commands with consistent error handling
pub struct GitCommand {
    args: Vec<String>,
    current_dir: Option<std::path::PathBuf>,
    capture_output: bool,
    env_vars: Vec<(String, String)>,
    timeout_duration: Option<Duration>,
}

impl Default for GitCommand {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            current_dir: None,
            capture_output: true,
            env_vars: Vec::new(),
            // Default timeout of 5 minutes for most git operations
            timeout_duration: Some(Duration::from_secs(300)),
        }
    }
}

impl GitCommand {
    /// Create a new Git command builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the working directory for the command
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Add an argument to the command
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments to the command
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Add an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Disable output capture (for interactive commands)
    pub fn inherit_stdio(mut self) -> Self {
        self.capture_output = false;
        self
    }

    /// Set a custom timeout for the command (None for no timeout)
    pub fn with_timeout(mut self, duration: Option<Duration>) -> Self {
        self.timeout_duration = duration;
        self
    }

    /// Execute the command and return the output
    pub async fn execute(self) -> Result<GitCommandOutput> {
        let git_command = get_git_command();
        let mut cmd = Command::new(git_command);

        // Build the full arguments list including -C flag if needed
        let mut full_args = Vec::new();
        if let Some(ref dir) = self.current_dir {
            // Use -C flag to specify working directory
            // This makes git operations independent of the process's current directory
            full_args.push("-C".to_string());
            // Use the path as-is to avoid symlink resolution issues on macOS
            // (e.g., /var vs /private/var)
            full_args.push(dir.display().to_string());
        }
        full_args.extend(self.args.clone());

        cmd.args(&full_args);

        let working_dir = self.current_dir.clone();

        debug!(
            "Executing git command: {} {}",
            git_command,
            full_args.join(" ")
        );

        if let Some(ref dir) = working_dir {
            debug!("Working directory (via -C flag): {}", dir.display());
        }

        for (key, value) in &self.env_vars {
            trace!("Setting env var: {}={}", key, value);
            cmd.env(key, value);
        }

        if self.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        }

        if let Some(duration) = self.timeout_duration {
            debug!("Command timeout set to {} seconds", duration.as_secs());
        }

        let output_future = cmd.output();

        let output = match self.timeout_duration {
            Some(duration) => match timeout(duration, output_future).await {
                Ok(result) => {
                    trace!("Git command completed within timeout");
                    result.context(format!("Failed to execute git {}", full_args.join(" ")))?
                }
                Err(_) => {
                    warn!(
                        "Git command timed out after {} seconds: git {}",
                        duration.as_secs(),
                        full_args.join(" ")
                    );
                    // Extract the actual git operation (skip -C and path if present)
                    let git_operation =
                        if full_args.first() == Some(&"-C".to_string()) && full_args.len() > 2 {
                            full_args
                                .get(2)
                                .cloned()
                                .unwrap_or_else(|| "unknown".to_string())
                        } else {
                            full_args
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "unknown".to_string())
                        };
                    return Err(CcpmError::GitCommandError {
                        operation: git_operation,
                        stderr: format!(
                            "Git command timed out after {} seconds. This may indicate:\n\
                                - Network connectivity issues\n\
                                - Authentication prompts waiting for input\n\
                                - Large repository operations taking too long\n\
                                Try running the command manually: git {}",
                            duration.as_secs(),
                            full_args.join(" ")
                        ),
                    }
                    .into());
                }
            },
            None => {
                trace!("Executing git command without timeout");
                output_future
                    .await
                    .context(format!("Failed to execute git {}", full_args.join(" ")))?
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            debug!(
                "Git command failed with exit code: {:?}",
                output.status.code()
            );
            if !stderr.is_empty() {
                debug!("Git stderr: {}", stderr);
            }
            if !stdout.is_empty() {
                trace!("Git stdout: {}", stdout);
            }

            // Provide context-specific error messages
            // Skip -C flag arguments when checking command type
            let args_start = if full_args.first() == Some(&"-C".to_string()) && full_args.len() > 2
            {
                2
            } else {
                0
            };
            let effective_args = &full_args[args_start..];

            let error = if effective_args.first().is_some_and(|arg| arg == "clone") {
                let url = effective_args.get(2).cloned().unwrap_or_default();
                CcpmError::GitCloneFailed {
                    url,
                    reason: stderr.to_string(),
                }
            } else if effective_args.first().is_some_and(|arg| arg == "checkout") {
                let reference = effective_args.get(1).cloned().unwrap_or_default();
                CcpmError::GitCheckoutFailed {
                    reference,
                    reason: stderr.to_string(),
                }
            } else {
                CcpmError::GitCommandError {
                    operation: effective_args
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                    stderr: stderr.to_string(),
                }
            };

            return Err(error.into());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        debug!("Git command completed successfully");
        if !stdout.is_empty() {
            trace!("Git stdout: {}", stdout.trim());
        }
        if !stderr.is_empty() {
            trace!("Git stderr: {}", stderr.trim());
        }

        Ok(GitCommandOutput { stdout, stderr })
    }

    /// Execute the command and return only stdout as a trimmed string
    pub async fn execute_stdout(self) -> Result<String> {
        let output = self.execute().await?;
        Ok(output.stdout.trim().to_string())
    }

    /// Execute the command and check for success without capturing output
    pub async fn execute_success(self) -> Result<()> {
        self.execute().await?;
        Ok(())
    }
}

/// Output from a Git command
pub struct GitCommandOutput {
    /// Standard output from the Git command
    pub stdout: String,
    /// Standard error output from the Git command
    pub stderr: String,
}

// Convenience builders for common Git operations

impl GitCommand {
    /// Create a clone command
    pub fn clone(url: &str, target: impl AsRef<Path>) -> Self {
        let mut cmd = Self::new();
        cmd.args.push("clone".to_string());
        cmd.args.push("--progress".to_string());
        cmd.args.push(url.to_string());
        cmd.args.push(target.as_ref().display().to_string());
        cmd
    }

    /// Create a clone command with specific depth
    pub fn shallow_clone(url: &str, target: impl AsRef<Path>, depth: u32) -> Self {
        let mut cmd = Self::new();
        cmd.args.extend(vec![
            "clone".to_string(),
            "--progress".to_string(),
            "--depth".to_string(),
            depth.to_string(),
            url.to_string(),
            target.as_ref().display().to_string(),
        ]);
        cmd
    }

    /// Create a fetch command
    pub fn fetch() -> Self {
        Self::new().args(["fetch", "--all", "--tags"])
    }

    /// Create a checkout command
    pub fn checkout(ref_name: &str) -> Self {
        Self::new().args(["checkout", ref_name])
    }

    /// Create a checkout command that forces branch creation/update
    pub fn checkout_branch(branch_name: &str, remote_ref: &str) -> Self {
        Self::new().args(["checkout", "-B", branch_name, remote_ref])
    }

    /// Create a reset command
    pub fn reset_hard() -> Self {
        Self::new().args(["reset", "--hard", "HEAD"])
    }

    /// Create a tag list command
    pub fn list_tags() -> Self {
        Self::new().args(["tag", "-l"])
    }

    /// Create a branch list command
    pub fn list_branches() -> Self {
        Self::new().args(["branch", "-r"])
    }

    /// Create a rev-parse command
    pub fn rev_parse(ref_name: &str) -> Self {
        Self::new().args(["rev-parse", ref_name])
    }

    /// Create a command to get the current commit hash
    pub fn current_commit() -> Self {
        Self::new().args(["rev-parse", "HEAD"])
    }

    /// Create a command to get the remote URL
    pub fn remote_url() -> Self {
        Self::new().args(["remote", "get-url", "origin"])
    }

    /// Create a command to set the remote URL
    pub fn set_remote_url(url: &str) -> Self {
        Self::new().args(["remote", "set-url", "origin", url])
    }

    /// Create a ls-remote command for repository verification
    pub fn ls_remote(url: &str) -> Self {
        Self::new().args(["ls-remote", "--heads", url])
    }

    /// Create a command to verify a reference exists
    pub fn verify_ref(ref_name: &str) -> Self {
        Self::new().args(["rev-parse", "--verify", ref_name])
    }

    /// Create a command to get the current branch
    pub fn current_branch() -> Self {
        Self::new().args(["branch", "--show-current"])
    }

    /// Create an init command
    pub fn init() -> Self {
        Self::new().arg("init")
    }

    /// Create an add command
    pub fn add(pathspec: &str) -> Self {
        Self::new().args(["add", pathspec])
    }

    /// Create a commit command
    pub fn commit(message: &str) -> Self {
        Self::new().args(["commit", "-m", message])
    }

    /// Create a push command
    pub fn push() -> Self {
        Self::new().arg("push")
    }

    /// Create a status command
    pub fn status() -> Self {
        Self::new().arg("status")
    }

    /// Create a diff command
    pub fn diff() -> Self {
        Self::new().arg("diff")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_builder_basic() {
        let cmd = GitCommand::new().arg("status").arg("--short");
        assert_eq!(cmd.args, vec!["status", "--short"]);
    }

    #[test]
    fn test_command_builder_with_dir() {
        let cmd = GitCommand::new().current_dir("/tmp/repo").arg("status");
        assert_eq!(cmd.current_dir, Some(std::path::PathBuf::from("/tmp/repo")));
    }

    #[test]
    fn test_clone_builder() {
        let cmd = GitCommand::clone("https://example.com/repo.git", "/tmp/target");
        assert_eq!(cmd.args[0], "clone");
        assert_eq!(cmd.args[1], "--progress");
        assert!(cmd
            .args
            .contains(&"https://example.com/repo.git".to_string()));
    }

    #[test]
    fn test_shallow_clone_builder() {
        let cmd = GitCommand::shallow_clone("https://example.com/repo.git", "/tmp/target", 1);
        assert!(cmd.args.contains(&"--depth".to_string()));
        assert!(cmd.args.contains(&"1".to_string()));
    }

    #[test]
    fn test_checkout_branch_builder() {
        let cmd = GitCommand::checkout_branch("main", "origin/main");
        assert_eq!(cmd.args, vec!["checkout", "-B", "main", "origin/main"]);
    }
}
