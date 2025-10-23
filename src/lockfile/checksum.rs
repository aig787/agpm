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
    /// # use agpm_cli::lockfile::{LockFile, LockedResource, ResourceId};
    /// # use agpm_cli::core::ResourceType;
    /// # let mut lockfile = LockFile::default();
    /// # // First add a resource to update
    /// # lockfile.add_typed_resource("my-agent".to_string(), LockedResource::new(
    /// #     "my-agent".to_string(),
    /// #     None,
    /// #     None,
    /// #     "my-agent.md".to_string(),
    /// #     None,
    /// #     None,
    /// #     "".to_string(),
    /// #     "agents/my-agent.md".to_string(),
    /// #     vec![],
    /// #     ResourceType::Agent,
    /// #     Some("claude-code".to_string()),
    /// #     None,
    /// #     std::collections::HashMap::new(),
    /// #     None,
    /// #     serde_json::Value::Object(serde_json::Map::new()),
    /// # ), ResourceType::Agent);
    /// let id = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, serde_json::json!({}));
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

        false
    }
}
