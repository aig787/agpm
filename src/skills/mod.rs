//! Skills module for AGPM
//!
//! This module provides functionality for managing Claude Skills, which are
//! directory-based resources containing a SKILL.md file with frontmatter and
//! optional supporting files.
//!
//! ## What are Skills?
//!
//! Skills are directories that:
//! - Contain a SKILL.md file with required YAML frontmatter
//! - May include additional files (REFERENCE.md, scripts, examples)
//! - Install to `.claude/skills/<name>/` as directories
//! - Can declare dependencies on other resources
//! - Support patching for customization
//!
//! ## SKILL.md Format
//!
//! ```yaml
//! ---
//! name: Skill Name
//! description: What this skill does
//! version: 1.0.0  # optional
//! allowed-tools: Read, Grep  # optional
//! dependencies:  # optional
//!   agents:
//!     - path: agents/helper.md
//! ---
//! # Skill content in markdown
//! ```
//!
//! ## Async vs Sync Functions
//!
//! This module uses a hybrid async/sync approach for performance and compatibility:
//!
//! - **`validate_skill_size`**: Async wrapper around sync `walkdir`. Integrates with
//!   the async installer pipeline while using `spawn_blocking` for the actual I/O.
//!   This prevents blocking the Tokio runtime during directory traversal.
//!
//! - **`extract_skill_metadata`**: Async for the same reason - wraps sync directory
//!   iteration in `spawn_blocking` to avoid blocking async contexts.
//!
//! - **`collect_skill_directory_info`**: Sync helper that performs the actual directory
//!   walk. Called via `spawn_blocking` from async functions. Uses `walkdir` which is
//!   inherently synchronous.
//!
//! - **`validate_skill_frontmatter`**: Pure sync function that only parses in-memory
//!   YAML. No I/O, so no need for async.
//!
//! The `walkdir` crate is synchronous, so we wrap it in `spawn_blocking` rather than
//! using a fake async interface. This is the recommended Tokio pattern for CPU-bound
//! or blocking I/O operations.

pub mod patches;

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Maximum number of files allowed in a skill directory (hard limit)
const MAX_SKILL_FILES: usize = 1000;

/// Maximum total size in bytes for all files in a skill (hard limit)
const MAX_SKILL_SIZE_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

/// Maximum YAML frontmatter size in bytes (defense-in-depth against DoS)
const MAX_FRONTMATTER_SIZE_BYTES: usize = 64 * 1024; // 64 KB

/// Maximum skill name length (for filesystem compatibility)
const MAX_NAME_LENGTH: usize = 100;

/// Maximum skill description length (reasonable limit for metadata)
const MAX_DESCRIPTION_LENGTH: usize = 1000;

/// Information collected from iterating over a skill directory.
///
/// This struct consolidates all directory traversal results to enable
/// a single pass over the directory for both validation and metadata extraction.
#[derive(Debug, Clone)]
pub struct SkillDirectoryInfo {
    /// List of relative file paths in the skill directory (sorted)
    pub files: Vec<String>,
    /// Total size of all files in bytes
    pub total_size: u64,
    /// Path to the SKILL.md file (if found)
    pub skill_md_path: Option<PathBuf>,
    /// Content of the SKILL.md file (if found and read)
    pub skill_md_content: Option<String>,
}

