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

use crate::core::AgpmError;
use crate::utils::platform::get_git_command;

/// Type-safe builder for constructing and executing Git commands with consistent error handling.
///
/// This builder provides a fluent API for Git command construction that ensures
/// consistent behavior across AGPM's Git operations. It handles platform-specific
/// differences, timeout management, error context, and output capture in a unified way.
///
/// # Features
///
/// - **Fluent API**: Chainable methods for building commands
/// - **Error Context**: Automatic error message enhancement with context
/// - **Timeout Management**: Configurable timeouts with sensible defaults
/// - **Platform Independence**: Works across Windows, macOS, and Linux
/// - **Output Capture**: Flexible handling of command output and errors
/// - **Environment Variables**: Support for Git-specific environment settings
///
/// # Examples
///
/// ```rust,ignore
/// use agpm::git::command_builder::GitCommand;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Simple command with output capture
/// let output = GitCommand::new()
///     .args(["status", "--porcelain"])
///     .current_dir(Path::new("/path/to/repo"))
///     .execute()
///     .await?;
///
/// // Command with timeout and error context
/// let result = GitCommand::new()
///     .args(["clone", "https://github.com/example/repo.git"])
///     .timeout(std::time::Duration::from_secs(60))
///     .with_context("Cloning community repository")
///     .execute_success()
///     .await?;
///
/// // Interactive command (no output capture)
/// GitCommand::new()
///     .args(["merge", "--interactive"])
///     .inherit_stdio()
///     .execute_success()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// # Default Configuration
///
/// New commands are created with sensible defaults:
/// - **Timeout**: 5 minutes (300 seconds)
/// - **Output capture**: Enabled
/// - **Working directory**: Current process directory
/// - **Environment**: Inherits from parent process
///
/// # Platform Compatibility
///
/// The builder automatically handles platform-specific Git command location:
/// - **Windows**: Uses `git.exe` from PATH or common installation directories
/// - **Unix-like**: Uses `git` from PATH
/// - **Error handling**: Provides clear messages if Git is not installed
pub struct GitCommand {
    /// Command arguments to pass to Git (e.g., ["clone", "url", "path"])
    args: Vec<String>,

    /// Working directory for command execution (defaults to current directory)
    current_dir: Option<std::path::PathBuf>,

    /// Whether to capture command output (true) or inherit stdio (false)
    capture_output: bool,

    /// Environment variables to set for the Git process
    env_vars: Vec<(String, String)>,

    /// Maximum duration to wait for command completion (None = no timeout)
    timeout_duration: Option<Duration>,

    /// Optional context string for enhanced error messages
    context: Option<String>,

    /// For clone commands, store the URL for better error messages
    clone_url: Option<String>,
}

impl Default for GitCommand {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            clone_url: None,
            current_dir: None,
            capture_output: true,
            env_vars: Vec::new(),
            // Default timeout of 5 minutes for most git operations
            timeout_duration: Some(Duration::from_secs(300)),
            context: None,
        }
    }
}

