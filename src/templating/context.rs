//! Template context building for AGPM resource installation.
//!
//! This module provides structures and methods for building the template context
//! that will be available to Markdown templates during rendering.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, to_string, to_value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tera::Context as TeraContext;

use crate::core::ResourceType;
use crate::lockfile::{LockFile, ResourceId};

use super::cache::RenderCache;
use super::content::ContentExtractor;
use super::dependencies::DependencyExtractor;
use super::utils::{deep_merge_json, to_native_path_display};

/// Maximum recursion depth for template rendering to prevent stack overflow.
///
/// This limit prevents infinite recursion or extremely deep dependency chains
/// that could cause stack overflow during template rendering. A depth of 50
/// should be more than sufficient for any realistic use case while protecting
/// against pathological cases.
const MAX_RECURSION_DEPTH: usize = 50;

/// Metadata about the current resource being rendered.
///
/// This struct represents information about the resource that is currently
/// being rendered (available as `agpm.resource` in templates). It contains
/// metadata but NOT content, since the content IS the template being rendered.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResourceMetadata {
    /// Resource type (agent, snippet, command, etc.)
    #[serde(rename = "type")]
    pub resource_type: String,
    /// Logical resource name from manifest
    pub name: String,
    /// Resolved installation path
    pub install_path: String,
    /// Source identifier (if applicable)
    pub source: Option<String>,
    /// Resolved version (if applicable)
    pub version: Option<String>,
    /// Git commit SHA (if applicable)
    pub resolved_commit: Option<String>,
    /// SHA256 checksum of the content
    pub checksum: String,
    /// Source-relative path in repository
    pub path: String,
}

/// Complete data about a dependency for template embedding.
///
/// This struct represents a dependency that can be embedded in templates
/// (available as `agpm.deps.<type>.<name>` in templates). It includes
/// the processed content of the dependency file, ready for embedding.
#[derive(Clone, Serialize, Deserialize)]
pub struct DependencyData {
    /// Resource type (agent, snippet, command, etc.)
    #[serde(rename = "type")]
    pub resource_type: String,
    /// Logical resource name from manifest
    pub name: String,
    /// Resolved installation path
    pub install_path: String,
    /// Source identifier (if applicable)
    pub source: Option<String>,
    /// Resolved version (if applicable)
    pub version: Option<String>,
    /// Git commit SHA (if applicable)
    pub resolved_commit: Option<String>,
    /// SHA256 checksum of the content
    pub checksum: String,
    /// Source-relative path in repository
    pub path: String,
    /// Processed content of the dependency file.
    ///
    /// Contains the file content with metadata stripped and optionally rendered:
    /// - For Markdown: Content without YAML frontmatter
    /// - For JSON: Content without metadata fields
    ///
    /// This enables template embedding via `{{ agpm.deps.<type>.<name>.content }}`.
    ///
    /// Note: This field is large and should not be printed in debug output.
    /// Use the Debug impl which shows only the content length.
    pub content: String,
}

impl std::fmt::Debug for DependencyData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DependencyData")
            .field("resource_type", &self.resource_type)
            .field("name", &self.name)
            .field("install_path", &self.install_path)
            .field("source", &self.source)
            .field("version", &self.version)
            .field("resolved_commit", &self.resolved_commit)
            .field("checksum", &self.checksum)
            .field("path", &self.path)
            .field("content", &format!("<{} bytes>", self.content.len()))
            .finish()
    }
}