/// Iterate over a skill directory and collect file information.
///
/// This function performs a single pass over the skill directory, collecting:
/// - All file paths (relative to skill root)
/// - Total size of all files
/// - The SKILL.md content (if present)
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory
///
/// # Returns
///
/// Returns `SkillDirectoryInfo` with all collected information
///
/// # Errors
///
/// Returns an error if:
/// - The path is not a directory
/// - Directory traversal fails
/// - Symlinks are found (security risk)
/// - File count exceeds `MAX_SKILL_FILES`
/// - Total size exceeds `MAX_SKILL_SIZE_BYTES`
///
/// # Security
///
/// This function rejects symlinks to prevent data exfiltration and
/// path traversal attacks.
fn collect_skill_directory_info(skill_path: &Path) -> Result<SkillDirectoryInfo> {
    use walkdir::WalkDir;

    if !skill_path.is_dir() {
        return Err(anyhow!("Skill path {} is not a directory", skill_path.display()));
    }

    let mut files = Vec::new();
    let mut total_size = 0u64;
    let mut skill_md_path = None;
    let mut skill_md_content = None;

    for entry in WalkDir::new(skill_path).follow_links(false) {
        let entry = entry?;

        // Reject symlinks (security: could point to /etc/passwd, etc.)
        if entry.file_type().is_symlink() {
            return Err(anyhow!(
                "Skill at {} contains symlinks, which are not allowed for security reasons. \
                Symlinks could point to sensitive files or cause unexpected behavior across platforms.",
                skill_path.display()
            ));
        }

        if entry.file_type().is_file() {
            let file_path = entry.path();
            let relative_path = file_path
                .strip_prefix(skill_path)
                .map_err(|e| anyhow!("Failed to get relative path: {}", e))?
                .to_string_lossy()
                .to_string();

            // Check if this is the SKILL.md file
            if relative_path == "SKILL.md" {
                skill_md_path = Some(file_path.to_path_buf());
                // Read SKILL.md content while we're iterating
                // BLOCKING I/O is safe here: called via spawn_blocking from async context
                // (see validate_skill_size and extract_skill_metadata which wrap this function)
                skill_md_content = Some(std::fs::read_to_string(file_path).with_file_context(
                    FileOperation::Read,
                    file_path,
                    "loading skill metadata",
                    "collect_skill_directory_info",
                )?);
            }

            let metadata = entry.metadata()?;
            total_size += metadata.len();
            files.push(relative_path);

            // Check file count limit
            if files.len() > MAX_SKILL_FILES {
                return Err(anyhow!(
                    "Skill at {} contains {} files, which exceeds the maximum limit of {} files. \
                    Skills should be focused and minimal. Consider splitting into multiple skills.",
                    skill_path.display(),
                    files.len(),
                    MAX_SKILL_FILES
                ));
            }

            // Check size limit
            if total_size > MAX_SKILL_SIZE_BYTES {
                let size_mb = total_size as f64 / (1024.0 * 1024.0);
                let limit_mb = MAX_SKILL_SIZE_BYTES as f64 / (1024.0 * 1024.0);
                return Err(anyhow!(
                    "Skill at {} total size is {:.2} MB, which exceeds the maximum limit of {:.0} MB. \
                    Skills should be focused and minimal. Consider optimizing file sizes or removing unnecessary files.",
                    skill_path.display(),
                    size_mb,
                    limit_mb
                ));
            }
        }
    }

    // Sort files for consistent ordering
    files.sort();

    Ok(SkillDirectoryInfo {
        files,
        total_size,
        skill_md_path,
        skill_md_content,
    })
}

/// Frontmatter structure for SKILL.md files
///
/// This struct represents the YAML frontmatter that must be present
/// in every SKILL.md file. It defines the skill's metadata and
/// configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    /// Human-readable name of the skill
    pub name: String,

    /// Description of what the skill does
    pub description: String,

    /// Optional version identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional list of tools the skill is allowed to use
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional dependencies on other resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<serde_yaml::Value>,
}

