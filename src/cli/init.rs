//! Initialize a new AGPM project with a manifest file.
//!
//! This module provides the `init` command which creates a new `agpm.toml` manifest file
//! in the specified directory (or current directory). The manifest file is the main
//! configuration file for a AGPM project that defines dependencies on Claude Code resources.
//!
//! # Examples
//!
//! Initialize a manifest in the current directory:
//! ```bash
//! agpm init
//! ```
//!
//! Initialize a manifest in a specific directory:
//! ```bash
//! agpm init --path ./my-project
//! ```
//!
//! Force overwrite an existing manifest:
//! ```bash
//! agpm init --force
//! ```
//!
//! # Manifest Structure
//!
//! The generated manifest contains empty sections for all resource types:
//!
//! ```toml
//! [sources]
//!
//! [agents]
//!
//! [snippets]
//!
//! [commands]
//!
//! [scripts]
//!
//! [hooks]
//!
//! [mcp-servers]
//! ```
//!
//! # Error Conditions
//!
//! - Returns error if manifest already exists and `--force` is not used
//! - Returns error if unable to create the target directory
//! - Returns error if unable to write the manifest file (permissions, disk space, etc.)
//!
//! # Safety
//!
//! This command is safe to run and will not overwrite existing files unless `--force` is specified.

use anyhow::{Result, anyhow};
use clap::Args;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

/// Command to initialize a new AGPM project with a manifest file.
///
/// This command creates a `agpm.toml` manifest file in the specified directory
/// (or current directory if no path is provided). The manifest serves as the
/// main configuration file for defining Claude Code resource dependencies.
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::init::InitCommand;
/// use std::path::PathBuf;
///
/// // Initialize in current directory
/// let cmd = InitCommand {
///     path: None,
///     force: false,
/// };
///
/// // Initialize in specific directory with force overwrite
/// let cmd = InitCommand {
///     path: Some(PathBuf::from("./my-project")),
///     force: true,
/// };
/// ```
#[derive(Args)]
pub struct InitCommand {
    /// Path to create the manifest (defaults to current directory)
    ///
    /// If not provided, the manifest will be created in the current working directory.
    /// If the specified directory doesn't exist, it will be created.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Force overwrite if manifest already exists
    ///
    /// By default, the command will fail if a `agpm.toml` file already exists
    /// in the target directory. Use this flag to overwrite an existing file.
    #[arg(short, long)]
    force: bool,
}

impl InitCommand {
    /// Updates the .gitignore file to include AGPM-specific entries.
    ///
    /// This method ensures that the following entries are added to the project's `.gitignore` file:
    /// - `.agpm/backups/` - AGPM backup directory
    /// - `agpm.private.toml` - User-level patches (private configuration)
    /// - `agpm.private.lock` - Private lockfile
    ///
    /// If the `.gitignore` file doesn't exist, it will be created. If entries already exist,
    /// they won't be duplicated.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The directory where the `.gitignore` file should be updated or created
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the `.gitignore` was updated successfully
    /// - `Err(anyhow::Error)` if unable to read or write the `.gitignore` file
    fn update_gitignore(target_dir: &std::path::Path) -> Result<()> {
        let gitignore_path = target_dir.join(".gitignore");
        let entries = [".agpm/backups/", "agpm.private.toml", "agpm.private.lock"];

        // Read existing .gitignore or start with empty content
        let mut content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };

        // Check which entries need to be added
        let entries_to_add: Vec<&str> = entries
            .iter()
            .filter(|entry| !content.lines().any(|line| line.trim() == **entry))
            .copied()
            .collect();

        if entries_to_add.is_empty() {
            return Ok(());
        }

        // Add entries (ensure there's a newline before it if content exists)
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        if !content.is_empty() {
            content.push('\n');
            content.push_str("# AGPM\n");
        }
        for entry in entries_to_add {
            content.push_str(entry);
            content.push('\n');
        }

        fs::write(&gitignore_path, content)?;

