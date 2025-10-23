//! Lockfile validation and staleness detection.
//!
//! This module provides validation logic to ensure lockfiles are consistent with
//! manifests, detect corruption, and identify when lockfiles need regeneration.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::{LockFile, StalenessReason};

impl LockFile {
    /// Validate lockfile against manifest for staleness detection.
    ///
    /// Checks consistency and detects staleness indicators requiring regeneration.
    /// Similar to Cargo's `--locked` mode.
    ///
    /// # Arguments
    ///
    /// * `manifest` - The current project manifest to validate against
    /// * `strict` - If true, check version/path changes; if false, only check corruption and security
    ///
    /// # Returns
    ///
    /// * `Ok(None)` - Lockfile is valid and up-to-date
    /// * `Ok(Some(StalenessReason))` - Lockfile is stale and needs regeneration
    /// * `Err(anyhow::Error)` - Validation failed due to IO or parse error
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::manifest::Manifest;
    /// # fn example() -> anyhow::Result<()> {
    /// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
    /// let manifest = Manifest::load(Path::new("agpm.toml"))?;
    ///
    /// // Strict mode: check everything including version/path changes
    /// match lockfile.validate_against_manifest(&manifest, true)? {
    ///     None => println!("Lockfile is valid"),
    ///     Some(reason) => {
    ///         eprintln!("Lockfile is stale: {}", reason);
    ///         eprintln!("Run 'agpm install' to auto-update it");
    ///     }
    /// }
    ///
    /// // Lenient mode: only check corruption and security (for --frozen)
    /// match lockfile.validate_against_manifest(&manifest, false)? {
    ///     None => println!("Lockfile has no critical issues"),
    ///     Some(reason) => eprintln!("Critical issue: {}", reason),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Staleness Detection
    ///
    /// The method checks for several staleness indicators:
    /// - **Duplicate entries**: Multiple entries for the same dependency (corruption) - always checked
    /// - **Source URL changes**: Source URLs changed in manifest (security concern) - always checked
    /// - **Missing dependencies**: Manifest has deps not in lockfile - only in strict mode
    /// - **Version changes**: Same dependency with different version constraint - only in strict mode
    /// - **Path changes**: Same dependency with different source path - only in strict mode
    ///
    /// Note: Extra lockfile entries are allowed (for transitive dependencies).
    pub fn validate_against_manifest(
        &self,
        manifest: &crate::manifest::Manifest,
        strict: bool,
    ) -> Result<Option<StalenessReason>> {
        // Always check for critical issues:
        // 1. Corruption (duplicate entries)
        // 2. Security concerns (source URL changes)

        // Check for duplicate entries within the lockfile (corruption)
        if let Some(reason) = self.detect_duplicate_entries()? {
            return Ok(Some(reason));
        }

        // Check source URL changes (security concern - different repository)
        for (source_name, manifest_url) in &manifest.sources {
            if let Some(locked_source) = self.get_source(source_name)
                && &locked_source.url != manifest_url
            {
                return Ok(Some(StalenessReason::SourceUrlChanged {
                    name: source_name.clone(),
                    old_url: locked_source.url.clone(),
                    new_url: manifest_url.clone(),
                }));
            }
        }

        // In strict mode, also check for missing dependencies, version changes, and path changes
        if strict {
            for resource_type in crate::core::ResourceType::all() {
                if let Some(manifest_deps) = manifest.get_dependencies(*resource_type) {
                    for (name, dep) in manifest_deps {
                        // Find matching resource in lockfile
                        let locked_resource = self.get_resource(name);

                        if locked_resource.is_none() {
                            // Dependency is in manifest but not in lockfile
                            return Ok(Some(StalenessReason::MissingDependency {
                                name: name.clone(),
                                resource_type: *resource_type,
                            }));
                        }

                        // Check for version changes
                        if let Some(locked) = locked_resource {
                            if let Some(manifest_version) = dep.get_version()
                                && let Some(locked_version) = &locked.version
                                && manifest_version != locked_version
                            {
                                return Ok(Some(StalenessReason::VersionChanged {
                                    name: name.clone(),
                                    resource_type: *resource_type,
                                    old_version: locked_version.clone(),
                                    new_version: manifest_version.to_string(),
                                }));
                            }

                            // Check for path changes
                            if dep.get_path() != locked.path {
                                return Ok(Some(StalenessReason::PathChanged {
                                    name: name.clone(),
                                    resource_type: *resource_type,
                                    old_path: locked.path.clone(),
                                    new_path: dep.get_path().to_string(),
                                }));
                            }

                            // Check for tool changes (apply defaults if not specified)
                            let manifest_tool_string = dep
                                .get_tool()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| manifest.get_default_tool(*resource_type));
                            let manifest_tool = manifest_tool_string.as_str();
                            let locked_tool = locked.tool.as_deref().unwrap_or("claude-code");
                            if manifest_tool != locked_tool {
                                return Ok(Some(StalenessReason::ToolChanged {
                                    name: name.clone(),
                                    resource_type: *resource_type,
                                    old_tool: locked_tool.to_string(),
                                    new_tool: manifest_tool.to_string(),
                                }));
                            }
                        }
                    }
                }
            }
        }

