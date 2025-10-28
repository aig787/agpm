//! Lockfile dependency reference handling.
//!
//! This module provides a structured way to parse and format the dependency references
//! that appear in the lockfile's `dependencies` arrays. These use a specific compact format
//! designed for lockfile serialization.

use anyhow::{Result, bail};
use std::fmt;
use std::str::FromStr;

use crate::core::ResourceType;

/// A structured representation of a lockfile dependency reference.
///
/// This type represents dependencies as they appear in `agpm.lock` files.
/// The format is compact and designed for lockfile serialization.
///
/// Supports the following formats:
/// - Local: `<type>:<path>` (e.g., `snippet:snippets/commands/update-docstrings`)
/// - Git: `<source>/<type>:<path>@<version>` (e.g., `agpm-resources/snippet:snippets/commands/update-docstrings@v0.0.1`)
///
/// Examples:
/// ```
/// use agpm_cli::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
/// use agpm_cli::core::ResourceType;
/// use std::str::FromStr;
///
/// let local_dep = LockfileDependencyRef::from_str("snippet:snippets/commands/update-docstrings").unwrap();
/// assert_eq!(local_dep.source, None);
/// assert_eq!(local_dep.resource_type, ResourceType::Snippet);
/// assert_eq!(local_dep.path, "snippets/commands/update-docstrings");
/// assert_eq!(local_dep.version, None);
///
/// let git_dep = LockfileDependencyRef::from_str("agpm-resources/snippet:snippets/commands/update-docstrings@v0.0.1").unwrap();
/// assert_eq!(git_dep.source, Some("agpm-resources".to_string()));
/// assert_eq!(git_dep.resource_type, ResourceType::Snippet);
/// assert_eq!(git_dep.path, "snippets/commands/update-docstrings");
/// assert_eq!(git_dep.version, Some("v0.0.1".to_string()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockfileDependencyRef {
    /// Optional source name (e.g., "agpm-resources")
    pub source: Option<String>,
    /// Resource type (agent, snippet, command, etc.)
    pub resource_type: ResourceType,
    /// Path within the source repository (e.g., "snippets/commands/update-docstrings")
    pub path: String,
    /// Optional version constraint (e.g., "v0.0.1")
    pub version: Option<String>,
}

impl LockfileDependencyRef {
    /// Create a new lockfile dependency reference.
    pub fn new(
        source: Option<String>,
        resource_type: ResourceType,
        path: String,
        version: Option<String>,
    ) -> Self {
        Self {
            source,
            resource_type,
            path,
            version,
        }
    }

    /// Create a local dependency reference (no source).
    pub fn local(resource_type: ResourceType, path: String, version: Option<String>) -> Self {
        Self {
            source: None,
            resource_type,
            path,
            version,
        }
    }

    /// Create a Git dependency reference with source.
    pub fn git(
        source: String,
        resource_type: ResourceType,
        path: String,
        version: Option<String>,
    ) -> Self {
        Self {
            source: Some(source),
            resource_type,
            path,
            version,
        }
    }
}

impl FromStr for LockfileDependencyRef {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        // Parse version first (if present)
        let (base_part, version) = if let Some(at_pos) = s.rfind('@') {
            let base = &s[..at_pos];
            let version = Some(s[at_pos + 1..].to_string());
            (base, version)
        } else {
            (s, None)
        };

        // Determine if this is a Git dependency (has / before the first :) or Local dependency
        let first_colon_pos = base_part.find(':').unwrap_or(0);
        let has_slash_before_colon = if let Some(slash_pos) = base_part.find('/') {
            slash_pos < first_colon_pos
        } else {
            false
        };
        let is_git = has_slash_before_colon;

        let (source, type_path_part) = if is_git {
            if let Some(slash_pos) = base_part.find('/') {
                let source_part = &base_part[..slash_pos];
                let rest = &base_part[slash_pos + 1..];
                (Some(source_part.to_string()), rest)
            } else {
                bail!("Git dependency format requires / separator: {}", s);
            }
        } else {
            (None, base_part)
        };

        // Parse type and path
        if let Some(colon_pos) = type_path_part.find(':') {
            let type_part = &type_path_part[..colon_pos];
            let path_part = &type_path_part[colon_pos + 1..];

            // Parse resource type
            let resource_type = type_part.parse::<ResourceType>()?;

            if path_part.is_empty() {
                bail!("Dependency path cannot be empty in: {}", s);
            }

            Ok(Self {
                source,
                resource_type,
                path: path_part.to_string(),
                version,
            })
        } else {
            bail!(
                "Invalid dependency reference format: {}. Expected format: <type>:<path> or <source>/<type>:<path>",
                s
            );
        }
    }
}

