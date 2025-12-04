//! List installed Claude Code resources from the lockfile.
//!
//! This module provides the `list` command which displays information about
//! currently installed dependencies as recorded in the lockfile (`agpm.lock`).
//! The command offers various output formats and filtering options to help
//! users understand their project's dependencies.
//!
//! # Features
//!
//! - **Multiple Output Formats**: JSON, table, or tree view
//! - **Filtering Options**: Show only agents, snippets, or specific dependencies
//! - **Detailed Information**: Source URLs, versions, installation paths, checksums
//! - **Dependency Analysis**: Shows unused dependencies and source statistics
//! - **Path Information**: Displays where resources are installed
//!
//! # Examples
//!
//! List all installed resources:
//! ```bash
//! agpm list
//! ```
//!
//! List only agents:
//! ```bash
//! agpm list --agents
//! ```
//!
//! List with detailed information:
//! ```bash
//! agpm list --details
//! ```
//!
//! Output in JSON format:
//! ```bash
//! agpm list --format json
//! ```
//!
//! Show dependency tree:
//! ```bash
//! agpm list --format tree
//! ```
//!
//! List specific dependencies:
//! ```bash
//! agpm list my-agent utils-snippet
//! ```
//!
//! # Output Formats
//!
//! ## Table Format (Default)
//! ```text
//! NAME          TYPE     SOURCE      VERSION   PATH
//! code-reviewer agent    official    v1.0.0    agents/code-reviewer.md
//! utils         snippet  community   v2.1.0    snippets/utils.md
//! ```
//!
//! ## JSON Format
//! ```json
//! {
//!   "agents": [...],
//!   "snippets": [...],
//!   "sources": [...]
//! }
//! ```
//!
//! ## Tree Format
//! ```text
//! Sources:
//! ├── official (https://github.com/org/official.git)
//! │   └── agents/code-reviewer.md@v1.0.0
//! └── community (https://github.com/org/community.git)
//!     └── snippets/utils.md@v2.1.0
//! ```
//!
//! # Data Sources
//!
//! The command primarily reads from:
//! - **Primary**: `agpm.lock` - Contains installed resource information
//! - **Secondary**: `agpm.toml` - Used for manifest comparison and validation
//!
//! # Error Conditions
//!
//! - No lockfile found (no dependencies installed)
//! - Lockfile is corrupted or has invalid format
//! - Requested dependency names not found in lockfile
//! - File system access issues

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::cache::Cache;
use crate::manifest::{Manifest, find_manifest_with_optional};

mod converters;
mod filters;
mod formatters;

#[cfg(test)]
mod list_tests;

pub use formatters::{ListItem, OutputConfig};

/// Command to list installed Claude Code resources.
///
/// This command displays information about dependencies currently installed
/// in the project based on the lockfile. It supports various output formats,
/// filtering options, and detail levels to help users understand their
/// project's resource dependencies.
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::list::ListCommand;
///
/// // List all resources in default table format
/// let cmd = ListCommand {
///     agents: false,
///     snippets: false,
///     format: "table".to_string(),
///     manifest: false,
///     r#type: None,
///     source: None,
///     search: None,
///     detailed: false,
///     files: false,
///     verbose: false,
///     sort: None,
/// };
///
/// // List only agents with detailed information
/// let cmd = ListCommand {
///     agents: true,
///     snippets: false,
///     format: "table".to_string(),
///     manifest: false,
///     r#type: None,
///     source: None,
///     search: None,
///     detailed: true,
///     files: true,
///     verbose: false,
///     sort: Some("name".to_string()),
/// };
/// ```
#[derive(Args)]
pub struct ListCommand {
    /// Show only agents
    ///
    /// When specified, filters the output to show only agent resources,
    /// excluding snippets. Mutually exclusive with `--snippets`.
    #[arg(long)]
    agents: bool,

    /// Show only snippets
    ///
    /// When specified, filters the output to show only snippet resources,
    /// excluding agents and commands. Mutually exclusive with `--agents` and `--commands`.
    #[arg(long)]
    snippets: bool,

    /// Show only commands
    ///
    /// When specified, filters the output to show only command resources,
    /// excluding agents and snippets. Mutually exclusive with `--agents` and `--snippets`.
    #[arg(long)]
    commands: bool,