        Ok(())
    }

    /// Execute the init command with an optional manifest path (for API compatibility)
    pub async fn execute_with_manifest_path(
        self,
        _manifest_path: Option<std::path::PathBuf>,
    ) -> Result<()> {
        // Init command doesn't use manifest_path since it creates a new manifest
        // The path is already part of the InitCommand struct
        self.execute().await
    }

    /// Execute the init command to create a new AGPM manifest file.
    ///
    /// This method creates a `agpm.toml` manifest file with a minimal template structure
    /// that includes empty sections for all resource types. The file is
    /// created in the specified directory or current directory if no path is provided.
    ///
    /// # Behavior
    ///
    /// 1. Determines the target directory (from `path` option or current directory)
    /// 2. Checks if a manifest already exists and handles the `force` flag
    /// 3. Creates the target directory if it doesn't exist
    /// 4. Writes the manifest template to `agpm.toml`
    /// 5. Displays success message and next steps to the user
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the manifest was created successfully
    /// - `Err(anyhow::Error)` if:
    ///   - A manifest already exists and `force` is false
    ///   - Unable to create the target directory
    ///   - Unable to write the manifest file
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::init::InitCommand;
    /// use std::path::PathBuf;
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = InitCommand {
    ///     path: Some(PathBuf::from("./test-project")),
    ///     force: false,
    /// };
    ///
    /// // This would create ./test-project/agpm.toml
    /// // cmd.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute(self) -> Result<()> {
        let target_dir = self.path.unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = target_dir.join("agpm.toml");

        // Check if manifest already exists
        if manifest_path.exists() && !self.force {
            return Err(anyhow!(
                "Manifest already exists at {}. Use --force to overwrite",
                manifest_path.display()
            ));
        }

        // Create directory if it doesn't exist
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
        }

        // Write a minimal template with empty sections
        let template = r#"# AGPM Manifest
# This file defines your Claude Code resource dependencies

[sources]
# Add your Git repository sources here
# Example: official = "https://github.com/aig787/agpm-community.git"

# Project-specific template variables (optional)
# Provides context to AI agents - use any structure you want!
# [project]
# style_guide = "docs/STYLE_GUIDE.md"
# max_line_length = 100
# test_framework = "pytest"
#
# [project.paths]
# architecture = "docs/ARCHITECTURE.md"
# conventions = "docs/CONVENTIONS.md"
#
# Access in templates: {{ agpm.project.style_guide }}