/// Validate and extract frontmatter from SKILL.md content
///
/// This function parses the YAML frontmatter from a SKILL.md file,
/// validates that required fields are present, and returns the
/// structured frontmatter data.
///
/// # Arguments
///
/// * `content` - The full content of the SKILL.md file
///
/// # Returns
///
/// Returns the parsed frontmatter if valid
///
/// # Errors
///
/// Returns an error if:
/// - The file doesn't have proper YAML frontmatter (missing --- markers)
/// - The YAML is invalid
/// - Required fields (name, description) are missing or empty
///
/// # Examples
///
/// ```
/// use agpm_cli::skills::validate_skill_frontmatter;
///
/// # fn example() -> anyhow::Result<()> {
/// let content = r#"---
/// name: My Skill
/// description: A helpful skill
/// ---
/// # My Skill
///
/// This skill helps with...
/// "#;
///
/// let frontmatter = validate_skill_frontmatter(content)?;
/// assert_eq!(frontmatter.name, "My Skill");
/// assert_eq!(frontmatter.description, "A helpful skill");
/// # Ok(())
/// # }
/// ```
pub fn validate_skill_frontmatter(content: &str) -> Result<SkillFrontmatter> {
    // Split content by --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();

    if parts.len() < 3 {
        return Err(anyhow!(
            "SKILL.md missing required YAML frontmatter. Format:\n---\nname: Skill Name\ndescription: What it does\n---\n# Content"
        ));
    }

    // Parse YAML frontmatter
    let frontmatter_str = parts[1].trim();

    // Validate frontmatter size (defense-in-depth against DoS)
    if frontmatter_str.len() > MAX_FRONTMATTER_SIZE_BYTES {
        return Err(anyhow!(
            "SKILL.md frontmatter exceeds maximum size of {} KB",
            MAX_FRONTMATTER_SIZE_BYTES / 1024
        ));
    }

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(frontmatter_str).map_err(|e| {
        // Truncate YAML content in error messages to avoid leaking sensitive data from patches
        // Use 80 chars (single line) to minimize potential exposure of API keys or secrets
        // Use chars().take() to avoid splitting UTF-8 character boundaries
        let char_count = frontmatter_str.chars().count();
        let yaml_preview = if char_count > 80 {
            let truncated: String = frontmatter_str.chars().take(80).collect();
            format!("{}... ({} chars total)", truncated, char_count)
        } else {
            frontmatter_str.to_string()
        };
        anyhow!("Invalid SKILL.md frontmatter: {}\nYAML content:\n{}", e, yaml_preview)
    })?;

    // Validate required fields
    if frontmatter.name.trim().is_empty() {
        return Err(anyhow!("SKILL.md frontmatter missing required 'name' field"));
    }

    if frontmatter.description.trim().is_empty() {
        return Err(anyhow!("SKILL.md frontmatter missing required 'description' field"));
    }

    // Validate field lengths
    if frontmatter.name.len() > MAX_NAME_LENGTH {
        return Err(anyhow!("Skill name exceeds maximum length of {} characters", MAX_NAME_LENGTH));
    }

    if frontmatter.description.len() > MAX_DESCRIPTION_LENGTH {
        return Err(anyhow!(
            "Skill description exceeds maximum length of {} characters",
            MAX_DESCRIPTION_LENGTH
        ));
    }

    // Validate name contains only allowed ASCII characters for cross-platform filename compatibility
    // Defense-in-depth: explicitly check for path traversal sequences even though
    // the allowlist below would block them anyway
    if frontmatter.name.contains("..")
        || frontmatter.name.contains('/')
        || frontmatter.name.contains('\\')
    {
        return Err(anyhow!(
            "Skill name contains path traversal sequences or path separators. \
             Use ASCII letters, numbers, spaces, hyphens, and underscores only"
        ));
    }

    if !frontmatter
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(anyhow!(
            "Skill name contains invalid characters. Use ASCII letters, numbers, spaces, hyphens, and underscores only"
        ));
    }

    Ok(frontmatter)
}

/// Validate skill directory size and file count before installation.
///
/// This prevents malicious or accidentally large skills from consuming
/// excessive disk space or inodes. Checks:
/// - File count ≤ MAX_SKILL_FILES (1000)
/// - Total size ≤ MAX_SKILL_SIZE_BYTES (100MB)
/// - No symlinks (security risk: could point to sensitive files)
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory to validate
///
/// # Returns
///
/// * `Ok(SkillDirectoryInfo)` - Skill passes all checks, returns collected info
/// * `Err(anyhow::Error)` - Skill exceeds limits or contains symlinks
///
/// # Security
///
/// This function rejects symlinks to prevent:
/// - Data exfiltration (symlink to /etc/passwd, ~/.ssh/id_rsa)
/// - Path traversal attacks
/// - Unexpected behavior across platforms
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::skills::validate_skill_size;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let info = validate_skill_size(Path::new("my-skill")).await?;
/// println!("Skill has {} files totaling {} bytes", info.files.len(), info.total_size);
/// # Ok(())
/// # }
/// ```
pub async fn validate_skill_size(skill_path: &Path) -> Result<SkillDirectoryInfo> {
    let path = skill_path.to_path_buf();

    // Run blocking directory iteration in a separate thread
    tokio::task::spawn_blocking(move || collect_skill_directory_info(&path))
        .await
        .map_err(|e| anyhow!("Task join error during skill validation: {}", e))?
}

