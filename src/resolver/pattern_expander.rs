//! Pattern expansion for AGPM dependencies.
//!
//! This module handles expansion of glob patterns to concrete file paths,
//! converting pattern dependencies (like "agents/*.md") into individual file
//! dependencies. It supports both local and remote pattern resolution with
//! proper path handling, dependency naming, and locked resource generation.

use crate::git::GitRepo;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::pattern::PatternResolver;
use crate::resolver::version_resolver::PreparedSourceVersion;
use crate::utils::normalize_path_for_storage;
use anyhow::{Context, Result};
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Expands a pattern dependency into concrete dependencies.
///
/// This function is the core engine for pattern-based dependency resolution,
/// handling both local and remote patterns to generate specific resource
/// dependencies that can be fetched and installed.
///
/// # Pattern Types Supported
///
/// ## Local Patterns
/// - Relative paths: `local/agents/*.md`, `./snippets/*.toml`
/// - Absolute paths: `/home/user/resources/*`
/// - Directory patterns: `tools/*/bin`
///
/// ## Remote Patterns
/// - Git repository patterns: `repo:agents/*`, `source:tools/*.py`
/// - Version-constrained patterns: `repo:agents/*@v2.0.0`
///
/// # Resolution Strategy
///
/// 1. **Pattern Analysis**: Parse pattern to identify base path and glob
/// 2. **Source Resolution**: For remote patterns, resolve source name
/// 3. **Resource Discovery**: Use pattern to find matching resources
/// 4. **Dependency Generation**: Create concrete dependencies for each resource
/// 5. **Version Application**: Apply version constraints to remote patterns
///
/// # Parameters
///
/// * `dep` - The pattern dependency to expand
/// * `resource_type` - Type of resource (agent, snippet, command, etc.)
/// * `source_manager` - Source management instance for remote patterns
/// * `cache` - Cache instance for repository access
/// * `manifest_dir` - Optional manifest directory for local pattern resolution
/// * `prepared_versions` - Pre-resolved versions for performance optimization
///
/// # Returns
///
/// Vector of tuples containing:
/// - Generated dependency name
/// - Concrete resource dependency ready for installation
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::resolver::pattern_expander::expand_pattern_to_concrete_deps;
/// use agpm_cli::source::SourceManager;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::manifest::{DetailedDependency, ResourceDependency};
/// use agpm_cli::core::ResourceType;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let source_manager = SourceManager::new()?;
/// # let cache = Cache::new()?;
/// # let pattern_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
/// #     path: "agents/*.md".to_string(),
/// #     source: Some("community".to_string()),
/// #     version: None,
/// #     branch: None,
/// #     rev: None,
/// #     command: None,
/// #     args: None,
/// #     target: None,
/// #     filename: None,
/// #     dependencies: None,
/// #     tool: None,
/// #     flatten: None,
/// #     install: None,
/// #     template_vars: None,
/// # }));
/// let deps = expand_pattern_to_concrete_deps(
///     &pattern_dep,           // Pattern dependency
///     ResourceType::Agent,     // Resource type
///     &source_manager,         // For remote sources
///     &cache,                // For repository access
///     Some(Path::new("/project")), // For local resolution
///     None,                  // No pre-prepared versions
/// ).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Performance Considerations
///
/// - Remote patterns trigger a single repository fetch, then cache multiple resources
/// - Local patterns scan filesystem without network operations
/// - Prepared versions enable SHA reuse to avoid redundant Git operations
/// - The function returns a complete dependency set ready for installer processing
pub async fn expand_pattern_to_concrete_deps(
    dep: &ResourceDependency,
    resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
    manifest_dir: Option<&Path>,
    prepared_versions: Option<&DashMap<String, PreparedSourceVersion>>,
) -> Result<Vec<(String, ResourceDependency)>> {
    let pattern = dep.get_path();

    if dep.is_local() {
        expand_local_pattern(dep, pattern, resource_type, manifest_dir).await
    } else {
        expand_remote_pattern(dep, pattern, resource_type, source_manager, cache, prepared_versions)
            .await
    }
}