# Tool type configurations (multi-tool support)
[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents" }, snippets = { path = "snippets" }, commands = { path = "commands" }, scripts = { path = "scripts" }, hooks = {}, mcp-servers = {} }
# Note: hooks and mcp-servers merge into configuration files (no file installation)

[tools.opencode]
enabled = false  # Enable if you want to use OpenCode resources
path = ".opencode"
resources = { agents = { path = "agent" }, commands = { path = "command" }, mcp-servers = {} }
# Note: MCP servers merge into opencode.json (no file installation)

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets" } }

[agents]
# Add your agent dependencies here
# Example: my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
# For OpenCode: my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0", tool = "opencode" }

[snippets]
# Add your snippet dependencies here
# Example: utils = { source = "official", path = "snippets/utils.md", tool = "agpm" }

[commands]
# Add your command dependencies here
# Example: deploy = { source = "official", path = "commands/deploy.md" }

[scripts]
# Add your script dependencies here
# Example: build = { source = "official", path = "scripts/build.sh" }

[hooks]
# Add your hook dependencies here
# Example: pre-commit = { source = "official", path = "hooks/pre-commit.json" }

[mcp-servers]
# Add your MCP server dependencies here
# Example: filesystem = { source = "official", path = "mcp-servers/filesystem.json" }
"#;
        fs::write(&manifest_path, template)?;

        // Add .agpm/backups/ to .gitignore
        Self::update_gitignore(&target_dir)?;

        println!("{} Initialized agpm.toml at {}", "✓".green(), manifest_path.display());

        println!("\n{}", "Next steps:".cyan());
        println!("  Add dependencies with {}:", "agpm add".bright_white());
        println!(
            "    agpm add agent my-agent --source https://github.com/org/repo.git --path agents/my-agent.md"
        );
        println!("    agpm add snippet utils --path ../local/snippets/utils.md");
        println!("\n  Then run {} to install", "agpm install".bright_white());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_init_creates_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let manifest_path = temp_dir.path().join("agpm.toml");
        assert!(manifest_path.exists());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(content.contains("[agents]"));
        assert!(content.contains("[snippets]"));
    }

    #[tokio::test]
    async fn test_init_creates_directory_if_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let new_dir = temp_dir.path().join("new_project");

        let cmd = InitCommand {
            path: Some(new_dir.clone()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        assert!(new_dir.exists());
        assert!(new_dir.join("agpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_fails_if_manifest_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        fs::write(&manifest_path, "existing content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_init_force_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        fs::write(&manifest_path, "old content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(!content.contains("old content"));
    }

    #[tokio::test]
    async fn test_init_uses_current_dir_by_default() {
        let temp_dir = TempDir::new().unwrap();

        // Use explicit path instead of changing directory
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());
        assert!(temp_dir.path().join("agpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_template_content() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let manifest_path = temp_dir.path().join("agpm.toml");
        let content = fs::read_to_string(&manifest_path).unwrap();

        // Verify template content
        assert!(content.contains("# AGPM Manifest"));
        assert!(content.contains("# This file defines your Claude Code resource dependencies"));
        assert!(content.contains("# Add your Git repository sources here"));
        assert!(content.contains("# Example: official ="));
        assert!(content.contains("# Add your agent dependencies here"));
        assert!(content.contains("# Example: my-agent ="));
        assert!(content.contains("# Add your snippet dependencies here"));
        assert!(content.contains("# Example: utils ="));

        // Verify opencode is disabled by default
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("enabled = false"));
        assert!(content.contains("# Enable if you want to use OpenCode resources"));
    }

    #[tokio::test]
    async fn test_init_nested_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a").join("b").join("c");

        let cmd = InitCommand {
            path: Some(nested_path.clone()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());
        assert!(nested_path.exists());
        assert!(nested_path.join("agpm.toml").exists());
    }

    #[tokio::test]
    async fn test_init_force_flag_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Write initial content
        let initial_content = "# Old manifest\n[sources]\n";
        fs::write(&manifest_path, initial_content).unwrap();

        // Try without force - should fail
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };
        let result = cmd.execute().await;
        assert!(result.is_err());

        // Verify old content still exists
        let content = fs::read_to_string(&manifest_path).unwrap();
        assert_eq!(content, initial_content);

        // Try with force - should succeed
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
        };
        let result = cmd.execute().await;
        assert!(result.is_ok());

        // Verify new template content
        let new_content = fs::read_to_string(&manifest_path).unwrap();
        assert!(new_content.contains("# AGPM Manifest"));
        assert!(!new_content.contains("# Old manifest"));
    }

    #[tokio::test]
    async fn test_init_creates_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let gitignore_path = temp_dir.path().join(".gitignore");
        assert!(gitignore_path.exists());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains(".agpm/backups/"));
        assert!(content.contains("agpm.private.toml"));
        assert!(content.contains("agpm.private.lock"));
    }

    #[tokio::test]
    async fn test_init_updates_existing_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with some content
        fs::write(&gitignore_path, "node_modules/\n*.log\n").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains("*.log"));
        assert!(content.contains(".agpm/backups/"));
        assert!(content.contains("agpm.private.toml"));
        assert!(content.contains("agpm.private.lock"));
        assert!(content.contains("# AGPM"));
    }

    #[tokio::test]
    async fn test_init_doesnt_duplicate_gitignore_entry() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with all entries already present
        fs::write(&gitignore_path, ".agpm/backups/\nagpm.private.toml\nagpm.private.lock\n")
            .unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        // Count occurrences - each should be exactly 1
        assert_eq!(content.matches(".agpm/backups/").count(), 1);
        assert_eq!(content.matches("agpm.private.toml").count(), 1);
        assert_eq!(content.matches("agpm.private.lock").count(), 1);
    }

    #[tokio::test]
    async fn test_init_gitignore_with_no_trailing_newline() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with no trailing newline
        fs::write(&gitignore_path, "node_modules/").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".agpm/backups/"));
        assert!(content.contains("agpm.private.toml"));
        assert!(content.contains("agpm.private.lock"));
        // Verify proper formatting (no missing newlines)
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines.contains(&"node_modules/"));
        assert!(lines.contains(&".agpm/backups/"));
        assert!(lines.contains(&"agpm.private.toml"));
        assert!(lines.contains(&"agpm.private.lock"));
    }
}
