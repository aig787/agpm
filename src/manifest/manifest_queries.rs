//! Query operations for manifest data.
//!
//! This module contains all read-only query methods for the manifest, including:
//! - Tool configuration queries
//! - Dependency lookups
//! - Resource type queries
//! - Iteration over all dependencies

use crate::manifest::{ArtifactTypeConfig, Manifest, ResourceDependency, ToolsConfig};
use std::collections::HashMap;
use std::path::PathBuf;

impl Manifest {
    /// Get the tools configuration, returning default if not specified.
    ///
    /// This method provides access to the tool configurations which define
    /// where resources are installed for different tools (claude-code, opencode, agpm).
    ///
    /// Returns the configured tools or the default configuration if not specified.
    pub fn get_tools_config(&self) -> &ToolsConfig {
        self.tools.as_ref().unwrap_or_else(|| {
            // Return a static default - this is safe because ToolsConfig::default() is deterministic
            static DEFAULT: std::sync::OnceLock<ToolsConfig> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(ToolsConfig::default)
        })
    }

    /// Get configuration for a specific tool type.
    ///
    /// Returns None if the tool is not configured.
    pub fn get_tool_config(&self, tool: &str) -> Option<&ArtifactTypeConfig> {
        self.get_tools_config().types.get(tool)
    }

