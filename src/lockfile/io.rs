//! I/O operations for lockfile loading and saving.
//!
//! This module handles atomic file operations for lockfiles, including
//! loading from disk, saving with atomic writes, and format validation.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::utils::fs::atomic_write;

use super::LockFile;
use super::helpers::serialize_lockfile_with_inline_patches;

impl LockFile {
    /// Load lockfile from disk with error handling and validation.
    ///
    /// Returns empty lockfile if file doesn't exist. Performs format version
    /// compatibility checking with detailed error messages.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the lockfile (typically "agpm.lock")
    ///
    /// # Returns
    ///
    /// * `Ok(LockFile)` - Successfully loaded lockfile or new empty lockfile if file doesn't exist
    /// * `Err(anyhow::Error)` - Parse error, IO error, or version incompatibility
    ///
    /// # Error Handling
    ///
    /// This method provides detailed error messages for common issues:
    /// - **File not found**: Returns empty lockfile (not an error)
    /// - **Permission denied**: Suggests checking file ownership/permissions
    /// - **TOML parse errors**: Suggests regenerating lockfile or checking syntax
    /// - **Version incompatibility**: Suggests updating AGPM
    /// - **Empty file**: Returns empty lockfile (graceful handling)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// // Load existing lockfile
    /// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
    /// println!("Loaded {} sources", lockfile.sources.len());
    ///
    /// // Non-existent file returns empty lockfile
    /// let empty = LockFile::load(Path::new("missing.lock"))?;
    /// assert!(empty.sources.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Version Compatibility
    ///
    /// The method checks the lockfile format version and will refuse to load
    /// lockfiles created by newer versions of AGPM:
    ///
    /// ```text
    /// Error: Lockfile version 2 is newer than supported version 1.
    /// This lockfile was created by a newer version of agpm.
    /// Please update agpm to the latest version to use this lockfile.
    /// ```
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path).with_context(|| {
            format!(
                "Cannot read lockfile: {}\n\n\
                    Possible causes:\n\
                    - File doesn't exist (run 'agpm install' to create it)\n\
                    - Permission denied (check file ownership)\n\
                    - File is corrupted or locked by another process",
                path.display()
            )
        })?;

        // Handle empty file
        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let mut lockfile: Self = toml::from_str(&content)
            .map_err(|e| crate::core::AgpmError::LockfileParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in lockfile: {}\n\n\
                    The lockfile may be corrupted. You can:\n\
                    - Delete agpm.lock and run 'agpm install' to regenerate it\n\
                    - Check for syntax errors if you manually edited the file\n\
                    - Restore from backup if available",
                    path.display()
                )
            })?;

        // Set resource_type and apply tool defaults based on which section it's in
        for resource in &mut lockfile.agents {
            resource.resource_type = crate::core::ResourceType::Agent;
            if resource.tool.is_none() {
                resource.tool = Some(crate::core::ResourceType::Agent.default_tool().to_string());
            }
        }
        for resource in &mut lockfile.snippets {
            resource.resource_type = crate::core::ResourceType::Snippet;
            if resource.tool.is_none() {
                resource.tool = Some(crate::core::ResourceType::Snippet.default_tool().to_string());
            }
        }
        for resource in &mut lockfile.commands {
            resource.resource_type = crate::core::ResourceType::Command;
            if resource.tool.is_none() {
                resource.tool = Some(crate::core::ResourceType::Command.default_tool().to_string());
            }
        }
        for resource in &mut lockfile.scripts {
            resource.resource_type = crate::core::ResourceType::Script;
            if resource.tool.is_none() {
                resource.tool = Some(crate::core::ResourceType::Script.default_tool().to_string());
            }
        }
        for resource in &mut lockfile.hooks {
            resource.resource_type = crate::core::ResourceType::Hook;
            if resource.tool.is_none() {
                resource.tool = Some(crate::core::ResourceType::Hook.default_tool().to_string());
            }
        }
        for resource in &mut lockfile.mcp_servers {
            resource.resource_type = crate::core::ResourceType::McpServer;
            if resource.tool.is_none() {
                resource.tool =
                    Some(crate::core::ResourceType::McpServer.default_tool().to_string());
            }
        }

        // Recompute hash for all VariantInputs
        // The hash is not stored in the lockfile (serde(skip)) but needs to be computed
        // from the variant_inputs Value for resource identity comparison
        for resource_type in crate::core::ResourceType::all() {
            for resource in lockfile.get_resources_mut(resource_type) {
                resource.variant_inputs.recompute_hash();
            }
        }

        // Check version compatibility
        if lockfile.version > Self::CURRENT_VERSION {
            return Err(crate::core::AgpmError::Other {
                message: format!(
                    "Lockfile version {} is newer than supported version {}.\n\n\
                    This lockfile was created by a newer version of agpm.\n\
                    Please update agpm to the latest version to use this lockfile.",
                    lockfile.version,
                    Self::CURRENT_VERSION
                ),
            }
            .into());
        }

        Ok(lockfile)
    }

    /// Save lockfile to disk with atomic writes and custom formatting.
    ///
    /// Serializes to TOML with header warning and custom formatting. Uses atomic writes
    /// (temp file + rename) to prevent corruption.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where to save the lockfile (typically "agpm.lock")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully saved lockfile
    /// * `Err(anyhow::Error)` - IO error, permission denied, or disk full
    ///
    /// # Atomic Write Behavior
    ///
    /// The save operation is atomic - the lockfile is written to a temporary file
    /// and then renamed to the target path. This ensures the lockfile is never
    /// left in a partially written state even if the process is interrupted.
    ///
    /// # Custom Formatting
    ///
    /// The method uses custom TOML formatting instead of standard serde serialization
    /// to produce more readable output:
    /// - Adds header comment warning against manual editing
    /// - Groups related fields together
    /// - Uses consistent indentation and spacing
    /// - Omits empty arrays to keep the file clean
    ///
    /// # Error Handling
    ///
    /// Provides detailed error messages for common issues:
    /// - **Permission denied**: Suggests running with elevated permissions
    /// - **Directory doesn't exist**: Suggests creating parent directories
    /// - **Disk full**: Suggests freeing space or using different location
    /// - **File locked**: Suggests closing other programs using the file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut lockfile = LockFile::new();
    ///
    /// // Add a source
    /// lockfile.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/example/repo.git".to_string(),
    ///     "a1b2c3d4e5f6...".to_string()
    /// );
    ///
    /// // Save to disk
    /// lockfile.save(Path::new("agpm.lock"))?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Generated File Format
    ///
    /// The saved file starts with a warning header:
    ///
    /// ```toml
    /// # Auto-generated lockfile - DO NOT EDIT
    /// version = 1
    ///
    /// [[sources]]
    /// name = "community"
    /// url = "https://github.com/example/repo.git"
    /// commit = "a1b2c3d4e5f6..."
    /// fetched_at = "2024-01-15T10:30:00Z"
    /// ```
    pub fn save(&self, path: &Path) -> Result<()> {
        // Normalize lockfile for backward compatibility before saving
        let normalized = self.normalize();

        // Use toml_edit to ensure applied_patches are formatted as inline tables
        let mut content = String::from("# Auto-generated lockfile - DO NOT EDIT\n");
        let toml_content = serialize_lockfile_with_inline_patches(&normalized)?;
        content.push_str(&toml_content);

        atomic_write(path, content.as_bytes()).with_context(|| {
            format!(
                "Cannot write lockfile: {}\n\n\
                    Possible causes:\n\
                    - Permission denied (try running with elevated permissions)\n\
                    - Directory doesn't exist\n\
                    - Disk is full or read-only\n\
                    - File is locked by another process",
                path.display()
            )
        })?;

        Ok(())
    }
}
