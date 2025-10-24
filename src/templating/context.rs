//! Template context building for AGPM resource installation.
//!
//! This module provides structures and methods for building the template context
//! that will be available to Markdown templates during rendering.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, to_string, to_value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tera::Context as TeraContext;

use crate::core::ResourceType;
use crate::lockfile::{LockFile, ResourceId};

use super::cache::RenderCache;
use super::content::ContentExtractor;
use super::dependencies::DependencyExtractor;
use super::utils::{deep_merge_json, to_native_path_display};

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
    /// Shared via Arc<Mutex> for safe concurrent access during template rendering
    render_cache: Arc<Mutex<RenderCache>>,
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
    /// // Create ResourceId with template_vars and ResourceType included
    /// let resource_id = ResourceId::new("agent", None::<String>, Some("claude-code"), ResourceType::Agent, overrides);
    /// let context = builder
    ///     .build_context(&resource_id)
    ///     .await?;
    ///
    /// // Result: project.name preserved, language replaced, framework added
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build_context(&self, resource_id: &ResourceId) -> Result<TeraContext> {
        self.build_context_with_visited(resource_id, &mut HashSet::new()).await
    }

    /// Build resource metadata for the template context.
    ///
    /// # Arguments
    ///
    /// * `resource_name` - Name of the resource
    /// * `resource_type` - Type of the resource
    fn build_resource_data(
        &self,
        resource_name: &str,
        resource_type: ResourceType,
    ) -> Result<ResourceMetadata> {
        let entry =
            self.lockfile.find_resource(resource_name, &resource_type).with_context(|| {
                format!(
                    "Resource '{}' of type {:?} not found in lockfile",
                    resource_name, resource_type
                )
            })?;

        Ok(ResourceMetadata {
            resource_type: resource_type.to_string(),
            name: resource_name.to_string(),
            install_path: to_native_path_display(&entry.installed_at),
            source: entry.source.clone(),
            version: entry.version.clone(),
            resolved_commit: entry.resolved_commit.clone(),
            checksum: entry.checksum.clone(),
            path: entry.path.clone(),
        })
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

    async fn build_context_with_visited(
        &self,
        resource_id: &ResourceId,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<TeraContext> {
        tracing::info!(
            "Starting context build for '{}' (type: {:?})",
            resource_id.name(),
            resource_id.resource_type()
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

        // Build current resource data
        let resource_data =
            self.build_resource_data(resource_id.name(), resource_id.resource_type())?;
        agpm.insert("resource".to_string(), to_value(resource_data)?);

        // Build dependency data from ALL lockfile resources + current resource's declared dependencies
        tracing::info!("Building dependencies data for '{}'...", resource_id.name());
        let deps_data = self
            .build_dependencies_data(current_resource, rendering_stack)
            .await
            .with_context(|| {
                format!(
                    "Failed to build dependencies data for resource '{}' (type: {:?})",
                    resource_id.name(),
                    resource_id.resource_type()
                )
            })?;
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

        // Apply template variable overrides if provided
        if let Some(overrides) = resource_id.template_vars() {
            tracing::debug!(
                "Applying template variable overrides for resource '{}'",
                resource_id.name()
            );

            // Convert context to JSON for merging
            let mut context_json = context.clone().into_json();

            // Iterate through all keys in template_vars and merge them
            for (key, value) in overrides.as_object().unwrap_or(&serde_json::Map::new()) {
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
                serde_json::to_string_pretty(&overrides).unwrap_or_else(|_| "{}".to_string())
            );
        }

        Ok(context)
    }
}