    /// Show only skills
    ///
    /// When specified, filters the output to show only skill resources,
    /// excluding other resource types. Mutually exclusive with `--agents`, `--snippets`, and `--commands`.
    #[arg(long)]
    skills: bool,

    /// Output format (table, json, yaml, compact, simple)
    ///
    /// Controls how the resource information is displayed:
    /// - `table`: Formatted table with columns (default)
    /// - `json`: JSON object with structured data
    /// - `yaml`: YAML format for structured data
    /// - `compact`: Minimal single-line format
    /// - `simple`: Plain text list format
    #[arg(short = 'f', long, default_value = "table")]
    format: String,

    /// Show from manifest instead of lockfile
    ///
    /// When enabled, shows dependencies defined in the manifest (`agpm.toml`)
    /// rather than installed dependencies from the lockfile (`agpm.lock`).
    /// This is useful for comparing intended vs. actual installations.
    #[arg(long)]
    manifest: bool,

    /// Filter by resource type
    ///
    /// Filters resources by their type (agent, snippet). This is an
    /// alternative to using the `--agents` or `--snippets` flags.
    #[arg(long, value_name = "TYPE")]
    r#type: Option<String>,

    /// Filter by source name
    ///
    /// Shows only resources from the specified source repository.
    /// The source name should match one defined in the manifest.
    #[arg(long, value_name = "SOURCE")]
    source: Option<String>,

    /// Search by name pattern
    ///
    /// Filters resources whose names match the given pattern.
    /// Supports substring matching (case-insensitive).
    #[arg(long, value_name = "PATTERN")]
    search: Option<String>,

    /// Show detailed information
    ///
    /// Includes additional columns in the output such as checksums,
    /// resolved commits, and full source URLs. This provides more
    /// comprehensive information about each resource.
    #[arg(long)]
    detailed: bool,

    /// Show installed file paths
    ///
    /// Includes the local file system paths where resources are installed.
    /// Useful for understanding the project layout and locating resource files.
    #[arg(long)]
    files: bool,

    /// Verbose output (show all sections)
    ///
    /// Enables verbose mode which shows additional information including
    /// source statistics, dependency summaries, and extended metadata.
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Sort by field (name, version, source, type)
    ///
    /// Controls the sorting order of the resource list. Supported fields:
    /// - `name`: Sort alphabetically by resource name
    /// - `version`: Sort by version (semantic versioning aware)
    /// - `source`: Sort by source repository name
    /// - `type`: Sort by resource type (agents first, then snippets)
    #[arg(long, value_name = "FIELD")]
    sort: Option<String>,
}

impl ListCommand {
    /// Execute the list command to display installed resources.
    ///
    /// This method orchestrates the process of loading resource data, applying
    /// filters, and formatting the output according to the specified options.
    ///
    /// # Behavior
    ///
    /// 1. **Data Loading**: Loads resource data from lockfile or manifest
    /// 2. **Filtering**: Applies type, source, and search filters
    /// 3. **Sorting**: Orders results according to the specified sort field
    /// 4. **Formatting**: Outputs data in the requested format
    ///
    /// # Data Sources
    ///
    /// - **Default**: Uses lockfile (`agpm.lock`) to show installed resources
    /// - **Manifest Mode**: Uses manifest (`agpm.toml`) to show defined dependencies
    ///
    /// # Filtering Logic
    ///
    /// Filters are applied in this order:
    /// 1. Type filter (agents/snippets)
    /// 2. Source filter (specific repository)
    /// 3. Search pattern (name matching)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the list was displayed successfully
    /// - `Err(anyhow::Error)` if:
    ///   - No lockfile found (and not using manifest mode)
    ///   - Lockfile format is invalid
    /// - Unable to load manifest (in manifest mode)
    ///   - Output formatting fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::list::ListCommand;
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = ListCommand {
    ///     agents: false,
    ///     snippets: false,
    ///     format: "json".to_string(),
    ///     manifest: false,
    ///     r#type: None,
    ///     source: None,
    ///     search: None,
    ///     detailed: true,
    ///     files: false,
    ///     verbose: false,
    ///     sort: Some("name".to_string()),
    /// };
    /// // cmd.execute_with_manifest_path(None).await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # }));
    /// ```
    /// Execute the list command with an optional manifest path
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // Find manifest file
        let manifest_path = find_manifest_with_optional(manifest_path)
            .context("No agpm.toml found. Please create one to define your dependencies.")?;

        self.execute_from_path(manifest_path).await
    }