/// Expands a local pattern dependency.
async fn expand_local_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    resource_type: crate::core::ResourceType,
    manifest_dir: Option<&Path>,
) -> Result<Vec<(String, ResourceDependency)>> {
    // For absolute patterns, use the parent directory as base and strip the pattern to just the filename part
    // For relative patterns, use manifest directory
    let pattern_path = Path::new(pattern);
    let (base_path, search_pattern) = if pattern_path.is_absolute() {
        // Absolute pattern: extract base directory and relative pattern
        // Example: "/tmp/xyz/agents/*.md" -> base="/tmp/xyz", pattern="agents/*.md"
        let components: Vec<_> = pattern_path.components().collect();

        // Find the first component with a glob character
        let glob_idx = components.iter().position(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.contains('*') || s.contains('?') || s.contains('[')
        });

        if let Some(idx) = glob_idx {
            // Split at the glob component
            let base_components = &components[..idx];
            let pattern_components = &components[idx..];

            let base: PathBuf = base_components.iter().collect();
            let pattern: String = pattern_components
                .iter()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");

            (base, pattern)
        } else {
            // No glob characters, use as-is
            (PathBuf::from("."), pattern.to_string())
        }
    } else {
        // Relative pattern, use manifest directory as base
        let base = manifest_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        (base, pattern.to_string())
    };

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    // Skills are directory-based, so use special directory matching
    if resource_type == crate::core::ResourceType::Skill {
        let skill_matches = crate::resolver::skills::match_skill_directories(
            &base_path,
            &search_pattern,
            None, // No strip prefix for local patterns
        )
        .await?;

        debug!("Local skill pattern '{}' matched {} directories", pattern, skill_matches.len());

        for (skill_name, skill_path) in skill_matches {
            // Create a concrete dependency for the matched skill directory
            let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
                path: skill_path,
                source: None,
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: target.clone(),
                filename: None,
                dependencies: None,
                tool: tool.clone(),
                flatten,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));

            concrete_deps.push((skill_name, concrete_dep));
        }
    } else {
        // For file-based resources, use the pattern resolver
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(&search_pattern, &base_path)?;

        debug!("Pattern '{}' matched {} files", pattern, matches.len());

        for matched_path in matches {
            // Convert matched path to absolute by joining with base_path
            // Use normalized paths (forward slashes) for cross-platform lockfile compatibility
            let absolute_path = base_path.join(&matched_path);
            let concrete_path = normalize_path_for_storage(&absolute_path);

            // Generate a dependency name using source context
            let source_context = if let Some(manifest_dir) = manifest_dir {
                // For local dependencies, use manifest directory as source context
                crate::resolver::source_context::SourceContext::local(manifest_dir)
            } else {
                // Fallback: use the base_path as source context
                crate::resolver::source_context::SourceContext::local(&base_path)
            };

            let dep_name = generate_dependency_name(&concrete_path, &source_context);

            // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
            let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
                path: concrete_path,
                source: None,
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: target.clone(),
                filename: None,
                dependencies: None,
                tool: tool.clone(),
                flatten,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));

            concrete_deps.push((dep_name, concrete_dep));
        }
    }

    Ok(concrete_deps)
}

/// Expands a remote pattern dependency.
async fn expand_remote_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
    prepared_versions: Option<&DashMap<String, PreparedSourceVersion>>,
) -> Result<Vec<(String, ResourceDependency)>> {
    let source_name = dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Remote pattern dependency missing source: {}", pattern))?;

    let source_url = source_manager
        .get_source_url(source_name)
        .with_context(|| format!("Source not found: {}", source_name))?;

    // Get or clone the source repository
    let repo_path = cache
        .get_or_clone_source(source_name, &source_url, dep.get_version())
        .await
        .with_context(|| format!("Failed to access source repository: {}", source_name))?;

    let repo = GitRepo::new(&repo_path);

    // Resolve the version to a commit SHA, preferring pre-prepared versions to avoid redundant Git work
    let version = dep.get_version().unwrap_or("HEAD");
    let group_key = format!("{}::{}", source_name, version);
    let (commit_sha, worktree_path) = if let Some(prepared_map) = prepared_versions {
        if let Some(prepared) = prepared_map.get(&group_key) {
            (prepared.resolved_commit.clone(), prepared.worktree_path.clone())
        } else {
            let sha = repo.resolve_to_sha(Some(version)).await.with_context(|| {
                format!("Failed to resolve version '{}' for source {}", version, source_name)
            })?;
            let path = cache
                .get_or_create_worktree_for_sha(source_name, &source_url, &sha, Some(version))
                .await
                .with_context(|| {
                    format!("Failed to create worktree for {}@{}", source_name, version)
                })?;
            (sha, path)
        }
    } else {
        let sha = repo.resolve_to_sha(Some(version)).await.with_context(|| {
            format!("Failed to resolve version '{}' for source {}", version, source_name)
        })?;
        let path = cache
            .get_or_create_worktree_for_sha(source_name, &source_url, &sha, Some(version))
            .await
            .with_context(|| {
                format!("Failed to create worktree for {}@{}", source_name, version)
            })?;
        (sha, path)
    };

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    // Skills are directory-based, so use special directory matching
    if resource_type == crate::core::ResourceType::Skill {
        let skill_matches = crate::resolver::skills::match_skill_directories(
            &worktree_path,
            pattern,
            Some(&worktree_path),
        )
        .await?;

        debug!(
            "Remote skill pattern '{}' in {} matched {} directories",
            pattern,
            source_name,
            skill_matches.len()
        );

        for (skill_name, skill_path) in skill_matches {
            // Create a concrete dependency for the matched skill directory
            let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
                path: skill_path,
                source: Some(source_name.to_string()),
                version: Some(commit_sha.clone()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: target.clone(),
                filename: None,
                dependencies: None,
                tool: tool.clone(),
                flatten,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));

            concrete_deps.push((skill_name, concrete_dep));
        }
    } else {
        // For file-based resources, use the pattern resolver
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(pattern, &worktree_path)?;

        debug!("Remote pattern '{}' in {} matched {} files", pattern, source_name, matches.len());

        for matched_path in matches {
            // Generate a dependency name using source context
            // For Git dependencies, use the repository root as source context
            let source_context =
                crate::resolver::source_context::SourceContext::git(&worktree_path);
            let dep_name =
                generate_dependency_name(&matched_path.to_string_lossy(), &source_context);

            // matched_path is already relative to worktree root (from PatternResolver)
            // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
            let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
                path: matched_path.to_string_lossy().to_string(),
                source: Some(source_name.to_string()),
                version: Some(commit_sha.clone()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: target.clone(),
                filename: None,
                dependencies: None,
                tool: tool.clone(),
                flatten,
                install: None,
                template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
            }));

            concrete_deps.push((dep_name, concrete_dep));
        }
    }

    Ok(concrete_deps)
}