    /// Get the installation path for a resource within a tool.
    ///
    /// Returns the full installation directory path by combining:
    /// - Tool's base directory (e.g., ".claude", ".opencode")
    /// - Resource type's subdirectory (e.g., "agents", "command")
    ///
    /// Returns None if:
    /// - The tool is not configured
    /// - The resource type is not supported by this tool
    /// - The resource has no configured path (special handling like MCP merge)
    pub fn get_artifact_resource_path(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> Option<std::path::PathBuf> {
        let artifact_config = self.get_tool_config(tool)?;
        let resource_config = artifact_config.resources.get(resource_type.to_plural())?;

        resource_config.path.as_ref().map(|subdir| artifact_config.path.join(subdir))
    }

    /// Get the merge target configuration file path for a resource type.
    ///
    /// Returns the path to the configuration file where resources of this type
    /// should be merged (e.g., hooks, MCP servers). Returns None if the resource
    /// type doesn't use merge targets or if the tool doesn't support this resource type.
    ///
    /// # Arguments
    ///
    /// * `tool` - The tool name (e.g., "claude-code", "opencode")
    /// * `resource_type` - The resource type to look up
    ///
    /// # Returns
    ///
    /// The merge target path if configured, otherwise None.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    ///
    /// // Hooks merge into .claude/settings.local.json
    /// let hook_target = manifest.get_merge_target("claude-code", ResourceType::Hook);
    /// assert_eq!(hook_target, Some(".claude/settings.local.json".into()));
    ///
    /// // MCP servers merge into .mcp.json for claude-code
    /// let mcp_target = manifest.get_merge_target("claude-code", ResourceType::McpServer);
    /// assert_eq!(mcp_target, Some(".mcp.json".into()));
    ///
    /// // MCP servers merge into .opencode/opencode.json for opencode
    /// let opencode_mcp = manifest.get_merge_target("opencode", ResourceType::McpServer);
    /// assert_eq!(opencode_mcp, Some(".opencode/opencode.json".into()));
    /// ```
    pub fn get_merge_target(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> Option<PathBuf> {
        let artifact_config = self.get_tool_config(tool)?;
        let resource_config = artifact_config.resources.get(resource_type.to_plural())?;

        resource_config.merge_target.as_ref().map(PathBuf::from)
    }

    /// Check if a resource type is supported by a tool.
    ///
    /// A resource type is considered supported if it has either:
    /// - A configured installation path (for file-based resources)
    /// - A configured merge target (for resources that merge into config files)
    ///
    /// Returns true if the tool has valid configuration for the given resource type.
    pub fn is_resource_supported(
        &self,
        tool: &str,
        resource_type: crate::core::ResourceType,
    ) -> bool {
        self.get_tool_config(tool)
            .and_then(|config| config.resources.get(resource_type.to_plural()))
            .map(|res_config| res_config.path.is_some() || res_config.merge_target.is_some())
            .unwrap_or(false)
    }

    /// Returns all dependencies from all resource types.
    ///
    /// This method collects dependencies from agents, snippets, commands,
    /// scripts, hooks, and MCP servers into a single vector. It's commonly used for:
    /// - Manifest validation across all dependency types
    /// - Dependency resolution operations
    /// - Generating reports of all configured dependencies
    /// - Bulk operations on all dependencies
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the dependency name and its configuration.
    /// Each tuple is `(name, dependency)` where:
    /// - `name`: The dependency name as specified in the manifest
    /// - `dependency`: Reference to the [`ResourceDependency`] configuration
    ///
    /// The order follows the resource type order defined in [`crate::core::ResourceType::all()`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::manifest::Manifest;
    /// # let manifest = Manifest::new();
    /// for (name, dep) in manifest.all_dependencies() {
    ///     println!("Dependency: {} -> {}", name, dep.get_path());
    ///     if let Some(source) = dep.get_source() {
    ///         println!("  Source: {}", source);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn all_dependencies(&self) -> Vec<(&str, &ResourceDependency)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort for deterministic iteration order
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    deps.push((name.as_str(), dep));
                }
            }
        }

        deps
    }

    /// Get all dependencies including MCP servers.
    ///
    /// All resource types now use standard `ResourceDependency`, so no conversion needed.
    #[must_use]
    pub fn all_dependencies_with_mcp(
        &self,
    ) -> Vec<(&str, std::borrow::Cow<'_, ResourceDependency>)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort for deterministic iteration order
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    deps.push((name.as_str(), std::borrow::Cow::Borrowed(dep)));
                }
            }
        }

        deps
    }

    /// Get all dependencies with their resource types.
    ///
    /// Returns a vector of tuples containing the dependency name, dependency details,
    /// and the resource type. This preserves type information that is lost in
    /// `all_dependencies_with_mcp()`.
    ///
    /// This is used by the resolver to correctly type transitive dependencies without
    /// falling back to manifest section order lookups.
    ///
    /// Dependencies for disabled tools are automatically filtered out.
    pub fn all_dependencies_with_types(
        &self,
    ) -> Vec<(&str, std::borrow::Cow<'_, ResourceDependency>, crate::core::ResourceType)> {
        let mut deps = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            if let Some(type_deps) = self.get_dependencies(*resource_type) {
                // CRITICAL: Sort dependencies for deterministic iteration order!
                // HashMap iteration is non-deterministic, so we must sort by name
                // to ensure consistent lockfile generation across runs.
                let mut sorted_deps: Vec<_> = type_deps.iter().collect();
                sorted_deps.sort_by_key(|(name, _)| name.as_str());

                for (name, dep) in sorted_deps {
                    // Determine the tool for this dependency
                    let tool_string = dep
                        .get_tool()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| self.get_default_tool(*resource_type));
                    let tool = tool_string.as_str();

                    // Check if the tool is enabled
                    if let Some(tool_config) = self.get_tools_config().types.get(tool) {
                        if !tool_config.enabled {
                            // Skip dependencies for disabled tools
                            tracing::debug!(
                                "Skipping dependency '{}' for disabled tool '{}'",
                                name,
                                tool
                            );
                            continue;
                        }
                    }

                    // Ensure the tool is set on the dependency (apply default if not explicitly set)
                    let dep_with_tool = if dep.get_tool().is_none() {
                        tracing::debug!(
                            "Setting default tool '{}' for dependency '{}' (type: {:?})",
                            tool,
                            name,
                            resource_type
                        );
                        // Need to set the tool - create a modified copy
                        let mut dep_owned = dep.clone();
                        dep_owned.set_tool(Some(tool_string.clone()));
                        std::borrow::Cow::Owned(dep_owned)
                    } else {
                        std::borrow::Cow::Borrowed(dep)
                    };

                    deps.push((name.as_str(), dep_with_tool, *resource_type));
                }
            }
        }

        deps
    }

    /// Check if a dependency with the given name exists in any section.
    ///
    /// Searches the `[agents]`, `[snippets]`, and `[commands]` sections for a dependency
    /// with the specified name. This is useful for avoiding duplicate names
    /// across different resource types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// assert!(manifest.has_dependency("helper"));
    /// assert!(!manifest.has_dependency("nonexistent"));
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs two `HashMap` lookups, so it's O(1) on average.
    #[must_use]
    pub fn has_dependency(&self, name: &str) -> bool {
        self.agents.contains_key(name)
            || self.snippets.contains_key(name)
            || self.commands.contains_key(name)
    }

    /// Get a dependency by name from any section.
    ///
    /// Searches both the `[agents]` and `[snippets]` sections for a dependency
    /// with the specified name, returning the first match found. Agents are
    /// searched before snippets.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// if let Some(dep) = manifest.get_dependency("helper") {
    ///     println!("Found dependency: {}", dep.get_path());
    /// }
    /// ```
    ///
    /// # Search Order
    ///
    /// Dependencies are searched in this order:
    /// 1. `[agents]` section
    /// 2. `[snippets]` section
    /// 3. `[commands]` section
    ///
    /// If the same name exists in multiple sections, the first match is returned.
    #[must_use]
    pub fn get_dependency(&self, name: &str) -> Option<&ResourceDependency> {
        self.agents
            .get(name)
            .or_else(|| self.snippets.get(name))
            .or_else(|| self.commands.get(name))
    }

    /// Find a dependency by name from any section (alias for `get_dependency`).
    ///
    /// Searches the `[agents]`, `[snippets]`, and `[commands]` sections for a dependency
    /// with the specified name, returning the first match found.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../helper.md".to_string()),
    ///     true  // is_agent
    /// );
    ///
    /// if let Some(dep) = manifest.find_dependency("helper") {
    ///     println!("Found dependency: {}", dep.get_path());
    /// }
    /// ```
    pub fn find_dependency(&self, name: &str) -> Option<&ResourceDependency> {
        self.get_dependency(name)
    }

    /// Get resource dependencies by type.
    ///
    /// Returns a reference to the HashMap of dependencies for the specified resource type.
    /// This provides a unified interface for accessing different resource collections,
    /// similar to `LockFile::get_resources()`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    /// let agents = manifest.get_resources(&ResourceType::Agent);
    /// println!("Found {} agent dependencies", agents.len());
    /// ```
    #[must_use]
    pub fn get_resources(
        &self,
        resource_type: &crate::core::ResourceType,
    ) -> &HashMap<String, ResourceDependency> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &self.agents,
            ResourceType::Snippet => &self.snippets,
            ResourceType::Command => &self.commands,
            ResourceType::Script => &self.scripts,
            ResourceType::Hook => &self.hooks,
            ResourceType::McpServer => &self.mcp_servers,
        }
    }

    /// Get all resource dependencies across all types.
    ///
    /// Returns a vector of tuples containing the resource type, manifest key (name),
    /// and the dependency specification. This provides a unified way to iterate over
    /// all resources regardless of type.
    ///
    /// # Returns
    ///
    /// A vector of `(ResourceType, &str, &ResourceDependency)` tuples where:
    /// - The first element is the type of resource (Agent, Snippet, etc.)
    /// - The second element is the manifest key (the name in the TOML file)
    /// - The third element is the resource dependency specification
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let manifest = Manifest::new();
    /// let all = manifest.all_resources();
    ///
    /// for (resource_type, name, dep) in all {
    ///     println!("{:?}: {}", resource_type, name);
    /// }
    /// ```
    #[must_use]
    pub fn all_resources(&self) -> Vec<(crate::core::ResourceType, &str, &ResourceDependency)> {
        use crate::core::ResourceType;

        let mut resources = Vec::new();

        for resource_type in ResourceType::all() {
            let type_resources = self.get_resources(resource_type);
            for (name, dep) in type_resources {
                resources.push((*resource_type, name.as_str(), dep));
            }
        }

        resources
    }
}
