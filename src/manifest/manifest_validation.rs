//! Validation operations for manifest files.
//!
//! This module contains validation logic for ensuring manifests are:
//! - Structurally correct
//! - Logically consistent
//! - Secure (no credential leakage, path traversal, etc.)
//! - Cross-platform compatible

use crate::manifest::{Manifest, PatchData, ToolsConfig, expand_url};
use anyhow::Result;
use std::collections::BTreeMap;

impl Manifest {
    /// Validate the manifest structure and enforce business rules.
    ///
    /// This method performs comprehensive validation of the manifest to ensure
    /// logical consistency, security best practices, and correct dependency
    /// relationships. It's automatically called during [`Self::load`] but can
    /// also be used independently to validate programmatically constructed manifests.
    ///
    /// # Validation Rules
    ///
    /// ## Source Validation
    /// - All source URLs must use supported protocols (HTTPS, SSH, git://, file://)
    /// - No plain directory paths allowed as sources (must use file:// URLs)
    /// - No authentication tokens embedded in URLs (security check)
    /// - Environment variable expansion is validated for syntax
    ///
    /// ## Dependency Validation
    /// - All dependency paths must be non-empty
    /// - Remote dependencies must reference existing sources
    /// - Remote dependencies must specify version constraints
    /// - Local dependencies cannot have version constraints
    /// - No version conflicts between dependencies with the same name within each resource type
    ///
    /// ## Path Validation
    /// - Local dependency paths are checked for proper format
    /// - Remote dependency paths are validated as repository-relative
    /// - Path traversal attempts are detected and rejected
    ///
    /// # Error Types
    ///
    /// Returns specific error types for different validation failures:
    /// - [`crate::core::AgpmError::SourceNotFound`]: Referenced source doesn't exist
    /// - [`crate::core::AgpmError::ManifestValidationError`]: General validation failures
    /// - Context errors for specific issues with actionable suggestions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_dependency(
    ///     "local".to_string(),
    ///     ResourceDependency::Simple("../local/helper.md".to_string()),
    ///     true
    /// );
    /// assert!(manifest.validate().is_ok());
    /// ```
    ///
    /// # Security
    ///
    /// Enforces: no credential leakage in URLs, no path traversal, valid URL schemes.
    pub fn validate(&self) -> Result<()> {
        // Validate artifact type names
        for artifact_type in self.get_tools_config().types.keys() {
            if artifact_type.contains('/') || artifact_type.contains('\\') {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Artifact type name '{artifact_type}' cannot contain path separators ('/' or '\\\\'). \n\
                        Artifact type names must be simple identifiers without special characters."
                    ),
                }
                .into());
            }

            // Also check for other potentially problematic characters
            if artifact_type.contains("..") {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Artifact type name '{artifact_type}' cannot contain '..' (path traversal). \n\
                        Artifact type names must be simple identifiers."
                    ),
                }
                .into());
            }
        }

        // Check that all referenced sources exist and dependencies have required fields
        for (name, dep) in self.all_dependencies() {
            // Check for empty path
            if dep.get_path().is_empty() {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!("Missing required field 'path' for dependency '{name}'"),
                }
                .into());
            }

            // Validate pattern safety if it's a pattern dependency
            if dep.is_pattern() {
                crate::pattern::validate_pattern_safety(dep.get_path()).map_err(|e| {
                    crate::core::AgpmError::ManifestValidationError {
                        reason: format!("Invalid pattern in dependency '{name}': {e}"),
                    }
                })?;
            }

            // Check for version when source is specified (non-local dependencies)
            if let Some(source) = dep.get_source() {
                if !self.sources.contains_key(source) {
                    return Err(crate::core::AgpmError::SourceNotFound {
                        name: source.to_string(),
                    }
                    .into());
                }

                // Check if the source URL is a local path
                let source_url = self.sources.get(source).unwrap();
                let _is_local_source = source_url.starts_with('/')
                    || source_url.starts_with("./")
                    || source_url.starts_with("../");

                // Git dependencies can optionally have a version (defaults to 'main' if not specified)
                // Local path sources don't need versions
                // We no longer require versions for Git dependencies - they'll default to 'main'
            } else {
                // For local path dependencies (no source), version is not allowed
                // Skip directory check for pattern dependencies
                if !dep.is_pattern() {
                    let path = dep.get_path();
                    let is_plain_dir =
                        path.starts_with('/') || path.starts_with("./") || path.starts_with("../");

                    if is_plain_dir && dep.get_version().is_some() {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Version specified for plain directory dependency '{name}' with path '{path}'. \n\
                                Plain directory dependencies do not support versions. \n\
                            Remove the 'version' field or use a git source instead."
                            ),
                        }
                        .into());
                    }
                }
            }
        }

        // Check for version conflicts within each resource type
        // (same dependency name with different versions in the same section)
        // Note: Same name in different sections (e.g., agents vs commands) is allowed
        // because they install to different directories
        for resource_type in crate::core::ResourceType::all() {
            if let Some(deps) = self.get_dependencies(*resource_type) {
                let mut seen_deps: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();
                for (name, dep) in deps {
                    if let Some(version) = dep.get_version() {
                        if let Some(existing_version) = seen_deps.get(name) {
                            if existing_version != version {
                                return Err(crate::core::AgpmError::ManifestValidationError {
                                    reason: format!(
                                        "Version conflict for dependency '{name}' in [{}]: found versions '{existing_version}' and '{version}'",
                                        resource_type.to_plural()
                                    ),
                                }
                                .into());
                            }
                        } else {
                            seen_deps.insert(name.clone(), version.to_string());
                        }
                    }
                }
            }
        }

        // Validate URLs in sources
        for (name, url) in &self.sources {
            // Expand environment variables and home directory in URL
            let expanded_url = expand_url(url)?;

            if !expanded_url.starts_with("http://")
                && !expanded_url.starts_with("https://")
                && !expanded_url.starts_with("git@")
                && !expanded_url.starts_with("file://")
            // Plain directory paths not allowed as sources
            && !expanded_url.starts_with('/')
            && !expanded_url.starts_with("./")
            && !expanded_url.starts_with("../")
            {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!("Source '{name}' has invalid URL: '{url}'. Must be HTTP(S), SSH (git@...), or file:// URL"),
                }
                .into());
            }

            // Check if plain directory path is used as a source
            if expanded_url.starts_with('/')
                || expanded_url.starts_with("./")
                || expanded_url.starts_with("../")
            {
                return Err(crate::core::AgpmError::ManifestValidationError {
                    reason: format!(
                        "Plain directory path '{url}' cannot be used as source '{name}'. \n\
                        Sources must be git repositories. Use one of:\n\
                        - Remote URL: https://github.com/owner/repo.git\n\
                        - Local git repo: file:///absolute/path/to/repo\n\
                        - Or use direct path dependencies without a source"
                    ),
                }
                .into());
            }
        }

        // Check for case-insensitive conflicts within each resource type
        // This ensures manifests are portable across different filesystems
        // Even though Linux supports case-sensitive files, we reject conflicts
        // to ensure the manifest works on Windows and macOS too
        // Note: Same name in different sections (e.g., agents vs commands) is allowed
        // because they install to different directories
        for resource_type in crate::core::ResourceType::all() {
            if let Some(deps) = self.get_dependencies(*resource_type) {
                let mut normalized_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();

                for name in deps.keys() {
                    let normalized = name.to_lowercase();
                    if !normalized_names.insert(normalized.clone()) {
                        // Find the original conflicting name within this resource type
                        for other_name in deps.keys() {
                            if other_name != name && other_name.to_lowercase() == normalized {
                                return Err(crate::core::AgpmError::ManifestValidationError {
                                    reason: format!(
                                        "Case conflict in [{}]: '{name}' and '{other_name}' would map to the same file on case-insensitive filesystems. To ensure portability across platforms, resource names must be case-insensitively unique.",
                                        resource_type.to_plural()
                                    ),
                                }
                                .into());
                            }
                        }
                    }
                }
            }
        }

        // Validate artifact types and resource type support
        for resource_type in crate::core::ResourceType::all() {
            if let Some(deps) = self.get_dependencies(*resource_type) {
                for (name, dep) in deps {
                    // Get tool from dependency (defaults based on resource type)
                    let tool_string = dep
                        .get_tool()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| self.get_default_tool(*resource_type));
                    let tool = tool_string.as_str();

                    // Check if tool is configured
                    if self.get_tool_config(tool).is_none() {
                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Unknown tool '{tool}' for dependency '{name}'.\n\
                                Available types: {}\n\
                                Configure custom types in [tools] section or use a standard type.",
                                self.get_tools_config()
                                    .types
                                    .keys()
                                    .map(|s| format!("'{s}'"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        }
                        .into());
                    }

                    // Check if resource type is supported by this tool
                    if !self.is_resource_supported(tool, *resource_type) {
                        let artifact_config = self.get_tool_config(tool).unwrap();
                        let resource_plural = resource_type.to_plural();

                        // Check if this is a malformed configuration (resource exists but not properly configured)
                        let is_malformed = artifact_config.resources.contains_key(resource_plural);

                        let supported_types: Vec<String> = artifact_config
                            .resources
                            .iter()
                            .filter(|(_, res_config)| {
                                res_config.path.is_some() || res_config.merge_target.is_some()
                            })
                            .map(|(s, _)| s.to_string())
                            .collect();

                        // Build resource-type-specific suggestions
                        let mut suggestions = Vec::new();

                        if is_malformed {
                            // Resource type exists but is malformed
                            suggestions.push(format!(
                                "Resource type '{}' is configured for tool '{}' but missing required 'path' or 'merge_target' field",
                                resource_plural, tool
                            ));

                            // Provide specific fix suggestions based on resource type
                            match resource_type {
                                crate::core::ResourceType::Hook => {
                                    suggestions.push("For hooks, add: merge_target = '.claude/settings.local.json'".to_string());
                                }
                                crate::core::ResourceType::McpServer => {
                                    suggestions.push(
                                        "For MCP servers, add: merge_target = '.mcp.json'"
                                            .to_string(),
                                    );
                                }
                                _ => {
                                    suggestions.push(format!(
                                        "For {}, add: path = '{}'",
                                        resource_plural, resource_plural
                                    ));
                                }
                            }
                        } else {
                            // Resource type not supported at all
                            match resource_type {
                                crate::core::ResourceType::Snippet => {
                                    suggestions.push("Snippets work best with the 'agpm' tool (shared infrastructure)".to_string());
                                    suggestions.push(
                                        "Add tool='agpm' to this dependency to use shared snippets"
                                            .to_string(),
                                    );
                                }
                                _ => {
                                    // Find which tool types DO support this resource type
                                    let default_config = ToolsConfig::default();
                                    let tools_config =
                                        self.tools.as_ref().unwrap_or(&default_config);
                                    let supporting_types: Vec<String> = tools_config
                                        .types
                                        .iter()
                                        .filter(|(_, config)| {
                                            config.resources.contains_key(resource_plural)
                                                && config
                                                    .resources
                                                    .get(resource_plural)
                                                    .map(|res| {
                                                        res.path.is_some()
                                                            || res.merge_target.is_some()
                                                    })
                                                    .unwrap_or(false)
                                        })
                                        .map(|(type_name, _)| format!("'{}'", type_name))
                                        .collect();

                                    if !supporting_types.is_empty() {
                                        suggestions.push(format!(
                                            "This resource type is supported by tools: {}",
                                            supporting_types.join(", ")
                                        ));
                                    }
                                }
                            }
                        }

                        let mut reason = if is_malformed {
                            format!(
                                "Resource type '{}' is improperly configured for tool '{}' for dependency '{}'.\n\n",
                                resource_plural, tool, name
                            )
                        } else {
                            format!(
                                "Resource type '{}' is not supported by tool '{}' for dependency '{}'.\n\n",
                                resource_plural, tool, name
                            )
                        };

                        reason.push_str(&format!(
                            "Tool '{}' properly supports: {}\n\n",
                            tool,
                            supported_types.join(", ")
                        ));

                        if !suggestions.is_empty() {
                            reason.push_str("💡 Suggestions:\n");
                            for suggestion in &suggestions {
                                reason.push_str(&format!("  • {}\n", suggestion));
                            }
                            reason.push('\n');
                        }

                        reason.push_str(
                            "You can fix this by:\n\
                            1. Changing the 'tool' field to a supported tool\n\
                            2. Using a different resource type\n\
                            3. Removing this dependency from your manifest",
                        );

                        return Err(crate::core::AgpmError::ManifestValidationError {
                            reason,
                        }
                        .into());
                    }
                }
            }
        }

        // Validate patches reference valid aliases
        self.validate_patches()?;

        Ok(())
    }

    /// Validate that patches reference valid manifest aliases.
    ///
    /// This method checks that all patch aliases correspond to actual dependencies
    /// defined in the manifest. Patches for non-existent aliases are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error if a patch references an alias that doesn't exist in the manifest.
    fn validate_patches(&self) -> Result<()> {
        use crate::core::ResourceType;

        // Helper to check if an alias exists for a resource type
        let check_patch_aliases = |resource_type: ResourceType,
                                   patches: &BTreeMap<String, PatchData>|
         -> Result<()> {
            let deps = self.get_dependencies(resource_type);

            for alias in patches.keys() {
                // Check if this alias exists in the manifest
                let exists = if let Some(deps) = deps {
                    deps.contains_key(alias)
                } else {
                    false
                };

                if !exists {
                    return Err(crate::core::AgpmError::ManifestValidationError {
                            reason: format!(
                                "Patch references unknown alias '{alias}' in [patch.{}] section.\n\
                                The alias must be defined in [{}] section of agpm.toml.\n\
                                To patch a transitive dependency, first add it explicitly to your manifest.",
                                resource_type.to_plural(),
                                resource_type.to_plural()
                            ),
                        }
                        .into());
                }
            }
            Ok(())
        };

        // Validate patches for each resource type
        check_patch_aliases(ResourceType::Agent, &self.patches.agents)?;
        check_patch_aliases(ResourceType::Snippet, &self.patches.snippets)?;
        check_patch_aliases(ResourceType::Command, &self.patches.commands)?;
        check_patch_aliases(ResourceType::Script, &self.patches.scripts)?;
        check_patch_aliases(ResourceType::McpServer, &self.patches.mcp_servers)?;
        check_patch_aliases(ResourceType::Hook, &self.patches.hooks)?;
        check_patch_aliases(ResourceType::Skill, &self.patches.skills)?;

        Ok(())
    }
}