/// Generates a dependency name from a path using source context.
/// Creates collision-resistant names by preserving directory structure relative to source.
pub fn generate_dependency_name(
    path: &str,
    source_context: &crate::resolver::source_context::SourceContext,
) -> String {
    // Use the new source context-aware name generation
    crate::resolver::source_context::compute_canonical_name(path, source_context)
}

// ============================================================================
// Pattern Expansion Service
// ============================================================================

use crate::core::ResourceType;
use std::sync::Arc;

use super::types::ResolutionCore;

/// Service for pattern expansion and resolution.
///
/// Handles expansion of glob patterns to concrete dependencies and maintains
/// mappings between concrete files and their source patterns.
pub struct PatternExpansionService {
    /// Map tracking pattern alias relationships (concrete_name -> pattern_name)
    pattern_alias_map: Arc<DashMap<(ResourceType, String), String>>,
}

impl PatternExpansionService {
    /// Create a new pattern expansion service.
    pub fn new() -> Self {
        Self {
            pattern_alias_map: Arc::new(DashMap::new()),
        }
    }

    /// Expand a pattern dependency to concrete dependencies.
    ///
    /// Takes a glob pattern like "agents/*.md" and expands it to
    /// concrete file paths like ["agents/foo.md", "agents/bar.md"].
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with cache and source manager
    /// * `dep` - The pattern dependency to expand
    /// * `resource_type` - The type of resource being expanded
    /// # Returns
    ///
    /// List of (name, concrete_dependency) tuples
    pub async fn expand_pattern(
        &self,
        core: &ResolutionCore,
        dep: &ResourceDependency,
        resource_type: ResourceType,
        prepared_versions: &DashMap<String, PreparedSourceVersion>,
    ) -> Result<Vec<(String, ResourceDependency)>> {
        // Delegate to expand_pattern_to_concrete_deps helper
        expand_pattern_to_concrete_deps(
            dep,
            resource_type,
            &core.source_manager,
            &core.cache,
            core.manifest.manifest_dir.as_deref(),
            Some(prepared_versions),
        )
        .await
    }

    /// Get pattern alias for a concrete dependency.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type
    /// * `name` - The concrete dependency name
    ///
    /// # Returns
    ///
    /// The pattern name if this is from a pattern expansion
    pub fn get_pattern_alias(
        &self,
        resource_type: ResourceType,
        name: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, (ResourceType, String), String>> {
        self.pattern_alias_map.get(&(resource_type, name.to_string()))
    }

    /// Record a pattern alias mapping.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - The resource type
    /// * `concrete_name` - The concrete file name
    /// * `pattern_name` - The pattern that expanded to this file
    pub fn add_pattern_alias(
        &self,
        resource_type: ResourceType,
        concrete_name: String,
        pattern_name: String,
    ) {
        self.pattern_alias_map.insert((resource_type, concrete_name), pattern_name);
    }
}

impl Default for PatternExpansionService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;
    use std::path::Path;
    use tempfile;
    use tokio::fs;