/// Template context builder for AGPM resource installation.
///
/// This struct is responsible for building the template context that will be
/// available to Markdown templates during rendering. It collects data from
/// the manifest, lockfile, and current resource being processed.
///
/// # Context Structure
///
/// The built context follows this structure:
/// ```json
/// {
///   "agpm": {
///     "resource": {
///       "type": "agent",
///       "name": "example-agent",
///       "install_path": ".claude/agents/example.md",
///       "source": "community",
///       "version": "v1.0.0",
///       "resolved_commit": "abc123...",
///       "checksum": "sha256:...",
///       "path": "agents/example.md"
///     },
///     "deps": {
///       "agents": {
///         "helper": {
///           "install_path": ".claude/agents/helper.md",
///           "version": "v1.0.0",
///           "resolved_commit": "def456...",
///           "checksum": "sha256:...",
///           "source": "community",
///           "path": "agents/helper.md"
///         }
///       },
///       "snippets": { ... },
///       "commands": { ... }
///     }
///   }
/// }
/// ```
pub struct TemplateContextBuilder {
    /// The lockfile containing resolved resource information
    /// Shared via Arc to avoid expensive clones when building contexts for multiple resources
    lockfile: Arc<LockFile>,
    /// Project-specific template variables from the manifest
    project_config: Option<crate::manifest::ProjectConfig>,
    /// Cache instance for reading source files during content extraction
    /// Shared via Arc to avoid expensive clones
    cache: Arc<crate::cache::Cache>,
    /// Project root directory for resolving local file paths
    project_dir: PathBuf,
    /// Cache of rendered content to avoid re-rendering same dependencies
    /// Shared via `Arc<Mutex>` for safe concurrent access during template rendering
    render_cache: Arc<Mutex<RenderCache>>,
    /// Cache of parsed custom dependency names to avoid re-reading and re-parsing files
    /// Maps resource ID (name@type) to custom name mappings (dep_ref -> custom_name)
    /// Shared via `Arc<Mutex>` for safe concurrent access
    custom_names_cache: Arc<Mutex<HashMap<String, BTreeMap<String, String>>>>,
    /// Cache of parsed dependency specifications to avoid re-reading and re-parsing files
    /// Maps resource ID (name@type) to full DependencySpec objects (dep_ref -> DependencySpec)
    /// Shared via `Arc<Mutex>` for safe concurrent access
    dependency_specs_cache:
        Arc<Mutex<HashMap<String, BTreeMap<String, crate::manifest::DependencySpec>>>>,
}

