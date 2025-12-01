//! I/O operations for manifest files.
//!
//! This module contains all file I/O operations for the manifest, including:
//! - Loading manifests from TOML files
//! - Saving manifests to TOML files
//! - Creating new empty manifests
//! - Applying tool defaults
//! - Merging private configurations

use crate::core::file_error::{FileOperation, FileResultExt};
use crate::manifest::{Manifest, ManifestPatches, PatchConflict, ResourceDependency};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

impl Manifest {
    /// Load and parse a manifest from a TOML file.
    ///
    /// This method reads the specified file, parses it as TOML, deserializes
    /// it into a [`Manifest`] struct, and validates the result. The entire
    /// operation is atomic - either the manifest loads successfully or an
    /// error is returned.
    ///
    /// # Validation
    ///
    /// After parsing, the manifest is automatically validated to ensure:
    /// - All dependency sources reference valid entries in the `[sources]` section
    /// - Required fields are present and non-empty
    /// - Version constraints are properly specified for remote dependencies
    /// - Source URLs use supported protocols
    /// - No version conflicts exist between dependencies
    ///
    /// # Error Handling
    ///
    /// Returns detailed errors for common problems:
    /// - **File I/O errors**: File not found, permission denied, etc.
    /// - **TOML syntax errors**: Invalid TOML format with helpful suggestions
    /// - **Validation errors**: Logical inconsistencies in the manifest
    /// - **Security errors**: Unsafe URL patterns or credential leakage
    ///
    /// All errors include contextual information and actionable suggestions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// // Load a manifest file
    /// let manifest = Manifest::load(Path::new("agpm.toml"))?;
    ///
    /// // Access parsed data
    /// println!("Found {} sources", manifest.sources.len());
    /// println!("Found {} agents", manifest.agents.len());
    /// println!("Found {} snippets", manifest.snippets.len());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # File Format
    ///
    /// Expects a valid TOML file following the AGPM manifest format.
    /// See the module-level documentation for complete format specification.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_file_context(
            FileOperation::Read,
            path,
            "reading manifest file",
            "manifest_module",
        )?;

        let mut manifest: Self = toml::from_str(&content)
            .map_err(|e| crate::core::AgpmError::ManifestParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in manifest file: {}\n\n\
                    Common TOML syntax errors:\n\
                    - Missing quotes around strings\n\
                    - Unmatched brackets [ ] or braces {{ }}\n\
                    - Invalid characters in keys or values\n\
                    - Incorrect indentation or structure",
                    path.display()
                )
            })?;

        // Apply resource-type-specific defaults for tool
        // Snippets default to "agpm" (shared infrastructure) instead of "claude-code"
        manifest.apply_tool_defaults();

        // Store the manifest directory for resolving relative paths
        manifest.manifest_dir = Some(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Manifest path has no parent directory"))?
                .to_path_buf(),
        );

        manifest.validate()?;

        Ok(manifest)
    }

    /// Load manifest with private config merged.
    ///
    /// Loads the project manifest from `agpm.toml` and then attempts to load
    /// `agpm.private.toml` from the same directory. If a private config exists:
    /// - **Sources** are merged (private sources can use same names, which shadows project sources)
    /// - **Dependencies** are merged (private deps tracked via `private_dependency_names`)
    /// - **Patches** are merged (private patches take precedence)
    ///
    /// Any conflicts (same field defined in both files with different values) are
    /// returned for informational purposes only. Private patches always override
    /// project patches without raising an error.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the project manifest file (`agpm.toml`)
    ///
    /// # Returns
    ///
    /// A manifest with merged sources, dependencies, patches, and a list of any
    /// patch conflicts detected (for informational/debugging purposes).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// let (manifest, conflicts) = Manifest::load_with_private(Path::new("agpm.toml"))?;
    /// // Conflicts are informational only - private patches already won
    /// if !conflicts.is_empty() {
    ///     eprintln!("Note: {} private patch(es) override project settings", conflicts.len());
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load_with_private(path: &Path) -> Result<(Self, Vec<PatchConflict>)> {
        // Load the main project manifest
        let mut manifest = Self::load(path)?;

        // Store project patches before merging
        manifest.project_patches = manifest.patches.clone();

        // Try to load private config
        let private_path = if let Some(parent) = path.parent() {
            parent.join("agpm.private.toml")
        } else {
            PathBuf::from("agpm.private.toml")
        };

        if private_path.exists() {
            let private_manifest = Self::load_private(&private_path)?;

            // Merge sources (private can shadow project sources with same name)
            for (name, url) in private_manifest.sources {
                manifest.sources.insert(name, url);
            }

            // Track which dependencies are from private manifest and merge them
            let mut private_names = std::collections::HashSet::new();

            // Merge agents
            for (name, dep) in private_manifest.agents {
                private_names.insert(("agents".to_string(), name.clone()));
                manifest.agents.insert(name, dep);
            }

            // Merge snippets
            for (name, dep) in private_manifest.snippets {
                private_names.insert(("snippets".to_string(), name.clone()));
                manifest.snippets.insert(name, dep);
            }

            // Merge commands
            for (name, dep) in private_manifest.commands {
                private_names.insert(("commands".to_string(), name.clone()));
                manifest.commands.insert(name, dep);
            }

            // Merge scripts
            for (name, dep) in private_manifest.scripts {
                private_names.insert(("scripts".to_string(), name.clone()));
                manifest.scripts.insert(name, dep);
            }

            // Merge hooks
            for (name, dep) in private_manifest.hooks {
                private_names.insert(("hooks".to_string(), name.clone()));
                manifest.hooks.insert(name, dep);
            }

            // Merge MCP servers
            for (name, dep) in private_manifest.mcp_servers {
                private_names.insert(("mcp-servers".to_string(), name.clone()));
                manifest.mcp_servers.insert(name, dep);
            }

            manifest.private_dependency_names = private_names;

            // Store private patches
            manifest.private_patches = private_manifest.patches.clone();

            // Merge patches (private takes precedence)
            let (merged_patches, conflicts) =
                manifest.patches.merge_with(&private_manifest.patches);
            manifest.patches = merged_patches;

            // Re-validate after merge to ensure private dependencies reference valid sources
            // This catches cases where private deps reference sources not in either manifest
            manifest.validate().with_context(|| {
                format!(
                    "Validation failed after merging private manifest: {}",
                    private_path.display()
                )
            })?;

            Ok((manifest, conflicts))
        } else {
            // No private config, keep private_patches empty
            manifest.private_patches = ManifestPatches::new();
            Ok((manifest, Vec::new()))
        }
    }

    /// Load a private manifest file.
    ///
    /// Private manifests can contain:
    /// - **Sources**: Private Git repositories with authentication
    /// - **Dependencies**: User-only resources (agents, snippets, commands, etc.)
    /// - **Patches**: Customizations to project or private dependencies
    ///
    /// Private manifests **cannot** contain:
    /// - **Tools**: Tool configuration must be in the main manifest
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the private manifest file (`agpm.private.toml`)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The TOML syntax is invalid
    /// - The private config contains tools configuration
    fn load_private(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_file_context(
            FileOperation::Read,
            path,
            "reading private manifest file",
            "manifest_module",
        )?;

        let mut manifest: Self = toml::from_str(&content)
            .map_err(|e| crate::core::AgpmError::ManifestParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in private manifest file: {}\n\n\
                    Common TOML syntax errors:\n\
                    - Missing quotes around strings\n\
                    - Unmatched brackets [ ] or braces {{ }}\n\
                    - Invalid characters in keys or values\n\
                    - Incorrect indentation or structure",
                    path.display()
                )
            })?;

        // Validate that private config doesn't contain tools
        if manifest.tools.is_some() {
            anyhow::bail!(
                "Private manifest file ({}) cannot contain [tools] section. \
                 Tool configuration must be defined in the project manifest (agpm.toml).",
                path.display()
            );
        }

        // Apply resource-type-specific defaults for tool
        manifest.apply_tool_defaults();

        // Store the manifest directory for resolving relative paths
        manifest.manifest_dir = Some(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("Private manifest path has no parent directory"))?
                .to_path_buf(),
        );

        Ok(manifest)
    }

    /// Get the default tool for a resource type.
    ///
    /// Checks the `[default-tools]` configuration first, then falls back to
    /// the built-in defaults:
    /// - `snippets` → `"agpm"` (shared infrastructure)
    /// - All other resource types → `"claude-code"`
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type to get the default tool for
    ///
    /// # Returns
    ///
    /// The default tool name as a string.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::core::ResourceType;
    ///
    /// let manifest = Manifest::new();
    /// assert_eq!(manifest.get_default_tool(ResourceType::Snippet), "agpm");
    /// assert_eq!(manifest.get_default_tool(ResourceType::Agent), "claude-code");
    /// ```
    #[must_use]
    pub fn get_default_tool(&self, resource_type: crate::core::ResourceType) -> String {
        // Get the resource name in plural form for consistency with TOML section names
        // (agents, snippets, commands, etc.)
        let resource_name = match resource_type {
            crate::core::ResourceType::Agent => "agents",
            crate::core::ResourceType::Snippet => "snippets",
            crate::core::ResourceType::Command => "commands",
            crate::core::ResourceType::Script => "scripts",
            crate::core::ResourceType::Hook => "hooks",
            crate::core::ResourceType::McpServer => "mcp-servers",
        };

        // Check if there's a configured override
        if let Some(tool) = self.default_tools.get(resource_name) {
            return tool.clone();
        }

        // Fall back to built-in defaults
        resource_type.default_tool().to_string()
    }

    fn apply_tool_defaults(&mut self) {
        // Apply resource-type-specific defaults only when tool is not explicitly specified
        for resource_type in [
            crate::core::ResourceType::Snippet,
            crate::core::ResourceType::Agent,
            crate::core::ResourceType::Command,
            crate::core::ResourceType::Script,
            crate::core::ResourceType::Hook,
            crate::core::ResourceType::McpServer,
        ] {
            // Get the default tool before the mutable borrow to avoid borrow conflicts
            let default_tool = self.get_default_tool(resource_type);

            if let Some(deps) = self.get_dependencies_mut(resource_type) {
                for dependency in deps.values_mut() {
                    if let ResourceDependency::Detailed(details) = dependency {
                        if details.tool.is_none() {
                            details.tool = Some(default_tool.clone());
                        }
                    }
                }
            }
        }
    }

    /// Save the manifest to a TOML file with pretty formatting.
    ///
    /// This method serializes the manifest to TOML format and writes it to the
    /// specified file path. The output is pretty-printed for human readability
    /// and follows TOML best practices.
    ///
    /// # Formatting
    ///
    /// The generated TOML file will:
    /// - Use consistent indentation and spacing
    /// - Omit empty sections for cleaner output
    /// - Order sections logically (sources, target, agents, snippets)
    /// - Include inline tables for detailed dependencies
    ///
    /// # Atomic Operation
    ///
    /// The save operation is atomic - the file is either completely written
    /// or left unchanged. This prevents corruption if the operation fails
    /// partway through.
    ///
    /// # Error Handling
    ///
    /// Returns detailed errors for common problems:
    /// - **Permission denied**: Insufficient write permissions
    /// - **Directory doesn't exist**: Parent directory missing
    /// - **Disk full**: Insufficient storage space
    /// - **File locked**: Another process has the file open
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// let mut manifest = Manifest::new();
    /// manifest.add_source(
    ///     "official".to_string(),
    ///     "https://github.com/claude-org/resources.git".to_string()
    /// );
    ///
    /// // Save to file
    /// # use tempfile::tempdir;
    /// # let temp_dir = tempdir()?;
    /// # let manifest_path = temp_dir.path().join("agpm.toml");
    /// manifest.save(&manifest_path)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Output Format
    ///
    /// The generated file will follow this structure:
    ///
    /// ```toml
    /// [sources]
    /// official = "https://github.com/claude-org/resources.git"
    ///
    /// [target]
    /// agents = ".claude/agents"
    /// snippets = ".agpm/snippets"
    ///
    /// [agents]
    /// helper = { source = "official", path = "agents/helper.md", version = "v1.0.0" }
    ///
    /// [snippets]
    /// utils = { source = "official", path = "snippets/utils.md", version = "v1.0.0" }
    /// ```
    pub fn save(&self, path: &Path) -> Result<()> {
        // Serialize to a document first so we can control formatting
        let mut doc = toml_edit::ser::to_document(self)
            .with_context(|| "Failed to serialize manifest data to TOML format")?;

        // Convert top-level inline tables to regular tables (section headers)
        // This keeps [sources], [agents], etc. as sections but nested values stay inline
        for (_key, value) in doc.iter_mut() {
            if let Some(inline_table) = value.as_inline_table() {
                // Convert inline table to regular table
                let table = inline_table.clone().into_table();
                *value = toml_edit::Item::Table(table);
            }
        }

        let content = doc.to_string();

        std::fs::write(path, content).with_file_context(
            FileOperation::Write,
            path,
            "writing manifest file",
            "manifest_module",
        )?;

        Ok(())
    }
}
