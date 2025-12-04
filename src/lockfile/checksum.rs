//! Checksum computation and verification for lockfile integrity.
//!
//! This module provides SHA-256 checksum operations for verifying file integrity,
//! detecting corruption, and ensuring reproducible installations.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::{LockFile, ResourceId};

impl LockFile {
    /// Compute SHA-256 checksum for file integrity verification.
    ///
    /// Detects corruption, tampering, or changes after installation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - File read error with detailed context
    ///
    /// # Checksum Format
    ///
    /// The returned checksum follows the format:
    /// - **Algorithm prefix**: "sha256:"
    /// - **Hash encoding**: Lowercase hexadecimal
    /// - **Length**: 71 characters total (7 for prefix + 64 hex digits)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let checksum = LockFile::compute_checksum(Path::new("example.md"))?;
    /// println!("File checksum: {}", checksum);
    /// // Output: "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// Provides detailed error context for common issues:
    /// - **File not found**: Suggests checking the path
    /// - **Permission denied**: Suggests checking file permissions
    /// - **IO errors**: Suggests checking disk health or file locks
    ///
    /// # Security Considerations
    ///
    /// - Uses SHA-256, a cryptographically secure hash function
    /// - Suitable for integrity verification and tamper detection
    /// - Consistent across platforms (Windows, macOS, Linux)
    /// - Not affected by line ending differences (hashes actual bytes)
    ///
    /// # Performance
    ///
    /// The method reads the entire file into memory before hashing.
    /// For very large files (>100MB), consider streaming implementations
    /// in future versions.
    pub fn compute_checksum(path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let content = fs::read(path).with_context(|| {
            format!(
                "Cannot read file for checksum calculation: {}\n\n\
                    This error occurs when verifying file integrity.\n\
                    Check that the file exists and is readable.",
                path.display()
            )
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = hasher.finalize();

        Ok(format!("sha256:{}", hex::encode(result)))
    }

    /// Compute SHA-256 checksum for a directory (skill resources).
    ///
    /// Calculates a combined checksum of all files in a directory by concatenating
    /// their individual checksums in sorted order. This provides a deterministic
    /// checksum that changes when any file in the directory changes.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Combined checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - Directory read or file hash error
    ///
    /// # Algorithm
    ///
    /// 1. Walk directory recursively (files only, not directories)
    /// 2. Compute SHA-256 of each file
    /// 3. Sort file paths for deterministic ordering
    /// 4. Concatenate all checksums with file paths
    /// 5. Compute final SHA-256 of the concatenated data
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let checksum = LockFile::compute_directory_checksum(Path::new("my-skill"))?;
    /// println!("Directory checksum: {}", checksum);
    /// # Ok(())
    /// # }
    /// ```
    pub fn compute_directory_checksum(path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};
        use walkdir::WalkDir;

        let mut file_hashes: Vec<(String, String)> = Vec::new();

        for entry in WalkDir::new(path).follow_links(false) {
            let entry = entry.with_context(|| {
                format!("Failed to read directory entry in: {}", path.display())
            })?;

            if entry.file_type().is_file() {
                let file_path = entry.path();
                // Use normalize_path_for_storage for cross-platform deterministic checksums
                let relative_path = crate::utils::normalize_path_for_storage(
                    file_path.strip_prefix(path).unwrap_or(file_path),
                );

                let file_checksum = Self::compute_checksum(file_path)?;
                file_hashes.push((relative_path, file_checksum));
            }
        }

        // Sort by relative path for deterministic ordering
        file_hashes.sort_by(|a, b| a.0.cmp(&b.0));

        // Concatenate all checksums with their paths
        let mut hasher = Sha256::new();
        for (path, checksum) in &file_hashes {
            hasher.update(format!("{}:{}\n", path, checksum).as_bytes());
        }

        let result = hasher.finalize();
        Ok(format!("sha256:{}", hex::encode(result)))
    }

    /// Verify file matches expected checksum.
    ///
    /// Computes current checksum and compares with expected value.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to verify
    /// * `expected` - Expected checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - File checksum matches expected value
    /// * `Ok(false)` - File checksum does not match (corruption detected)
    /// * `Err(anyhow::Error)` - File read error or checksum calculation failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let expected = "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3";
    /// let is_valid = LockFile::verify_checksum(Path::new("example.md"), expected)?;
    ///
    /// if is_valid {
    ///     println!("File integrity verified");
    /// } else {
    ///     println!("WARNING: File has been modified or corrupted!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Installation verification**: Ensure copied files are intact
    /// - **Periodic validation**: Detect file corruption over time
    /// - **Security checks**: Detect unauthorized modifications
    /// - **Troubleshooting**: Diagnose installation issues
    ///
    /// # Performance
    ///
    /// This method internally calls [`compute_checksum`](Self::compute_checksum),
    /// so it has the same performance characteristics. For bulk verification
    /// operations, consider caching computed checksums.
    ///
    /// # Security
    ///
    /// The comparison is performed using standard string equality, which is
    /// not timing-attack resistant. Since checksums are not secrets, this
    /// is acceptable for integrity verification purposes.
    pub fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
        let actual = Self::compute_checksum(path)?;
        Ok(actual == expected)
    }