impl TemplateContextBuilder {
    /// Create a new template context builder.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The resolved lockfile, wrapped in Arc for efficient sharing
    /// * `project_config` - Optional project-specific template variables from the manifest
    /// * `cache` - Cache instance for reading source files during content extraction
    /// * `project_dir` - Project root directory for resolving local file paths
    pub fn new(
        lockfile: Arc<LockFile>,
        project_config: Option<crate::manifest::ProjectConfig>,
        cache: Arc<crate::cache::Cache>,
        project_dir: PathBuf,
    ) -> Self {
        Self {
            lockfile,
            project_config,
            cache,
            project_dir,
            render_cache: Arc::new(Mutex::new(RenderCache::new())),
            custom_names_cache: Arc::new(Mutex::new(HashMap::new())),
            dependency_specs_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Clear the render cache.
    ///
    /// Should be called after installation completes to free memory
    /// and ensure next installation starts with a fresh cache.
    pub fn clear_render_cache(&self) {
        if let Ok(mut cache) = self.render_cache.lock() {
            cache.clear();
        }
    }

    /// Clear the custom names cache.
    ///
    /// Should be called after installation completes to free memory
    /// and ensure next installation starts with a fresh cache.
    pub fn clear_custom_names_cache(&self) {
        if let Ok(mut cache) = self.custom_names_cache.lock() {
            cache.clear();
        }
    }

    /// Clear the dependency specs cache.
    ///
    /// Should be called after installation completes to free memory
    /// and ensure next installation starts with a fresh cache.
    pub fn clear_dependency_specs_cache(&self) {
        if let Ok(mut cache) = self.dependency_specs_cache.lock() {
            cache.clear();
        }
    }

    /// Get render cache statistics.
    ///
    /// Returns (hits, misses, hit_rate) where hit_rate is a percentage.
    pub fn render_cache_stats(&self) -> Option<(usize, usize, f64)> {
        self.render_cache.lock().ok().map(|cache| {
            let (hits, misses) = cache.stats();
            let hit_rate = cache.hit_rate();
            (hits, misses, hit_rate)
        })
    }

    /// Build the complete template context for a specific resource.
    ///
    /// # Arguments
    ///
    /// * `resource_name` - Name of the resource being rendered
    /// * `resource_type` - Type of the resource (agents, snippets, etc.)
    /// * `template_vars_override` - Optional template variable overrides for this specific resource.
    ///   Overrides are deep-merged into the base context, preserving unmodified fields.
    ///
    /// # Returns
    ///
    /// Returns a Tera `Context` containing all available template variables.
    ///
    /// # Template Variable Override Behavior
    ///
    /// When `template_vars_override` is provided, it is deep-merged into the base template context:
    ///
    /// - **Objects**: Recursively merged, preserving fields not present in override
    /// - **Primitives/Arrays**: Completely replaced by override value
    /// - **Null values**: Replace existing value with JSON null (may cause template errors)
    /// - **Empty objects**: No-op (no changes applied)
    ///
    /// Special handling for `project` namespace: Updates both `agpm.project` (canonical)
    /// and top-level `project` (convenience alias) to maintain consistency.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use serde_json::json;
    /// # use agpm_cli::templating::TemplateContextBuilder;
    /// # use agpm_cli::core::ResourceType;
    /// # async fn example(builder: TemplateContextBuilder) -> anyhow::Result<()> {
    /// // Base context has project.name = "agpm" and project.language = "rust"
    /// let overrides = json!({
    ///     "project": {
    ///         "language": "python",  // Replaces existing value
    ///         "framework": "fastapi" // Adds new field
    ///     }
    /// });
    ///
    /// use agpm_cli::lockfile::ResourceId;
    /// use agpm_cli::utils::compute_variant_inputs_hash;
    /// // Create ResourceId with template_vars and ResourceType included
    /// let variant_hash = compute_variant_inputs_hash(&overrides).unwrap_or_default();
    /// let resource_id = ResourceId::new("agent", None::<String>, Some("claude-code"), ResourceType::Agent, variant_hash);
    /// let (context, _context_checksum) = builder
    ///     .build_context(&resource_id, &overrides)
    ///     .await?;
    ///
    /// // Result: project.name preserved, language replaced, framework added
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build_context(
        &self,
        resource_id: &ResourceId,
        variant_inputs: &serde_json::Value,
    ) -> Result<(TeraContext, Option<String>)> {
        // Build the template context as before
        let context = self
            .build_context_with_visited(resource_id, variant_inputs, &mut HashSet::new())
            .await?;

        // Check if resource uses templating (optimized query)
        let uses_templating = self.resource_uses_templating(resource_id).await?;

        // Create optimized context with cached checksum
        let context_with_checksum = ContextWithChecksum::new(context, uses_templating);

        Ok(context_with_checksum.into_tuple())
    }

    /// Check if a resource has templating enabled.
    ///
    /// Returns true if the resource is a Markdown file with `agpm.templating: true`
    /// in its frontmatter. Non-Markdown files always return false.
    async fn resource_uses_templating(&self, resource_id: &ResourceId) -> Result<bool> {
        // Look up resource in lockfile
        let resource = self
            .lockfile
            .find_resource_by_id(resource_id)
            .ok_or_else(|| anyhow!("Resource not found in lockfile"))?;

        // Only Markdown files support templating
        if !resource.path.ends_with(".md") {
            return Ok(false);
        }

        // Determine source path (same logic as ContentExtractor::extract_content)
        let source_path = if let Some(_source_name) = &resource.source {
            let url = resource
                .url
                .as_ref()
                .ok_or_else(|| anyhow!("Resource '{}' has source but no URL", resource.name))?;

            // Check if this is a local directory source
            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local directory source - use URL as path directly
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git-based source - get worktree path
                let sha = resource.resolved_commit.as_deref().ok_or_else(|| {
                    anyhow!("Resource '{}' has no resolved commit", resource.name)
                })?;

                // Use centralized worktree path construction
                let worktree_dir = self.cache.get_worktree_path(url, sha)?;
                worktree_dir.join(&resource.path)
            }
        } else {
            // Local file - path is relative to project or absolute
            let local_path = std::path::Path::new(&resource.path);
            if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir.join(local_path)
            }
        };

        // Read and parse the Markdown file
        // If the file doesn't exist or can't be read, assume templating is disabled
        let content = match tokio::fs::read_to_string(&source_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    "Could not read file for resource '{}' from {}: {}. Assuming templating disabled.",
                    resource.name,
                    source_path.display(),
                    e
                );
                return Ok(false);
            }
        };

        // Parse the markdown document
        // If parsing fails, assume templating is disabled
        let doc = match crate::markdown::MarkdownDocument::parse(&content) {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(
                    "Could not parse markdown for resource '{}': {}. Assuming templating disabled.",
                    resource.name,
                    e
                );
                return Ok(false);
            }
        };

        // Check frontmatter for agpm.templating flag
        Ok(super::content::is_markdown_templating_enabled(doc.metadata.as_ref()))
    }

    /// Build resource metadata for the template context.
    ///
    /// # Arguments
    ///
    /// * `resource` - The locked resource entry (already looked up by full ResourceId)
    fn build_resource_data(&self, resource: &crate::lockfile::LockedResource) -> ResourceMetadata {
        ResourceMetadata {
            resource_type: resource.resource_type.to_string(),
            name: resource.name.clone(),
            install_path: to_native_path_display(&resource.installed_at),
            source: resource.source.clone(),
            version: resource.version.clone(),
            resolved_commit: resource.resolved_commit.clone(),
            checksum: resource.checksum.clone(),
            path: resource.path.clone(),
        }
    }

    /// Compute a stable digest of the template context data.
    ///
    /// This method creates a deterministic hash of all lockfile metadata that could
    /// affect template rendering. The digest is used as part of the cache key to ensure
    /// that changes to dependency versions or metadata properly invalidate the cache.
    ///
    /// # Returns
    ///
    /// Returns a hex-encoded string containing the first 16 characters of the SHA-256
    /// hash of the serialized template context data. This is sufficient to uniquely
    /// identify context changes while keeping the digest compact.
    ///
    /// # What's Included
    ///
    /// The digest includes all lockfile metadata that affects rendering:
    /// - Resource names, types, and installation paths
    /// - Dependency versions and resolved commits
    /// - Checksums and source information
    ///
    /// # Determinism
    ///
    /// The hash is stable across runs because:
    /// - Resources are sorted by type and name before hashing
    /// - JSON serialization uses consistent ordering (BTreeMap)
    /// - Only metadata fields that affect rendering are included
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::templating::TemplateContextBuilder;
    /// use agpm_cli::lockfile::LockFile;
    /// use std::path::{Path, PathBuf};
    /// use std::sync::Arc;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
    /// let cache = Arc::new(agpm_cli::cache::Cache::new()?);
    /// let project_dir = std::env::current_dir()?;
    /// let builder = TemplateContextBuilder::new(
    ///     Arc::new(lockfile),
    ///     None,
    ///     cache,
    ///     project_dir
    /// );
    ///
    /// let digest = builder.compute_context_digest()?;
    /// println!("Template context digest: {}", digest);
    /// # Ok(())
    /// # }
    /// ```
    pub fn compute_context_digest(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;

        // Build a deterministic representation of the lockfile data
        // Use BTreeMap for consistent ordering
        let mut digest_data: BTreeMap<String, BTreeMap<String, BTreeMap<&str, String>>> =
            BTreeMap::new();

        // Process each resource type in a consistent order
        for resource_type in [
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::Script,
            ResourceType::Hook,
            ResourceType::McpServer,
        ] {
            let resources = self.lockfile.get_resources_by_type(&resource_type);
            if resources.is_empty() {
                continue;
            }

            let type_str = resource_type.to_plural().to_string();
            let mut sorted_resources: Vec<_> = resources.iter().collect();
            // Sort by name for deterministic ordering
            sorted_resources.sort_by(|a, b| a.name.cmp(&b.name));

            let mut type_data = BTreeMap::new();
            for resource in sorted_resources {
                // Include only the fields that can affect template rendering
                let mut resource_data: BTreeMap<&str, String> = BTreeMap::new();
                resource_data.insert("name", resource.name.clone());
                resource_data.insert("install_path", resource.installed_at.clone());
                resource_data.insert("path", resource.path.clone());
                resource_data.insert("checksum", resource.checksum.clone());

                // Optional fields - only include if present
                if let Some(ref source) = resource.source {
                    resource_data.insert("source", source.to_string());
                }
                if let Some(ref version) = resource.version {
                    resource_data.insert("version", version.to_string());
                }
                if let Some(ref commit) = resource.resolved_commit {
                    resource_data.insert("resolved_commit", commit.to_string());
                }

                type_data.insert(resource.name.clone(), resource_data);
            }

            digest_data.insert(type_str, type_data);
        }

        // Serialize to JSON for stable representation
        let json_str =
            to_string(&digest_data).context("Failed to serialize template context for digest")?;

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let hash = hasher.finalize();

        // Return first 16 hex characters (64 bits) - sufficient for uniqueness
        Ok(hex::encode(&hash[..8]))
    }
}

