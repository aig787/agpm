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
use toml_edit::{DocumentMut, Item, Table};

use crate::manifest::tool_config::ToolsConfig;

/// Builds the default manifest template programmatically from the actual default configurations.
///
/// This function constructs the initial `agpm.toml` manifest by serializing the default tool
/// configurations from `ToolsConfig::default()`, ensuring a single source of truth for defaults.
/// All comments and structure are added programmatically to match the expected manifest format.
///
/// # Returns
///
/// A formatted TOML string containing:
/// - Header comment
/// - Empty `[sources]` section with comments
/// - Commented project variables example
/// - `[tools.*]` sections for claude-code, opencode, and agpm with their resource configs (from `ToolsConfig::default()`)
/// - Empty resource sections (agents, snippets, commands, scripts, hooks, mcp-servers) with examples
///
/// # Examples
///
/// ```rust,ignore
/// let manifest = build_default_manifest();
/// std::fs::write("agpm.toml", manifest)?;
/// ```
#[allow(clippy::too_many_lines)]
fn build_default_manifest() -> String {
    let mut doc = DocumentMut::new();

    // Add leading comment
    doc.as_table_mut().decor_mut().set_prefix(
        "# AGPM Manifest\n# This file defines your Claude Code resource dependencies\n\n",
    );

    // [sources] section
    let mut sources = Table::new();
    sources.set_implicit(false);
    sources.decor_mut().set_prefix(
        "# Add your Git repository sources here\n\
         # Example: official = \"https://github.com/aig787/agpm-community.git\"\n",
    );
    doc.insert("sources", Item::Table(sources));

    // Add comment about project variables (as a comment block between sources and tools)
    let project_comment = "\n\
        # Project-specific template variables (optional)\n\
        # Provides context to AI agents - use any structure you want!\n\
        # [project]\n\
        # style_guide = \"docs/STYLE_GUIDE.md\"\n\
        # max_line_length = 100\n\
        # test_framework = \"pytest\"\n\
        #\n\
        # [project.paths]\n\
        # architecture = \"docs/ARCHITECTURE.md\"\n\
        # conventions = \"docs/CONVENTIONS.md\"\n\
        #\n\
        # Access in templates: {{ agpm.project.style_guide }}\n\
        \n\
        # Tool type configurations (multi-tool support)\n";

    // Build tool configurations compactly using inline tables
    let tools_config = ToolsConfig::default();

    let mut tools_config_table = Table::new();
    tools_config_table.set_implicit(false);
    tools_config_table.decor_mut().set_prefix(project_comment);

    // Process each tool in order: claude-code, opencode, agpm
    for tool_name in &["claude-code", "opencode", "agpm"] {
        if let Some(tool_config) = tools_config.types.get(*tool_name) {
            let mut current_tool_table = Table::new();
            current_tool_table.set_implicit(false);

            // Add 'enabled' field for opencode (disabled by default)
            if *tool_name == "opencode" {
                current_tool_table.insert("enabled", toml_edit::value(false));
                if let Some(Item::Value(v)) = current_tool_table.get_mut("enabled") {
                    v.decor_mut().set_suffix("  # Enable if you want to use OpenCode resources");
                }
            }

            // Add 'path' field
            current_tool_table
                .insert("path", toml_edit::value(tool_config.path.to_string_lossy().as_ref()));

            // Build resources as inline table for compact output
            let mut resources_inline = toml_edit::InlineTable::new();

            // Sort resource keys for consistent output
            let mut resource_keys: Vec<_> = tool_config.resources.keys().collect();
            resource_keys.sort();

            for resource_key in resource_keys {
                if let Some(resource_config) = tool_config.resources.get(resource_key.as_str()) {
                    let mut current_resource_inline = toml_edit::InlineTable::new();

                    if let Some(path) = &resource_config.path {
                        current_resource_inline
                            .insert("path", toml_edit::Value::from(path.as_str()));
                    }

                    if let Some(merge_target) = &resource_config.merge_target {
                        current_resource_inline
                            .insert("merge-target", toml_edit::Value::from(merge_target.as_str()));
                    }

                    // Only include flatten if explicitly set (not None)
                    if let Some(flatten) = resource_config.flatten {
                        current_resource_inline.insert("flatten", toml_edit::Value::from(flatten));
                    }

                    resources_inline
                        .insert(resource_key, toml_edit::Value::from(current_resource_inline));
                }
            }

            current_tool_table
                .insert("resources", Item::Value(toml_edit::Value::from(resources_inline)));

            // Add tool-specific comment after path
            let comment = match *tool_name {
                "claude-code" => {
                    "\n# Note: hooks and mcp-servers merge into configuration files (no file installation)"
                }
                "opencode" => {
                    "\n# Note: MCP servers merge into opencode.json (no file installation)"
                }
                _ => "",
            };

            if let Some(Item::Value(path)) = current_tool_table.get_mut("path") {
                path.decor_mut().set_suffix(comment);
            }

            tools_config_table.insert(tool_name, Item::Table(current_tool_table));
        }
    }

    doc.insert("tools", Item::Table(tools_config_table));

    // Add default-tools section (commented out - optional configuration)
    let default_tools_comment = "\n\
        # Default tool overrides (optional)\n\
        # Override which tool is used by default for each resource type\n\
        # [default-tools]\n\
        # snippets = \"claude-code\"  # Override default (agpm) for Claude-only users\n\
        # agents = \"opencode\"        # Use OpenCode by default for agents\n\
        \n";

    // Add patch section (commented out - optional configuration)
    let patch_comment = "\
        # Patches - override resource fields without forking (optional)\n\
        # [patch.agents.my-agent]\n\
        # model = \"claude-3-haiku\"\n\
        # temperature = \"0.7\"\n\
        #\n\
        # [patch.commands.deploy]\n\
        # timeout = \"300\"\n\
        \n";

    // Combine comments and add as prefix to first resource section
    let combined_comment = format!("{default_tools_comment}{patch_comment}");

    // Add resource sections with examples
    let resource_examples = [
        (
            "agents",
            "# Add your agent dependencies here\n\
             # Example: my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\" }\n\
             # For OpenCode: my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\", tool = \"opencode\" }\n",
        ),
        (
            "snippets",
            "# Add your snippet dependencies here\n\
             # Example: utils = { source = \"official\", path = \"snippets/utils.md\", tool = \"agpm\" }\n",
        ),
        (
            "commands",
            "# Add your command dependencies here\n\
             # Example: deploy = { source = \"official\", path = \"commands/deploy.md\" }\n",
        ),
        (
            "scripts",
            "# Add your script dependencies here\n\
             # Example: build = { source = \"official\", path = \"scripts/build.sh\" }\n",
        ),
        (
            "hooks",
            "# Add your hook dependencies here\n\
             # Example: pre-commit = { source = \"official\", path = \"hooks/pre-commit.json\" }\n",
        ),
        (
            "mcp-servers",
            "# Add your MCP server dependencies here\n\
             # Example: filesystem = { source = \"official\", path = \"mcp-servers/filesystem.json\" }\n",
        ),
    ];

    for (i, (section_name, comment)) in resource_examples.iter().enumerate() {
        let mut section = Table::new();
        section.set_implicit(false);

        // Add the default-tools and patch comments before the first resource section
        let prefix = if i == 0 {
            format!("{combined_comment}{comment}")
        } else {
            format!("\n{comment}")
        };

        section.decor_mut().set_prefix(prefix);
        doc.insert(section_name, Item::Table(section));
    }

    doc.to_string()
}

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
///     defaults: false,
/// };
///
/// // Initialize in specific directory with force overwrite
/// let cmd = InitCommand {
///     path: Some(PathBuf::from("./my-project")),
///     force: true,
///     defaults: false,
/// };
///
/// // Merge defaults into existing manifest
/// let cmd = InitCommand {
///     path: None,
///     force: false,
///     defaults: true,
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

    /// Merge default configurations into existing manifest
    ///
    /// Instead of creating a new manifest, this flag loads an existing `agpm.toml`,
    /// merges in any missing default configurations (tool configs, resource sections),
    /// and saves the result while preserving all existing values and comments.
    ///
    /// This is useful for updating old manifests to include new default configurations
    /// without overwriting customizations.
    #[arg(long)]
    defaults: bool,
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
        // If --defaults flag is set, merge defaults into existing manifest
        if self.defaults {
            return self.execute_with_defaults().await;
        }

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

        // Write the default template (built programmatically from ToolsConfig::default())
        fs::write(&manifest_path, build_default_manifest())?;

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

        println!(
            "\n{} If Claude Code can't find installed resources, run {} in Claude Code",
            "💡".cyan(),
            "/config".bright_white()
        );
        println!(
            "   and set {} to {}.",
            "Respect .gitignore in file picker".yellow(),
            "false".green()
        );

        Ok(())
    }

    /// Execute init with --defaults flag to merge default configurations.
    ///
    /// This method loads an existing manifest, parses the default template, merges
    /// them at the TOML document level (preserving comments), and saves the result.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if defaults were successfully merged
    /// - `Err(anyhow::Error)` if:
    ///   - No manifest exists at the target path
    ///   - Unable to parse existing manifest or default template
    ///   - Unable to write the merged result
    async fn execute_with_defaults(&self) -> Result<()> {
        let target_dir = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = target_dir.join("agpm.toml");

        // Check that manifest exists
        if !manifest_path.exists() {
            return Err(anyhow!(
                "No manifest found at {}\nRun 'agpm init' first to create a new manifest.",
                manifest_path.display()
            ));
        }

        // Parse default template as Document (built programmatically from ToolsConfig::default())
        let default_manifest = build_default_manifest();
        let default_doc = default_manifest
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| anyhow!("Failed to parse default template: {e}"))?;

        // Load existing manifest as Document (preserves comments!)
        let existing_content = fs::read_to_string(&manifest_path)?;
        let mut existing_doc = existing_content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| anyhow!("Failed to parse existing manifest: {e}"))?;

        // Merge: add missing keys from defaults, preserve existing
        Self::merge_toml_documents(&mut existing_doc, &default_doc);

        // Write merged document back
        fs::write(&manifest_path, existing_doc.to_string())?;

        // Update .gitignore
        Self::update_gitignore(&target_dir)?;

        println!("{} Updated agpm.toml with default configurations", "✓".green());

        println!(
            "\n{} If Claude Code can't find installed resources, run {} in Claude Code",
            "💡".cyan(),
            "/config".bright_white()
        );
        println!(
            "   and set {} to {}.",
            "Respect .gitignore in file picker".yellow(),
            "false".green()
        );

        Ok(())
    }

    /// Merge two TOML documents, with existing values taking precedence.
    ///
    /// This is a wrapper around `merge_toml_tables` that operates at the document level.
    ///
    /// # Arguments
    ///
    /// * `existing` - The existing document (will be modified in place)
    /// * `defaults` - The default template document (read-only)
    fn merge_toml_documents(
        existing: &mut toml_edit::DocumentMut,
        defaults: &toml_edit::DocumentMut,
    ) {
        Self::merge_toml_tables(existing.as_table_mut(), defaults.as_table());
    }

    /// Recursively merge TOML tables, with existing values taking precedence.
    ///
    /// For each key in the defaults table:
    /// - If the key doesn't exist in existing, add it from defaults
    /// - If both are tables (regular or inline), recurse to merge nested keys
    /// - Otherwise, keep the existing value unchanged
    ///
    /// This preserves all comments, formatting, and existing values in the existing table.
    ///
    /// # Arguments
    ///
    /// * `existing` - The existing table (modified in place)
    /// * `defaults` - The defaults table (read-only)
    fn merge_toml_tables(existing: &mut toml_edit::Table, defaults: &toml_edit::Table) {
        for (key, default_value) in defaults {
            if !existing.contains_key(key) {
                // Key missing in existing - add from defaults
                existing.insert(key, default_value.clone());
            } else {
                // Key exists - recurse if both are tables
                match (existing.get_mut(key), default_value) {
                    (
                        Some(toml_edit::Item::Table(existing_table)),
                        toml_edit::Item::Table(default_table),
                    ) => {
                        // Both are regular tables - recurse
                        Self::merge_toml_tables(existing_table, default_table);
                    }
                    (
                        Some(toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            existing_inline,
                        ))),
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(default_inline)),
                    ) => {
                        // Both are inline tables - merge keys
                        for (inline_key, inline_default_value) in default_inline {
                            if !existing_inline.contains_key(inline_key) {
                                existing_inline.insert(inline_key, inline_default_value.clone());
                            }
                        }
                    }
                    _ => {
                        // Different types or non-table - existing wins, do nothing
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_init_creates_manifest() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

        let manifest_path = temp_dir.path().join("agpm.toml");
        assert!(manifest_path.exists());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(content.contains("[agents]"));
        assert!(content.contains("[snippets]"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_creates_directory_if_not_exists() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let new_dir = temp_dir.path().join("new_project");

        let cmd = InitCommand {
            path: Some(new_dir.clone()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

        assert!(new_dir.exists());
        assert!(new_dir.join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_init_fails_if_manifest_exists() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        fs::write(&manifest_path, "existing content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_force_overwrites_existing() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");
        fs::write(&manifest_path, "old content").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: true,
            defaults: false,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[sources]"));
        assert!(!content.contains("old content"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_uses_current_dir_by_default() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();

        // Use explicit path instead of changing directory
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;
        assert!(temp_dir.path().join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_init_template_content() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

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

        // Verify opencode is enabled by default with flatten settings
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("flatten = true"));
        assert!(
            content.contains("# Note: MCP servers merge into opencode.json (no file installation)")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_init_nested_directory_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a").join("b").join("c");

        let cmd = InitCommand {
            path: Some(nested_path.clone()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;
        assert!(nested_path.exists());
        assert!(nested_path.join("agpm.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_init_force_flag_behavior() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Write initial content
        let initial_content = "# Old manifest\n[sources]\n";
        fs::write(&manifest_path, initial_content).unwrap();

        // Try without force - should fail
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
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
            defaults: false,
        };
        cmd.execute().await?;

        // Verify new template content
        let new_content = fs::read_to_string(&manifest_path).unwrap();
        assert!(new_content.contains("# AGPM Manifest"));
        assert!(!new_content.contains("# Old manifest"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_creates_gitignore() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

        let gitignore_path = temp_dir.path().join(".gitignore");
        assert!(gitignore_path.exists());

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains(".agpm/backups/"));
        assert!(content.contains("agpm.private.toml"));
        assert!(content.contains("agpm.private.lock"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_updates_existing_gitignore() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with some content
        fs::write(&gitignore_path, "node_modules/\n*.log\n").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains("*.log"));
        assert!(content.contains(".agpm/backups/"));
        assert!(content.contains("agpm.private.toml"));
        assert!(content.contains("agpm.private.lock"));
        assert!(content.contains("# AGPM"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_doesnt_duplicate_gitignore_entry() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with all entries already present
        fs::write(&gitignore_path, ".agpm/backups/\nagpm.private.toml\nagpm.private.lock\n")
            .unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&gitignore_path).unwrap();
        // Count occurrences - each should be exactly 1
        assert_eq!(content.matches(".agpm/backups/").count(), 1);
        assert_eq!(content.matches("agpm.private.toml").count(), 1);
        assert_eq!(content.matches("agpm.private.lock").count(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_init_gitignore_with_no_trailing_newline() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Create existing .gitignore with no trailing newline
        fs::write(&gitignore_path, "node_modules/").unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: false,
        };

        cmd.execute().await?;

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
        Ok(())
    }

    #[tokio::test]
    async fn test_init_defaults_preserves_comments() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create manifest with custom comments
        let manifest_content = r#"
# My custom comment about sources
[sources]
community = "https://github.com/example/repo.git"

# Note: I only use Claude Code
[agents]
my-agent = { source = "community", path = "agents/my-agent.md", version = "v1.0.0" }
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&manifest_path).unwrap();
        // Verify comments are preserved
        assert!(content.contains("# My custom comment about sources"));
        assert!(content.contains("# Note: I only use Claude Code"));
        // Verify existing values preserved
        assert!(content.contains("community"));
        assert!(content.contains("my-agent"));
        // Verify tools were added
        assert!(content.contains("[tools.claude-code]"));
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("[tools.agpm]"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_defaults_adds_missing_sections() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create minimal manifest with only sources
        let manifest_content = r#"
[sources]
community = "https://github.com/example/repo.git"
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&manifest_path).unwrap();
        // Verify all tool sections were added
        assert!(content.contains("[tools.claude-code]"));
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("[tools.agpm]"));
        // Verify existing source preserved
        assert!(content.contains("community"));
        assert!(content.contains("https://github.com/example/repo.git"));
        // Verify resource sections added
        assert!(content.contains("[agents]"));
        assert!(content.contains("[snippets]"));
        assert!(content.contains("[commands]"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_defaults_preserves_existing_tools() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Create manifest with custom claude-code config
        let manifest_content = r#"
[sources]
community = "https://github.com/example/repo.git"

[tools.claude-code]
path = ".my-custom-claude"
resources = { agents = { path = "my-agents" } }

[agents]
my-agent = { source = "community", path = "agents/my-agent.md" }
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        cmd.execute().await?;

        let content = fs::read_to_string(&manifest_path).unwrap();
        // Verify custom claude-code config preserved
        assert!(content.contains(".my-custom-claude"));
        assert!(content.contains("my-agents"));
        // Verify other tools added
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("[tools.agpm]"));
        // Verify existing agent preserved
        assert!(content.contains("my-agent"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_defaults_fails_if_no_manifest() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();

        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        let result = cmd.execute().await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No manifest found"));
        assert!(error_msg.contains("agpm init"));
        Ok(())
    }

    #[tokio::test]
    async fn test_init_defaults_idempotent() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("agpm.toml");

        // Write the default template (built programmatically)
        fs::write(&manifest_path, build_default_manifest()).unwrap();

        // Run --defaults on already complete manifest
        let cmd = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        cmd.execute().await?;

        // Verify content is essentially unchanged (toml_edit may normalize formatting)
        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("[tools.claude-code]"));
        assert!(content.contains("[tools.opencode]"));
        assert!(content.contains("[tools.agpm]"));

        // Run again to verify true idempotency
        let cmd2 = InitCommand {
            path: Some(temp_dir.path().to_path_buf()),
            force: false,
            defaults: true,
        };

        cmd2.execute().await?;
        Ok(())
    }
}