impl GitCommand {
    /// Creates a new Git command builder with default settings.
    ///
    /// The new builder is initialized with:
    /// - Empty argument list
    /// - Current process directory as working directory
    /// - Output capture enabled
    /// - 5-minute timeout
    /// - No additional environment variables
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// let cmd = GitCommand::new()
    ///     .args(["status", "--short"])
    ///     .current_dir("/path/to/repo");
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the working directory for Git command execution.
    ///
    /// The command will be executed in the specified directory, which should
    /// typically be a Git repository root or subdirectory. If not set, the
    /// command executes in the current process working directory.
    ///
    /// # Arguments
    ///
    /// * `dir` - Path to the directory where the Git command should run
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::git::command_builder::GitCommand;
    /// use std::path::Path;
    ///
    /// let cmd = GitCommand::new()
    ///     .current_dir("/path/to/repository")
    ///     .args(["log", "--oneline"]);
    ///
    /// // Can also use Path references
    /// let repo_path = Path::new("/path/to/repo");
    /// let cmd2 = GitCommand::new()
    ///     .current_dir(repo_path)
    ///     .args(["status"]);
    /// ```
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Adds a single argument to the Git command.
    ///
    /// Arguments are passed to Git in the order they are added. This method
    /// is useful when building commands dynamically or when you need to add
    /// arguments conditionally.
    ///
    /// # Arguments
    ///
    /// * `arg` - The argument to add (will be converted to String)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// let cmd = GitCommand::new()
    ///     .arg("clone")
    ///     .arg("--depth")
    ///     .arg("1")
    ///     .arg("https://github.com/example/repo.git");
    /// ```
    ///
    /// # Note
    ///
    /// For adding multiple arguments at once, consider using [`args`](Self::args)
    /// which is more efficient for static argument lists.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple arguments to the Git command.
    ///
    /// This is the preferred method for adding multiple arguments at once,
    /// as it's more efficient than multiple calls to [`arg`](Self::arg).
    /// Arguments can be provided as any iterable of string-like types.
    ///
    /// # Arguments
    ///
    /// * `args` - An iterable of arguments to add to the command
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// // Using array literals
    /// let cmd = GitCommand::new()
    ///     .args(["clone", "--recursive", "https://github.com/example/repo.git"]);
    ///
    /// // Using vectors
    /// let clone_args = vec!["clone", "--depth", "1"];
    /// let cmd2 = GitCommand::new().args(clone_args);
    ///
    /// // Mixing with other methods
    /// let cmd3 = GitCommand::new()
    ///     .args(["fetch", "origin"])
    ///     .arg("--prune");
    /// ```
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Adds an environment variable for the Git command execution.
    ///
    /// Environment variables are useful for configuring Git behavior without
    /// modifying global Git configuration. Common use cases include setting
    /// credentials, configuration overrides, and locale settings.
    ///
    /// # Arguments
    ///
    /// * `key` - Environment variable name
    /// * `value` - Environment variable value
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// // Set Git configuration for this command only
    /// let cmd = GitCommand::new()
    ///     .args(["clone", "https://github.com/example/repo.git"])
    ///     .env("GIT_CONFIG_GLOBAL", "/dev/null")  // Ignore global config
    ///     .env("GIT_CONFIG_SYSTEM", "/dev/null"); // Ignore system config
    ///
    /// // Set locale for consistent output parsing
    /// let cmd2 = GitCommand::new()
    ///     .args(["log", "--oneline"])
    ///     .env("LC_ALL", "C");
    /// ```
    ///
    /// # Common Environment Variables
    ///
    /// - `GIT_DIR`: Override Git directory location
    /// - `GIT_WORK_TREE`: Override working tree location
    /// - `GIT_CONFIG_*`: Override configuration file locations
    /// - `LC_ALL`: Set locale for consistent output parsing
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Disables output capture, allowing the command to inherit parent stdio.
    ///
    /// By default, Git commands have their output captured for processing.
    /// This method disables capture, allowing the command to write directly
    /// to the terminal. This is useful for interactive commands or when you
    /// want to show Git output directly to the user.
    ///
    /// # Use Cases
    ///
    /// - Interactive Git commands (merge conflicts, rebase, etc.)
    /// - Commands where you want real-time output streaming
    /// - Debugging Git operations by seeing output immediately
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// // Interactive merge that may require user input
    /// # async fn example() -> anyhow::Result<()> {
    /// GitCommand::new()
    ///     .args(["merge", "feature-branch"])
    ///     .inherit_stdio()  // Allow user interaction
    ///     .execute_success()
    ///     .await?;
    ///
    /// // Show clone progress to user
    /// GitCommand::new()
    ///     .args(["clone", "--progress", "https://github.com/large/repo.git"])
    ///     .inherit_stdio()  // Show progress bars
    ///     .execute_success()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Note
    ///
    /// When stdio is inherited, you cannot use [`execute`](Self::execute) to
    /// capture output. Use [`execute_success`](Self::execute_success) instead.
    pub const fn inherit_stdio(mut self) -> Self {
        self.capture_output = false;
        self
    }

    /// Set a custom timeout for the command (None for no timeout)
    pub const fn with_timeout(mut self, duration: Option<Duration>) -> Self {
        self.timeout_duration = duration;
        self
    }

    /// Set a context for logging (e.g., dependency name)
    ///
    /// The context is included in debug log messages to help distinguish between
    /// concurrent operations when processing multiple dependencies in parallel.
    /// This is especially useful when using worktrees for parallel Git operations.
    ///
    /// # Arguments
    ///
    /// * `context` - A string identifier for this operation (typically dependency name)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let output = GitCommand::fetch()
    ///     .with_context("my-dependency")
    ///     .current_dir("/path/to/repo")
    ///     .execute()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Logging Output
    ///
    /// With context, log messages will include the context identifier:
    /// ```text
    /// (my-dependency) Executing command: git -C /path/to/repo fetch --all --tags
    /// (my-dependency) Command completed successfully
    /// ```
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Execute the command and return the output
    pub async fn execute(self) -> Result<GitCommandOutput> {
        let start = std::time::Instant::now();
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

        if let Some(ref ctx) = self.context {
            tracing::debug!(
                target: "git",
                "({}) Executing command: {} {}",
                ctx,
                git_command,
                full_args.join(" ")
            );
        } else {
            tracing::debug!(
                target: "git",
                "Executing command: {} {}",
                git_command,
                full_args.join(" ")
            );
        }

        for (key, value) in &self.env_vars {
            tracing::trace!(target: "git", "Setting env var: {}={}", key, value);
            cmd.env(key, value);
        }

        if self.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        }