    /// Update checksum for resource identified by ResourceId.
    ///
    /// Used after installation to record actual file checksum. ResourceId ensures unique
    /// identification via name, source, tool, and template_vars.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for the resource
    /// * `checksum` - The new SHA-256 checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, LockedResourceBuilder, ResourceId};
    /// # use agpm_cli::core::ResourceType;
    /// # use agpm_cli::utils::compute_variant_inputs_hash;
    /// # let mut lockfile = LockFile::default();
    /// # // First add a resource to update
    /// # let resource = LockedResourceBuilder::new(
    /// #     "my-agent".to_string(),
    /// #     "my-agent.md".to_string(),
    /// #     "".to_string(),
    /// #     "agents/my-agent.md".to_string(),
    /// #     ResourceType::Agent,
    /// # )
    /// # .tool(Some("claude-code".to_string()))
    /// # .build();
    /// # lockfile.add_typed_resource("my-agent".to_string(), resource, ResourceType::Agent);
    /// let variant_hash = compute_variant_inputs_hash(&serde_json::json!({})).unwrap_or_default();
    /// let id = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, variant_hash);
    /// let updated = lockfile.update_resource_checksum(&id, "sha256:abcdef123456...");
    /// assert!(updated);
    /// ```
    pub fn update_resource_checksum(&mut self, id: &ResourceId, checksum: &str) -> bool {
        // Try each resource type until we find a match by comparing ResourceIds
        for resource in &mut self.agents {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.id() == *id {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        false
    }

    /// Update context checksum for resource by ResourceId.
    ///
    /// Stores the SHA-256 checksum of template rendering inputs (context) in the lockfile.
    /// This is different from the file checksum which covers the final rendered content.
    ///
    /// # Arguments
    ///
    /// * `id` - The ResourceId identifying the resource to update
    /// * `context_checksum` - The SHA-256 checksum of template context, or None for non-templated resources
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut lockfile = LockFile::new();
    /// let id = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, serde_json::json!({}));
    /// let updated = lockfile.update_resource_context_checksum(&id, Some("sha256:context123456..."));
    /// assert!(updated);
    /// ```
    pub fn update_resource_context_checksum(
        &mut self,
        id: &ResourceId,
        context_checksum: &str,
    ) -> bool {
        // Try each resource type until we find a match by comparing ResourceIds
        for resource in &mut self.agents {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.id() == *id {
                resource.context_checksum = Some(context_checksum.to_string());
                return true;
            }
        }

        false
    }

    /// Update applied patches for resource by name.
    ///
    /// Stores project patches in main lockfile; private patches go to agpm.private.lock.
    /// Takes `AppliedPatches` from installer.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the resource to update
    /// * `applied_patches` - The patches that were applied (from `AppliedPatches` struct)
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use agpm_cli::lockfile::LockFile;
    /// # use agpm_cli::manifest::patches::AppliedPatches;
    /// # use std::collections::HashMap;
    /// # let mut lockfile = LockFile::new();
    /// let mut applied = AppliedPatches::new();
    /// applied.project.insert("model".to_string(), toml::Value::String("haiku".into()));
    ///
    /// let updated = lockfile.update_resource_applied_patches("my-agent", &applied);
    /// assert!(updated);
    /// ```
    pub fn update_resource_applied_patches(
        &mut self,
        name: &str,
        applied_patches: &crate::manifest::patches::AppliedPatches,
    ) -> bool {
        // Store ONLY project patches in the main lockfile (agpm.lock)
        // Private patches are stored separately in agpm.private.lock
        // This ensures the main lockfile is deterministic and safe to commit
        let project_patches = applied_patches.project.clone();

        // Try each resource type until we find a match
        for resource in &mut self.agents {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        for resource in &mut self.skills {
            if resource.name == name {
                resource.applied_patches = project_patches;
                return true;
            }
        }

        false
    }

    /// Apply installation results to the lockfile in batch.
    ///
    /// Updates the lockfile with checksums, context checksums, and applied patches
    /// from the installation process. This consolidates three separate update operations
    /// into one batch call, reducing code duplication between install and update commands.
    ///
    /// # Batch Processing Pattern
    ///
    /// This function processes three parallel vectors of installation results:
    /// 1. **File checksums** - SHA-256 of rendered content (triggers reinstall if changed)
    /// 2. **Context checksums** - SHA-256 of template inputs (audit/debug only)
    /// 3. **Applied patches** - Tracks which project patches were applied to each resource
    ///
    /// The batch approach ensures all three updates are applied consistently and
    /// atomically to the lockfile, avoiding partial state.
    ///
    /// # Arguments
    ///
    /// * `checksums` - File checksums for each installed resource (by ResourceId)
    /// * `context_checksums` - Context checksums for template inputs (Optional)
    /// * `applied_patches_list` - Patches that were applied to each resource
    ///
    /// # Implementation Details
    ///
    /// - Updates are applied by ResourceId to handle duplicate resource names correctly
    /// - Context checksums are only applied if present (non-templated resources have None)
    /// - Only project patches are stored; private patches go to `agpm.private.lock`
    /// - Called by both `install` and `update` commands after parallel installation
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm_cli::lockfile::{LockFile, ResourceId};
    /// # use agpm_cli::manifest::patches::AppliedPatches;
    /// # use agpm_cli::core::ResourceType;
    /// let mut lockfile = LockFile::default();
    ///
    /// // Collect results from parallel installation
    /// let checksums = vec![/* (ResourceId, checksum) pairs */];
    /// let context_checksums = vec![/* (ResourceId, Option<checksum>) pairs */];
    /// let applied_patches = vec![/* (ResourceId, AppliedPatches) pairs */];
    /// let token_counts = vec![/* (ResourceId, Option<u64>) pairs */];
    ///
    /// // Apply all results in batch (replaces 3 separate loops)
    /// lockfile.apply_installation_results(
    ///     checksums,
    ///     context_checksums,
    ///     applied_patches,
    ///     token_counts,
    /// );
    /// ```
    ///
    pub fn apply_installation_results(
        &mut self,
        checksums: Vec<(ResourceId, String)>,
        context_checksums: Vec<(ResourceId, Option<String>)>,
        applied_patches_list: Vec<(ResourceId, crate::manifest::patches::AppliedPatches)>,
        token_counts: Vec<(ResourceId, Option<u64>)>,
    ) {
        // Update lockfile with checksums
        for (id, checksum) in checksums {
            self.update_resource_checksum(&id, &checksum);
        }

        // Update lockfile with context checksums
        for (id, context_checksum) in context_checksums {
            if let Some(checksum) = context_checksum {
                self.update_resource_context_checksum(&id, &checksum);
            }
        }

        // Update lockfile with applied patches
        for (id, applied_patches) in applied_patches_list {
            self.update_resource_applied_patches(id.name(), &applied_patches);
        }

        // Update lockfile with token counts
        for (id, token_count) in token_counts {
            self.update_resource_token_count(&id, token_count);
        }
    }

    /// Update the approximate token count for a resource.
    ///
    /// # Arguments
    ///
    /// * `id` - The resource identifier
    /// * `token_count` - The approximate BPE token count, or None for skills/directories
    fn update_resource_token_count(&mut self, id: &ResourceId, token_count: Option<u64>) {
        let resources = match id.resource_type() {
            crate::core::ResourceType::Agent => &mut self.agents,
            crate::core::ResourceType::Snippet => &mut self.snippets,
            crate::core::ResourceType::Command => &mut self.commands,
            crate::core::ResourceType::Script => &mut self.scripts,
            crate::core::ResourceType::Hook => &mut self.hooks,
            crate::core::ResourceType::McpServer => &mut self.mcp_servers,
            crate::core::ResourceType::Skill => &mut self.skills,
        };

        for resource in resources.iter_mut() {
            if resource.matches_id(id) {
                resource.approximate_token_count = token_count;
                return;
            }
        }
    }
}