/// Extract metadata from a skill directory.
///
/// This function reads a skill directory, validates its structure,
/// and extracts metadata including the frontmatter and file list.
/// Uses the shared `SkillDirectoryInfo` to perform validation and
/// metadata extraction in a single pass.
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory
///
/// # Returns
///
/// Returns a tuple of (frontmatter, file_list) if valid
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::skills::extract_skill_metadata;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let (frontmatter, files) = extract_skill_metadata(Path::new("my-skill")).await?;
/// println!("Skill: {}", frontmatter.name);
/// println!("Files: {:?}", files);
/// # Ok(())
/// # }
/// ```
pub async fn extract_skill_metadata(skill_path: &Path) -> Result<(SkillFrontmatter, Vec<String>)> {
    tracing::debug!("extract_skill_metadata called with path: {}", skill_path.display());

    let path = skill_path.to_path_buf();
    let display_path = skill_path.display().to_string();

    // Run blocking directory iteration in a separate thread
    let info = tokio::task::spawn_blocking(move || collect_skill_directory_info(&path))
        .await
        .map_err(|e| anyhow!("Task join error during skill metadata extraction: {}", e))??;

    // Validate that SKILL.md was found and read
    let skill_md_content = info
        .skill_md_content
        .ok_or_else(|| anyhow!("Skill at {} missing required SKILL.md file", display_path))?;

    // Parse and validate frontmatter
    let frontmatter = validate_skill_frontmatter(&skill_md_content)?;

    tracing::debug!(
        "Extracted metadata for skill '{}': {} files, {} bytes",
        frontmatter.name,
        info.files.len(),
        info.total_size
    );

    Ok((frontmatter, info.files))
}

/// Extract metadata from pre-collected skill directory info.
///
/// This is a synchronous helper that extracts frontmatter from already-collected
/// directory information. Use this when you have already called `validate_skill_size`
/// and want to avoid re-iterating the directory.
///
/// # Arguments
///
/// * `info` - Pre-collected directory information from `validate_skill_size`
/// * `skill_path` - Path to the skill directory (for error messages)
///
/// # Returns
///
/// Returns a tuple of (frontmatter, file_list) if valid
pub fn extract_skill_metadata_from_info(
    info: &SkillDirectoryInfo,
    skill_path: &Path,
) -> Result<(SkillFrontmatter, Vec<String>)> {
    let skill_md_content = info.skill_md_content.as_ref().ok_or_else(|| {
        anyhow!("Skill at {} missing required SKILL.md file", skill_path.display())
    })?;

    let frontmatter = validate_skill_frontmatter(skill_md_content)?;

    Ok((frontmatter, info.files.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_skill_frontmatter_valid() {
        let content = r#"---
name: Test Skill
description: A test skill
version: 1.0.0
allowed-tools:
  - Read
  - Write
dependencies:
  agents:
    - path: helper.md
---
# Test Skill

This is a test skill.
"#;

        // Test assertion: valid frontmatter must parse successfully
        let result = validate_skill_frontmatter(content).unwrap();
        assert_eq!(result.name, "Test Skill");
        assert_eq!(result.description, "A test skill");
        assert_eq!(result.version, Some("1.0.0".to_string()));
        assert_eq!(result.allowed_tools, Some(vec!["Read".to_string(), "Write".to_string()]));
    }

    #[test]
    fn test_validate_skill_frontmatter_missing_fields() {
        let content = r#"---
name: Test Skill
---
# Test Skill
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        // Test assertion: error guaranteed by is_err() check above
        assert!(result.unwrap_err().to_string().contains("description"));
    }

    #[test]
    fn test_validate_skill_frontmatter_no_frontmatter() {
        let content = r#"# Test Skill

This skill has no frontmatter.
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        // Test assertion: error guaranteed by is_err() check above
        assert!(result.unwrap_err().to_string().contains("missing required YAML frontmatter"));
    }

    #[test]
    fn test_validate_skill_frontmatter_invalid_yaml() {
        let content = r#"---
name: Test Skill
description: Invalid YAML
unclosed: [ "item1", "item2"
---
# Test Skill
"#;

        let result = validate_skill_frontmatter(content);
        assert!(result.is_err());
        // Test assertion: error guaranteed by is_err() check above
        assert!(result.unwrap_err().to_string().contains("Invalid SKILL.md frontmatter"));
    }
}