/// A cached Tera context with pre-computed checksum for performance.
///
/// This structure optimizes repeated checksum calculations by computing
/// the checksum once when the context is first created, then caching
/// it for subsequent accesses.
#[derive(Debug, Clone)]
pub struct ContextWithChecksum {
    /// The template context
    pub context: TeraContext,
    /// Pre-computed checksum for cache invalidation
    pub checksum: Option<String>,
}

impl ContextWithChecksum {
    /// Create a new context with optional checksum computation.
    ///
    /// The checksum is computed only if `compute_checksum` is true.
    /// This avoids expensive hash calculations for non-templated resources.
    #[must_use]
    pub fn new(context: TeraContext, compute_checksum: bool) -> Self {
        let checksum = if compute_checksum {
            Self::compute_checksum(&context).ok()
        } else {
            None
        };

        Self {
            context,
            checksum,
        }
    }

    /// Compute checksum of a Tera context for cache invalidation.
    ///
    /// Creates a deterministic hash based on the context data structure.
    /// This ensures that changes to template inputs are detected.
    fn compute_checksum(context: &TeraContext) -> Result<String> {
        use crate::utils::canonicalize_json;
        use sha2::{Digest, Sha256};

        // Convert TeraContext to JSON Value using its built-in conversion
        let context_clone = context.clone();
        let json_value = context_clone.into_json();

        // Serialize to deterministic JSON with preserved order
        let json_str = canonicalize_json(&json_value)?;

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let hash = hasher.finalize();

        Ok(format!("sha256:{}", hex::encode(hash)))
    }