    pub async fn execute_from_path(self, manifest_path: PathBuf) -> Result<()> {
        // Validate arguments
        self.validate_arguments()?;

        // For consistency with execute(), require the manifest to exist
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!("Manifest file {} not found", manifest_path.display()));
        }

        let project_dir = manifest_path.parent().ok_or_else(|| {
            anyhow::anyhow!("Manifest file has no parent directory: {}", manifest_path.display())
        })?;

        if self.manifest {
            // List from manifest
            self.list_from_manifest(&manifest_path)?;
        } else {
            // List from lockfile
            self.list_from_lockfile(project_dir).await?;
        }

        Ok(())
    }

    fn validate_arguments(&self) -> Result<()> {
        // Validate format
        match self.format.as_str() {
            "table" | "json" | "yaml" | "compact" | "simple" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid format '{}'. Valid formats are: table, json, yaml, compact, simple",
                    self.format
                ));
            }
        }

        // Validate type filter
        if let Some(ref t) = self.r#type {
            match t.as_str() {
                "agents" | "snippets" | "commands" | "scripts" | "hooks" | "mcp-servers"
                | "skills" | "agent" | "snippet" | "command" | "script" | "hook" | "mcp-server"
                | "skill" => {}
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid type '{t}'. Valid types are: agents, snippets, commands, scripts, hooks, mcp-servers, skills"
                    ));
                }
            }
        }

        // Validate sort field
        if let Some(ref field) = self.sort {
            match field.as_str() {
                "name" | "version" | "source" | "type" => {}
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid sort field '{field}'. Valid fields are: name, version, source, type"
                    ));
                }
            }
        }

        Ok(())
    }

    fn list_from_manifest(&self, manifest_path: &std::path::Path) -> Result<()> {
        let manifest = Manifest::load(manifest_path)?;

        // Collect and filter dependencies
        let mut items = Vec::new();

        // Iterate through all resource types using the central definition
        for resource_type in crate::core::ResourceType::all() {
            // Check if we should show this resource type
            if !self.should_show_resource_type(*resource_type) {
                continue;
            }

            let type_str = resource_type.to_string();

            // Note: MCP servers are handled separately as they use a different dependency type
            if *resource_type == crate::core::ResourceType::McpServer {
                // Skip MCP servers in this generic iteration - they need special handling
                continue;
            }

            // Get dependencies for this resource type from the manifest
            if let Some(deps) = manifest.get_dependencies(*resource_type) {
                for (name, dep) in deps {
                    if self.matches_filters(name, Some(dep), &type_str) {
                        items.push(ListItem {
                            name: name.clone(),
                            source: dep.get_source().map(std::string::ToString::to_string),
                            version: dep.get_version().map(std::string::ToString::to_string),
                            path: Some(dep.get_path().to_string()),
                            resource_type: type_str.clone(),
                            installed_at: None,
                            checksum: None,
                            resolved_commit: None,
                            tool: Some(
                                dep.get_tool()
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| manifest.get_default_tool(*resource_type)),
                            ),
                            applied_patches: std::collections::BTreeMap::new(),
                            approximate_token_count: None,
                        });
                    }
                }
            }
        }

        // Handle MCP servers (now using standard ResourceDependency)
        if self.should_show_resource_type(crate::core::ResourceType::McpServer) {
            for (name, mcp_dep) in &manifest.mcp_servers {
                // MCP servers now use standard ResourceDependency
                if self.matches_filters(name, Some(mcp_dep), "mcp-server") {
                    items.push(ListItem {
                        name: name.clone(),
                        source: mcp_dep.get_source().map(std::string::ToString::to_string),
                        version: mcp_dep.get_version().map(std::string::ToString::to_string),
                        path: Some(mcp_dep.get_path().to_string()),
                        resource_type: "mcp-server".to_string(),
                        installed_at: None,
                        checksum: None,
                        resolved_commit: None,
                        tool: Some(mcp_dep.get_tool().map(|s| s.to_string()).unwrap_or_else(
                            || manifest.get_default_tool(crate::core::ResourceType::McpServer),
                        )),
                        applied_patches: std::collections::BTreeMap::new(),
                        approximate_token_count: None,
                    });
                }
            }
        }

        // Sort items
        self.sort_items(&mut items);

        // Output results
        self.output_items(&items, "Dependencies from agpm.toml:")?;

        Ok(())
    }

    async fn list_from_lockfile(&self, project_dir: &std::path::Path) -> Result<()> {
        let lockfile_path = project_dir.join("agpm.lock");

        if !lockfile_path.exists() {
            if self.format == "json" {
                println!("{{}}");
            } else {
                println!("No installed resources found.");
                println!("⚠️  agpm.lock not found. Run 'agpm install' first.");
            }
            return Ok(());
        }

        // Create a temporary manifest for CommandContext (we only need it for lockfile loading)
        let manifest_path = project_dir.join("agpm.toml");
        let manifest = crate::manifest::Manifest::load(&manifest_path)?;
        let command_context =
            crate::cli::common::CommandContext::new(manifest, project_dir.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let lockfile = match command_context.load_lockfile_with_regeneration(true, "list")? {
            Some(lockfile) => lockfile,
            None => {
                // Lockfile was regenerated and doesn't exist yet
                if self.format == "json" {
                    println!("{{}}");
                } else {
                    println!("No installed resources found.");
                    println!(
                        "⚠️  Lockfile was invalid and has been removed. Run 'agpm install' to regenerate it."
                    );
                }
                return Ok(());
            }
        };

        // Create cache if needed for detailed mode with patches
        let cache = if self.detailed {
            Some(Cache::new().context("Failed to initialize cache")?)
        } else {
            None
        };

        // Collect and filter entries
        let mut items = Vec::new();

        // Iterate through all resource types using the central definition
        for resource_type in crate::core::ResourceType::all() {
            // Check if we should show this resource type
            if !self.should_show_resource_type(*resource_type) {
                continue;
            }

            let type_str = resource_type.to_string();

            // Get resources for this type from the lockfile
            for entry in lockfile.get_resources(resource_type) {
                if self.matches_lockfile_filters(&entry.name, entry, &type_str) {
                    items.push(converters::lockentry_to_listitem(entry, &type_str));
                }
            }
        }

        // Sort items
        self.sort_items(&mut items);

        // Handle special flags

        // Output results
        if self.detailed {
            formatters::output_items_detailed(
                &items,
                "Installed resources from agpm.lock:",
                &lockfile,
                cache.as_ref(),
                self.should_show_resource_type(crate::core::ResourceType::Agent),
                self.should_show_resource_type(crate::core::ResourceType::Snippet),
            )
            .await?;
        } else {
            self.output_items(&items, "Installed resources from agpm.lock:")?;
        }

        Ok(())
    }

    /// Determine if a resource type should be shown based on filters
    fn should_show_resource_type(&self, resource_type: crate::core::ResourceType) -> bool {
        filters::should_show_resource_type(
            resource_type,
            self.agents,
            self.snippets,
            self.commands,
            self.skills,
            self.r#type.as_ref(),
        )
    }

    /// Check if an item matches all filters
    fn matches_filters(
        &self,
        name: &str,
        dep: Option<&crate::manifest::ResourceDependency>,
        resource_type: &str,
    ) -> bool {
        filters::matches_filters(
            name,
            dep,
            resource_type,
            self.source.as_ref(),
            self.search.as_ref(),
        )
    }

    /// Check if a lockfile entry matches all filters
    fn matches_lockfile_filters(
        &self,
        name: &str,
        entry: &crate::lockfile::LockedResource,
        resource_type: &str,
    ) -> bool {
        filters::matches_lockfile_filters(
            name,
            entry,
            resource_type,
            self.source.as_ref(),
            self.search.as_ref(),
        )
    }

    /// Sort items based on sort criteria
    fn sort_items(&self, items: &mut [ListItem]) {
        filters::sort_items(items, self.sort.as_ref());
    }

    /// Output items in the specified format
    fn output_items(&self, items: &[ListItem], title: &str) -> Result<()> {
        let config = OutputConfig {
            title: title.to_string(),
            format: self.format.clone(),
            files: self.files,
            detailed: self.detailed,
            verbose: self.verbose,
            should_show_agents: self.should_show_resource_type(crate::core::ResourceType::Agent),
            should_show_snippets: self
                .should_show_resource_type(crate::core::ResourceType::Snippet),
        };
        formatters::output_items(items, &config)
    }
}