    // Tests for generate_dependency_name() with proper SourceContext

    #[test]
    fn test_generate_dependency_name_local_context() {
        // Test with local source context - paths relative to manifest directory
        // Use platform-appropriate absolute paths
        #[cfg(windows)]
        let manifest_dir = Path::new("C:\\project");
        #[cfg(not(windows))]
        let manifest_dir = Path::new("/project");
        let source_context = crate::resolver::source_context::SourceContext::local(manifest_dir);

        // Test absolute path within manifest directory
        #[cfg(windows)]
        let abs_path = "C:\\project\\agents\\helper.md";
        #[cfg(not(windows))]
        let abs_path = "/project/agents/helper.md";
        let name = generate_dependency_name(abs_path, &source_context);
        assert_eq!(name, "agents/helper");

        // Test relative path (already relative to manifest)
        let name = generate_dependency_name("agents/helper.md", &source_context);
        assert_eq!(name, "agents/helper");

        // Test nested path
        #[cfg(windows)]
        let nested_path = "C:\\project\\snippets\\python\\utils.md";
        #[cfg(not(windows))]
        let nested_path = "/project/snippets/python/utils.md";
        let name = generate_dependency_name(nested_path, &source_context);
        assert_eq!(name, "snippets/python/utils");
    }

    #[test]
    fn test_generate_dependency_name_git_context() {
        // Test with git source context - paths relative to repository root
        // Use platform-appropriate absolute paths
        #[cfg(windows)]
        let repo_root = Path::new("C:\\repo");
        #[cfg(not(windows))]
        let repo_root = Path::new("/repo");
        let source_context = crate::resolver::source_context::SourceContext::git(repo_root);

        // Test path within repository
        #[cfg(windows)]
        let repo_path = "C:\\repo\\agents\\helper.md";
        #[cfg(not(windows))]
        let repo_path = "/repo/agents/helper.md";
        let name = generate_dependency_name(repo_path, &source_context);
        assert_eq!(name, "agents/helper");

        // Test deeply nested path
        #[cfg(windows)]
        let nested_path = "C:\\repo\\community\\agents\\ai\\python-assistant.md";
        #[cfg(not(windows))]
        let nested_path = "/repo/community/agents/ai/python-assistant.md";
        let name = generate_dependency_name(nested_path, &source_context);
        assert_eq!(name, "community/agents/ai/python-assistant");
    }

    #[test]
    fn test_generate_dependency_name_remote_context() {
        // Test with remote source context - preserves full path structure
        let source_context = crate::resolver::source_context::SourceContext::remote("community");

        // Test remote paths (passed as relative to repo root)
        let name = generate_dependency_name("agents/helper.md", &source_context);
        assert_eq!(name, "agents/helper");

        // Test nested remote path
        let name = generate_dependency_name("snippets/python/async-pattern.md", &source_context);
        assert_eq!(name, "snippets/python/async-pattern");
    }

    #[tokio::test]
    async fn test_expand_local_pattern_with_source_context() {
        // Test integration of pattern expansion with source context
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manifest_dir = temp_dir.path();

        // Create test files
        fs::create_dir_all(manifest_dir.join("agents")).await.unwrap();
        fs::create_dir_all(manifest_dir.join("snippets")).await.unwrap();

        fs::write(manifest_dir.join("agents/helper.md"), "# Helper Agent").await.unwrap();
        fs::write(manifest_dir.join("agents/assistant.md"), "# Assistant Agent").await.unwrap();
        fs::write(manifest_dir.join("snippets/python.md"), "# Python Snippets").await.unwrap();

        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: "agents/*.md".to_string(),
            source: None,
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        // Test pattern expansion with local source context
        let result = expand_local_pattern(
            &dep,
            "agents/*.md",
            crate::core::ResourceType::Agent,
            Some(manifest_dir),
        )
        .await
        .unwrap();

        // Verify we got expected files with correct names
        assert_eq!(result.len(), 2);

        let mut names: Vec<String> = result.iter().map(|(name, _dep)| name.clone()).collect();
        names.sort();

        assert_eq!(names[0], "agents/assistant");
        assert_eq!(names[1], "agents/helper");

        // Verify that the dependencies have the correct paths
        for (name, expanded_dep) in &result {
            // The expanded dependency should have the correct path
            assert!(expanded_dep.get_path().ends_with(".md"));

            // Verify the name matches what we'd expect from generate_dependency_name
            let source_context =
                crate::resolver::source_context::SourceContext::local(manifest_dir);
            let expected_name = generate_dependency_name(expanded_dep.get_path(), &source_context);
            assert_eq!(*name, expected_name);
        }
    }
}