    /// Get the context
    #[must_use]
    pub fn context(&self) -> &TeraContext {
        &self.context
    }

    /// Get the checksum (if computed)
    #[must_use]
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    /// Convert to tuple for backward compatibility
    #[must_use]
    pub fn into_tuple(self) -> (TeraContext, Option<String>) {
        (self.context, self.checksum)
    }
}

// Implement ContentExtractor trait for TemplateContextBuilder
impl ContentExtractor for TemplateContextBuilder {
    fn cache(&self) -> &Arc<crate::cache::Cache> {
        &self.cache
    }

    fn project_dir(&self) -> &PathBuf {
        &self.project_dir
    }
}

// Implement DependencyExtractor trait for TemplateContextBuilder
impl DependencyExtractor for TemplateContextBuilder {
    fn lockfile(&self) -> &Arc<LockFile> {
        &self.lockfile
    }

    fn render_cache(&self) -> &Arc<Mutex<RenderCache>> {
        &self.render_cache
    }

    fn custom_names_cache(&self) -> &Arc<Mutex<HashMap<String, BTreeMap<String, String>>>> {
        &self.custom_names_cache
    }

    fn dependency_specs_cache(
        &self,
    ) -> &Arc<Mutex<HashMap<String, BTreeMap<String, crate::manifest::DependencySpec>>>> {
        &self.dependency_specs_cache
    }