        // Extra lockfile entries are allowed (for transitive dependencies)
        Ok(None)
    }

    /// Check if lockfile is stale (boolean convenience method).
    ///
    /// Returns simple bool instead of detailed `StalenessReason`.
    ///
    /// # Arguments
    ///
    /// * `manifest` - The current project manifest to validate against
    /// * `strict` - If true, check version/path changes; if false, only check corruption and security
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Lockfile is stale and needs updating
    /// * `Ok(false)` - Lockfile is valid and up-to-date
    /// * `Err(anyhow::Error)` - Validation failed due to IO or parse error
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::manifest::Manifest;
    /// # fn example() -> anyhow::Result<()> {
    /// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
    /// let manifest = Manifest::load(Path::new("agpm.toml"))?;
    ///
    /// if lockfile.is_stale(&manifest, true)? {
    ///     println!("Lockfile needs updating");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_stale(&self, manifest: &crate::manifest::Manifest, strict: bool) -> Result<bool> {
        Ok(self.validate_against_manifest(manifest, strict)?.is_some())
    }

    /// Detect duplicate entries indicating corruption.
    ///
    /// Scans all resource types for duplicate names.
    pub(crate) fn detect_duplicate_entries(&self) -> Result<Option<StalenessReason>> {
        // Check each resource type for duplicates
        for resource_type in crate::core::ResourceType::all() {
            let resources = self.get_resources(*resource_type);
            let mut seen_names = HashMap::new();

            for resource in resources {
                if seen_names.contains_key(&resource.name) {
                    return Ok(Some(StalenessReason::DuplicateEntries {
                        name: resource.name.clone(),
                        resource_type: *resource_type,
                        count: resources.iter().filter(|r| r.name == resource.name).count(),
                    }));
                }
                seen_names.insert(&resource.name, 0);
            }
        }

        Ok(None)
    }

    /// Validate no duplicate names within each resource type.
    ///
    /// Stricter than `detect_duplicate_entries`. Used during loading to catch
    /// corruption early.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the lockfile (used for error messages)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - No duplicates found
    /// * `Err(anyhow::Error)` - Duplicates found with detailed error message
    ///
    /// # Errors
    ///
    /// Returns an error if any resource type contains duplicate names, with
    /// details about which resource type and names are duplicated.
    pub fn validate_no_duplicates(&self, path: &Path) -> Result<()> {
        let mut found_duplicates = false;
        let mut error_messages = Vec::new();

        // Check each resource type for duplicates
        for resource_type in crate::core::ResourceType::all() {
            let resources = self.get_resources(*resource_type);
            let mut name_counts = HashMap::new();

            // Count occurrences of each name
            for resource in resources {
                *name_counts.entry(&resource.name).or_insert(0) += 1;
            }

            // Find duplicates
            let duplicates: Vec<_> = name_counts.iter().filter(|(_, count)| **count > 1).collect();

            if !duplicates.is_empty() {
                found_duplicates = true;
                let dup_names: Vec<_> = duplicates
                    .iter()
                    .map(|(name, count)| format!("{} ({} times)", name, **count))
                    .collect();
                error_messages.push(format!("  {}: {}", resource_type, dup_names.join(", ")));
            }
        }

        if found_duplicates {
            return Err(crate::core::AgpmError::Other {
                message: format!(
                    "Lockfile corruption detected in {}:\nDuplicate resource names found:\n{}\n\n\
                    This indicates lockfile corruption. To fix:\n\
                    - Delete agpm.lock and run 'agpm install' to regenerate it\n\
                    - Or restore from a backup if available",
                    path.display(),
                    error_messages.join("\n")
                ),
            }
            .into());
        }

        Ok(())
    }
}