impl fmt::Display for LockfileDependencyRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Always use forward slashes for lockfile storage (cross-platform compatibility)
        let normalized_path = crate::utils::normalize_path_for_storage(&self.path);

        match &self.source {
            Some(source) => {
                // Git dependency: source/type:path@version
                write!(f, "{}/{}:{}", source, self.resource_type, normalized_path)?;
                if let Some(version) = &self.version {
                    write!(f, "@{}", version)?;
                }
                Ok(())
            }
            None => {
                // Local dependency: type:path@version
                write!(f, "{}:{}", self.resource_type, normalized_path)?;
                if let Some(version) = &self.version {
                    write!(f, "@{}", version)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_dependency_no_version() {
        let dep =
            LockfileDependencyRef::from_str("snippet:snippets/commands/update-docstrings").unwrap();
        assert_eq!(dep.source, None);
        assert_eq!(dep.resource_type, ResourceType::Snippet);
        assert_eq!(dep.path, "snippets/commands/update-docstrings");
        assert_eq!(dep.version, None);
    }

    #[test]
    fn test_parse_local_dependency_with_version() {
        let dep =
            LockfileDependencyRef::from_str("snippet:snippets/commands/update-docstrings@v0.0.1")
                .unwrap();
        assert_eq!(dep.source, None);
        assert_eq!(dep.resource_type, ResourceType::Snippet);
        assert_eq!(dep.path, "snippets/commands/update-docstrings");
        assert_eq!(dep.version, Some("v0.0.1".to_string()));
    }

    #[test]
    fn test_parse_git_dependency_no_version() {
        let dep = LockfileDependencyRef::from_str(
            "agpm-resources/snippet:snippets/commands/update-docstrings",
        )
        .unwrap();
        assert_eq!(dep.source, Some("agpm-resources".to_string()));
        assert_eq!(dep.resource_type, ResourceType::Snippet);
        assert_eq!(dep.path, "snippets/commands/update-docstrings");
        assert_eq!(dep.version, None);
    }

    #[test]
    fn test_parse_git_dependency_with_version() {
        let dep = LockfileDependencyRef::from_str(
            "agpm-resources/snippet:snippets/commands/update-docstrings@v0.0.1",
        )
        .unwrap();
        assert_eq!(dep.source, Some("agpm-resources".to_string()));
        assert_eq!(dep.resource_type, ResourceType::Snippet);
        assert_eq!(dep.path, "snippets/commands/update-docstrings");
        assert_eq!(dep.version, Some("v0.0.1".to_string()));
    }

    #[test]
    fn test_parse_invalid_format() {
        let result = LockfileDependencyRef::from_str("invalid-format");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_path() {
        let result = LockfileDependencyRef::from_str("snippet:");
        assert!(result.is_err());
    }

    #[test]
    fn test_display_local_dependency() {
        let dep = LockfileDependencyRef::local(
            ResourceType::Snippet,
            "snippets/commands/update-docstrings".to_string(),
            Some("v0.0.1".to_string()),
        );
        assert_eq!(dep.to_string(), "snippet:snippets/commands/update-docstrings@v0.0.1");
    }

    #[test]
    fn test_display_local_dependency_no_version() {
        let dep = LockfileDependencyRef::local(
            ResourceType::Snippet,
            "snippets/commands/update-docstrings".to_string(),
            None,
        );
        assert_eq!(dep.to_string(), "snippet:snippets/commands/update-docstrings");
    }

    #[test]
    fn test_display_git_dependency() {
        let dep = LockfileDependencyRef::git(
            "agpm-resources".to_string(),
            ResourceType::Snippet,
            "snippets/commands/update-docstrings".to_string(),
            Some("v0.0.1".to_string()),
        );
        assert_eq!(
            dep.to_string(),
            "agpm-resources/snippet:snippets/commands/update-docstrings@v0.0.1"
        );
    }

    #[test]
    fn test_display_git_dependency_no_version() {
        let dep = LockfileDependencyRef::git(
            "agpm-resources".to_string(),
            ResourceType::Snippet,
            "snippets/commands/update-docstrings".to_string(),
            None,
        );
        assert_eq!(dep.to_string(), "agpm-resources/snippet:snippets/commands/update-docstrings");
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = "agpm-resources/snippet:snippets/commands/update-docstrings@v0.0.1";
        let parsed = LockfileDependencyRef::from_str(original).unwrap();
        assert_eq!(parsed.to_string(), original);
    }
}