    async fn build_dependencies_data(
        &self,
        current_resource: &crate::lockfile::LockedResource,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<BTreeMap<String, BTreeMap<String, DependencyData>>> {
        // Call the builder function from the builders module
        super::dependencies::build_dependencies_data(self, current_resource, rendering_stack).await
    }

    async fn build_context_with_visited(
        &self,
        resource_id: &ResourceId,
        variant_inputs: &serde_json::Value,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<TeraContext> {
        // Check recursion depth to prevent stack overflow
        if rendering_stack.len() >= MAX_RECURSION_DEPTH {
            anyhow::bail!(
                "Maximum recursion depth ({}) exceeded while rendering '{}'. \
                 This likely indicates a complex or cyclic dependency chain. \
                 Current stack contains {} resources.",
                MAX_RECURSION_DEPTH,
                resource_id.name(),
                rendering_stack.len()
            );
        }

        tracing::info!(
            "Starting context build for '{}' (type: {:?}, depth: {})",
            resource_id.name(),
            resource_id.resource_type(),
            rendering_stack.len()
        );

        let mut context = TeraContext::new();

        // Build the nested agpm structure
        let mut agpm = Map::new();

        // Get the current resource to access its declared dependencies
        let current_resource =
            self.lockfile.find_resource_by_id(resource_id).with_context(|| {
                format!(
                    "Resource '{}' of type {:?} not found in lockfile (source: {:?}, tool: {:?})",
                    resource_id.name(),
                    resource_id.resource_type(),
                    resource_id.source(),
                    resource_id.tool()
                )
            })?;

        tracing::info!(
            "Found resource '{}' with {} dependencies",
            resource_id.name(),
            current_resource.dependencies.len()
        );

        // Build current resource data (using already-looked-up resource to preserve full identity)
        let resource_data = self.build_resource_data(current_resource);
        agpm.insert("resource".to_string(), to_value(resource_data)?);

        // Build dependency data from ALL lockfile resources + current resource's declared dependencies
        tracing::info!("Building dependencies data for '{}'...", resource_id.name());

        // Build dependencies using the dependency builder
        let deps_data = self.build_dependencies_data(current_resource, rendering_stack).await?;
        tracing::info!("Successfully built dependencies data with {} types", deps_data.len());
        agpm.insert("deps".to_string(), to_value(deps_data)?);

        // Add project variables if available
        if let Some(ref project_config) = self.project_config {
            let project_json = project_config.to_json_value();
            agpm.insert("project".to_string(), project_json.clone());

            // Also add at top level for convenience (will be overridden by template_vars if provided)
            context.insert("project", &project_json);
        }

        // Insert the complete agpm object
        context.insert("agpm", &agpm);

        // Apply template variable overrides if provided (non-empty variant_inputs)
        if let Some(overrides_obj) = variant_inputs.as_object() {
            if !overrides_obj.is_empty() {
                tracing::debug!(
                    "Applying template variable overrides for resource '{}'",
                    resource_id.name()
                );

                // Convert context to JSON for merging
                let mut context_json = context.clone().into_json();

                // Iterate through all keys in variant_inputs and merge them
                for (key, value) in overrides_obj {
                    if key == "project" {
                        // Project vars need to be in both agpm.project and top-level project
                        let original_project = context_json
                            .get("agpm")
                            .and_then(|v| v.as_object())
                            .and_then(|o| o.get("project"))
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                        let merged_project = deep_merge_json(original_project, value);

                        // Update agpm.project
                        if let Some(agpm_obj) =
                            context_json.get_mut("agpm").and_then(|v| v.as_object_mut())
                        {
                            agpm_obj.insert("project".to_string(), merged_project.clone());
                        }

                        // Update top-level project
                        // SAFETY: context.into_json() always produces an object at the top level
                        context_json
                            .as_object_mut()
                            .expect("context JSON must be an object")
                            .insert("project".to_string(), merged_project);
                    } else {
                        // Other vars go to top-level context only
                        let original = context_json
                            .get(key)
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        let merged = deep_merge_json(original, value);

                        // SAFETY: context.into_json() always produces an object at the top level
                        context_json
                            .as_object_mut()
                            .expect("context JSON must be an object")
                            .insert(key.clone(), merged);
                    }
                }

                // Replace context with merged result
                context = TeraContext::from_serialize(&context_json)
                    .context("Failed to create context from merged template variables")?;

                tracing::debug!(
                    "Applied template overrides: {}",
                    serde_json::to_string_pretty(&variant_inputs)
                        .unwrap_or_else(|_| "{}".to_string())
                );
            }
        }

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_with_checksum_computation() {
        let mut context = TeraContext::new();
        context.insert("test", "value");
        context.insert("number", &42);

        // Test with checksum computation enabled
        let ctx_with_checksum = ContextWithChecksum::new(context.clone(), true);

        assert!(ctx_with_checksum.checksum().is_some(), "Checksum should be computed when enabled");
        assert_eq!(ctx_with_checksum.context(), &context, "Context should be preserved");

        // Verify checksum is deterministic
        let ctx_with_checksum2 = ContextWithChecksum::new(context.clone(), true);
        assert_eq!(
            ctx_with_checksum.checksum(),
            ctx_with_checksum2.checksum(),
            "Checksum should be deterministic for same context"
        );
    }

    #[test]
    fn test_context_with_checksum_disabled() {
        let mut context = TeraContext::new();
        context.insert("test", "value");

        // Test with checksum computation disabled
        let ctx_with_checksum = ContextWithChecksum::new(context.clone(), false);

        assert!(
            ctx_with_checksum.checksum().is_none(),
            "Checksum should not be computed when disabled"
        );
        assert_eq!(ctx_with_checksum.context(), &context, "Context should be preserved");
    }

    #[test]
    fn test_context_with_checksum_different_contexts() {
        let mut context1 = TeraContext::new();
        context1.insert("test", "value1");

        let mut context2 = TeraContext::new();
        context2.insert("test", "value2");

        let ctx1 = ContextWithChecksum::new(context1, true);
        let ctx2 = ContextWithChecksum::new(context2, true);

        assert_ne!(
            ctx1.checksum(),
            ctx2.checksum(),
            "Different contexts should have different checksums"
        );
    }

    #[test]
    fn test_context_with_checksum_into_tuple() {
        let mut context = TeraContext::new();
        context.insert("test", "value");

        let ctx_with_checksum = ContextWithChecksum::new(context.clone(), true);
        let (returned_context, returned_checksum) = ctx_with_checksum.into_tuple();

        assert_eq!(returned_context, context, "Returned context should match original");
        assert!(returned_checksum.is_some(), "Returned checksum should be present");
    }

    #[test]
    fn test_context_with_checksum_complex_structure() {
        let mut context = TeraContext::new();

        // Add nested structure to test comprehensive checksum calculation
        context.insert("simple", "value");
        context.insert("number", &42);
        context.insert("boolean", &true);

        let ctx_with_checksum = ContextWithChecksum::new(context, true);

        assert!(ctx_with_checksum.checksum().is_some(), "Complex context should produce checksum");

        // Verify checksum format
        let checksum = ctx_with_checksum.checksum().unwrap();
        assert!(checksum.starts_with("sha256:"), "Checksum should have sha256: prefix");
        assert_eq!(checksum.len(), 7 + 64, "SHA256 hex should be 64 characters plus prefix");
    }
}