        // Timeout is set but we don't need to log it every time

        let output_future = cmd.output();

        let output = if let Some(duration) = self.timeout_duration {
            if let Ok(result) = timeout(duration, output_future).await {
                tracing::trace!(target: "git", "Command completed within timeout");
                result.context(format!("Failed to execute git {}", full_args.join(" ")))?
            } else {
                tracing::warn!(
                    target: "git",
                    "Command timed out after {} seconds: git {}",
                    duration.as_secs(),
                    full_args.join(" ")
                );
                // Extract the actual git operation (skip -C and path if present)
                let git_operation =
                    if full_args.first() == Some(&"-C".to_string()) && full_args.len() > 2 {
                        full_args.get(2).cloned().unwrap_or_else(|| "unknown".to_string())
                    } else {
                        full_args.first().cloned().unwrap_or_else(|| "unknown".to_string())
                    };
                return Err(AgpmError::GitCommandError {
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
        } else {
            tracing::trace!(target: "git", "Executing command without timeout");
            output_future.await.context(format!("Failed to execute git {}", full_args.join(" ")))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            tracing::debug!(
                target: "git",
                "Command failed with exit code: {:?}",
                output.status.code()
            );
            if !stderr.is_empty() {
                tracing::debug!(target: "git", "Error: {}", stderr);
            }
            if !stdout.is_empty() && stderr.is_empty() {
                tracing::debug!(target: "git", "Error output: {}", stdout);
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
                // Use the URL we stored when building the command, not by parsing args
                let url = self.clone_url.unwrap_or_else(|| "unknown".to_string());
                AgpmError::GitCloneFailed {
                    url,
                    reason: stderr.to_string(),
                }
            } else if effective_args.first().is_some_and(|arg| arg == "checkout") {
                let reference = effective_args.get(1).cloned().unwrap_or_default();
                AgpmError::GitCheckoutFailed {
                    reference,
                    reason: stderr.to_string(),
                }
            } else if effective_args.first().is_some_and(|arg| arg == "worktree") {
                let subcommand = effective_args.get(1).cloned().unwrap_or_default();
                AgpmError::GitCommandError {
                    operation: format!("worktree {subcommand}"),
                    stderr: if stderr.is_empty() {
                        stdout.to_string()
                    } else {
                        stderr.to_string()
                    },
                }
            } else {
                AgpmError::GitCommandError {
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

        // Log stdout/stderr without prefixes if they're not empty
        if !stdout.is_empty() {
            if let Some(ref ctx) = self.context {
                tracing::debug!(target: "git", "({}) {}", ctx, stdout.trim());
            } else {
                tracing::debug!(target: "git", "{}", stdout.trim());
            }
        }
        if !stderr.is_empty() {
            if let Some(ref ctx) = self.context {
                tracing::debug!(target: "git", "({}) {}", ctx, stderr.trim());
            } else {
                tracing::debug!(target: "git", "{}", stderr.trim());
            }
        }

        let elapsed = start.elapsed();

        // Log performance for expensive operations
        if elapsed.as_secs() > 1 {
            let operation = if full_args.first() == Some(&"-C".to_string()) && full_args.len() > 2 {
                full_args.get(2).cloned().unwrap_or_else(|| "unknown".to_string())
            } else {
                full_args.first().cloned().unwrap_or_else(|| "unknown".to_string())
            };

            if let Some(ref ctx) = self.context {
                tracing::info!(target: "git::perf", "({}) Git {} took {:.2}s", ctx, operation, elapsed.as_secs_f64());
            } else {
                tracing::info!(target: "git::perf", "Git {} took {:.2}s", operation, elapsed.as_secs_f64());
            }
        } else if elapsed.as_millis() > 100 {
            // Log debug for moderately slow operations
            let operation = if full_args.first() == Some(&"-C".to_string()) && full_args.len() > 2 {
                full_args.get(2).cloned().unwrap_or_else(|| "unknown".to_string())
            } else {
                full_args.first().cloned().unwrap_or_else(|| "unknown".to_string())
            };

            if let Some(ref ctx) = self.context {
                tracing::debug!(target: "git::perf", "({}) Git {} took {}ms", ctx, operation, elapsed.as_millis());
            } else {
                tracing::debug!(target: "git::perf", "Git {} took {}ms", operation, elapsed.as_millis());
            }
        }

        Ok(GitCommandOutput {
            stdout,
            stderr,
        })
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
        cmd.args.push("--filter=blob:none".to_string());
        cmd.args.push("--recurse-submodules".to_string());
        cmd.args.push(url.to_string());
        cmd.args.push(target.as_ref().display().to_string());
        cmd.clone_url = Some(url.to_string());
        cmd
    }

    /// Create a clone command with specific depth
    ///
    /// Create a fetch command
    pub fn fetch() -> Self {
        // Use --all to fetch from all remotes and --tags to get tags
        // For bare repositories, we need to ensure remote tracking branches are created
        Self::new().args(["fetch", "--all", "--tags", "--force"])
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

    /// Create a git clone --bare command for bare repository.
    ///
    /// Bare repositories are optimized for use as a source for worktrees,
    /// allowing multiple concurrent checkouts without conflicts. This is
    /// the preferred method for parallel operations in AGPM.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL to clone
    /// * `target` - The local directory where the bare repository will be stored
    ///
    /// # Returns
    ///
    /// A configured `GitCommand` ready for execution.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// GitCommand::clone_bare(
    ///     "https://github.com/example/repo.git",
    ///     "/tmp/repo.git"
    /// )
    /// .execute_success()
    /// .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Worktree Usage
    ///
    /// Bare repositories created with this command are designed to be used
    /// with [`worktree_add`](#method.worktree_add) for parallel operations:
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn worktree_example() -> anyhow::Result<()> {
    /// // Clone bare repository
    /// GitCommand::clone_bare("https://github.com/example/repo.git", "/tmp/repo.git")
    ///     .execute_success()
    ///     .await?;
    ///
    /// // Create working directory from bare repo
    /// GitCommand::worktree_add("/tmp/work1", Some("v1.0.0"))
    ///     .current_dir("/tmp/repo.git")
    ///     .execute_success()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn clone_bare(url: &str, target: impl AsRef<Path>) -> Self {
        let mut cmd = Self::new();
        let mut args = vec!["clone".to_string(), "--bare".to_string(), "--progress".to_string()];

        // Only use partial clone for remote repositories
        // Local repositories (file://, absolute paths, relative paths) need full clones
        // to work properly with worktrees, especially when they're bare repositories
        let is_local = url.starts_with("file://")
            || url.starts_with('/')
            || url.starts_with('.')
            || url.starts_with('~')
            || (url.len() > 1 && url.chars().nth(1) == Some(':')); // Windows paths like C:

        if !is_local {
            args.push("--filter=blob:none".to_string());
        }

        args.extend(vec![
            "--recurse-submodules".to_string(),
            url.to_string(),
            target.as_ref().display().to_string(),
        ]);

        cmd.args.extend(args);
        cmd.clone_url = Some(url.to_string());
        cmd
    }

    /// Create a worktree add command for parallel-safe Git operations.
    ///
    /// Worktrees allow multiple working directories to be checked out from
    /// a single bare repository, enabling safe parallel operations on different
    /// versions of the same repository without conflicts.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path where the new worktree will be created
    /// * `reference` - Optional Git reference (branch, tag, or commit) to checkout
    ///
    /// # Returns
    ///
    /// A configured `GitCommand` that must be executed from a bare repository directory.
    ///
    /// # Parallel Safety
    ///
    /// Each worktree is independent and can be safely accessed concurrently:
    /// - Different dependencies can use different worktrees simultaneously
    /// - No conflicts between parallel checkout operations
    /// - Each worktree maintains its own working directory state
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Create worktree with specific version
    /// GitCommand::worktree_add("/tmp/work-v1", Some("v1.0.0"))
    ///     .current_dir("/tmp/bare-repo.git")
    ///     .execute_success()
    ///     .await?;
    ///
    /// // Create worktree with default branch
    /// GitCommand::worktree_add("/tmp/work-main", None)
    ///     .current_dir("/tmp/bare-repo.git")
    ///     .execute_success()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Concurrency Control
    ///
    /// AGPM uses a global semaphore to limit concurrent Git operations and
    /// prevent resource exhaustion. This is handled automatically by the
    /// cache layer when using worktrees for parallel installations.
    ///
    /// # Reference Types
    ///
    /// The `reference` parameter supports various Git reference types:
    /// - **Tags**: `"v1.0.0"` (most common for package versions)
    /// - **Branches**: `"main"`, `"develop"`
    /// - **Commits**: `"abc123"` (specific commit hashes)
    /// - **None**: Uses repository's default branch
    pub fn worktree_add(worktree_path: impl AsRef<Path>, reference: Option<&str>) -> Self {
        let mut cmd = Self::new();
        cmd.args.push("worktree".to_string());
        cmd.args.push("add".to_string());

        // Add the worktree path
        cmd.args.push(worktree_path.as_ref().display().to_string());

        // Add the reference if provided
        if let Some(ref_name) = reference {
            cmd.args.push(ref_name.to_string());
        }

        cmd
    }

    /// Remove a worktree and clean up associated files.
    ///
    /// This command removes a worktree that was created with [`worktree_add`]
    /// and cleans up Git's internal bookkeeping. The `--force` flag is used
    /// to ensure removal even if the worktree has local modifications.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path to the worktree to remove
    ///
    /// # Returns
    ///
    /// A configured `GitCommand` that must be executed from the bare repository.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Remove a worktree
    /// GitCommand::worktree_remove("/tmp/work-v1")
    ///     .current_dir("/tmp/bare-repo.git")
    ///     .execute_success()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Force Removal
    ///
    /// The command uses `--force` to ensure removal succeeds even when:
    /// - The worktree has uncommitted changes
    /// - Files are locked or in use
    /// - The worktree directory structure has been modified
    ///
    /// This is appropriate for AGPM's use case where worktrees are temporary
    /// and any local changes should be discarded.
    ///
    /// [`worktree_add`]: #method.worktree_add
    pub fn worktree_remove(worktree_path: impl AsRef<Path>) -> Self {
        Self::new().args([
            "worktree",
            "remove",
            "--force",
            &worktree_path.as_ref().display().to_string(),
        ])
    }

    /// List all worktrees associated with a repository.
    ///
    /// This command returns information about all worktrees linked to the
    /// current bare repository. The `--porcelain` flag provides machine-readable
    /// output that's easier to parse programmatically.
    ///
    /// # Returns
    ///
    /// A configured `GitCommand` that must be executed from a bare repository.
    ///
    /// # Output Format
    ///
    /// The porcelain output format provides structured information:
    /// ```text
    /// worktree /path/to/worktree1
    /// HEAD abc123def456...
    /// branch refs/heads/main
    ///
    /// worktree /path/to/worktree2
    /// HEAD def456abc123...
    /// detached
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let output = GitCommand::worktree_list()
    ///     .current_dir("/tmp/bare-repo.git")
    ///     .execute_stdout()
    ///     .await?;
    ///
    /// // Parse output to find worktree paths
    /// for line in output.lines() {
    ///     if line.starts_with("worktree ") {
    ///         let path = line.strip_prefix("worktree ").unwrap();
    ///         println!("Found worktree: {}", path);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn worktree_list() -> Self {
        Self::new().args(["worktree", "list", "--porcelain"])
    }

    /// Prune stale worktree administrative files.
    ///
    /// This command cleans up worktree entries that no longer have corresponding
    /// directories on disk. It's useful for maintenance after worktrees have been
    /// manually deleted or when cleaning up after failed operations.
    ///
    /// # Returns
    ///
    /// A configured `GitCommand` that must be executed from a bare repository.
    ///
    /// # When to Use
    ///
    /// Prune worktrees when:
    /// - Worktree directories have been manually deleted
    /// - After bulk cleanup operations
    /// - During cache maintenance
    /// - When Git reports stale worktree references
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::git::command_builder::GitCommand;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Clean up stale worktree references
    /// GitCommand::worktree_prune()
    ///     .current_dir("/tmp/bare-repo.git")
    ///     .execute_success()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// This is a lightweight operation that only updates Git's internal
    /// bookkeeping. It doesn't remove actual worktree directories.
    pub fn worktree_prune() -> Self {
        Self::new().args(["worktree", "prune"])
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

    #[tokio::test]
    async fn test_git_command_logging() {
        // This test verifies that git stdout/stderr are logged at debug level
        // Run with RUST_LOG=debug to see the output
        let result = GitCommand::new().args(["--version"]).execute().await;

        assert!(result.is_ok(), "Git --version should succeed");
        let output = result.unwrap();
        assert!(!output.stdout.is_empty(), "Git version should produce stdout");
        // When run with RUST_LOG=debug, this should produce:
        // - "Executing git command: git --version"
        // - "Git command completed successfully"
        // - "Git stdout (raw): git version X.Y.Z"
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
        assert!(cmd.args.contains(&"https://example.com/repo.git".to_string()));
    }

    #[test]
    fn test_checkout_branch_builder() {
        let cmd = GitCommand::checkout_branch("main", "origin/main");
        assert_eq!(cmd.args, vec!["checkout", "-B", "main", "origin/main"]);
    }
}
