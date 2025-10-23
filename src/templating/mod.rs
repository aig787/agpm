//! Markdown templating engine for AGPM resources.
//!
//! This module provides Tera-based templating functionality for Markdown resources,
//! enabling dynamic content generation during installation. It supports safe, sandboxed
//! template rendering with a rich context containing installation metadata.
//!
//! # Overview
//!
//! The templating system allows resource authors to:
//! - Reference other resources by name and type
//! - Access resolved installation paths and versions
//! - Use conditional logic and loops in templates
//! - Read and embed project-specific files (style guides, best practices, etc.)
//!
//! # Template Context
//!
//! Templates are rendered with a structured context containing:
//! - `agpm.resource`: Current resource information (name, type, install path, etc.)
//! - `agpm.deps`: Nested dependency information by resource type and name
//!
//! # Custom Filters
//!
//! - `content`: Read project-specific files (e.g., `{{ 'docs/guide.md' | content }}`)
//!
//! # Syntax Restrictions
//!
//! For security and safety, the following Tera features are disabled:
//! - `{% include %}` tags (no file system access)
//! - `{% extends %}` tags (no template inheritance)
//! - `{% import %}` tags (no external template imports)
//! - Custom functions that access the file system or network (except content filter)
//!
//! # Supported Features
//!
//! - Variable substitution: `{{ agpm.resource.install_path }}`
//! - Conditional logic: `{% if agpm.resource.source %}...{% endif %}`
//! - Loops: `{% for name, dep in agpm.deps.agents %}...{% endfor %}`
//! - Standard Tera filters (string manipulation, formatting)
//! - Project file embedding: `{{ 'path/to/file.md' | content }}`
//! - Literal blocks: Protect template syntax from rendering for documentation
//!
//! # Literal Blocks (Documentation Mode)
//!
//! When writing documentation that includes template syntax examples, you can use
//! `literal` fenced code blocks to protect the content from being rendered:
//!
//! ````markdown
//! # Template Documentation
//!
//! Here's how to use template variables:
//!
//! ```literal
//! {{ agpm.deps.snippets.example.content }}
//! ```
//!
//! The above syntax will be displayed literally, not rendered.
//! ````
//!
//! This is particularly useful for:
//! - Documentation snippets that show template syntax examples
//! - Tutorial content that explains how to use templates
//! - Example code that should not be executed during rendering
//!
//! The content inside `literal` blocks will be:
//! 1. Protected from template rendering (preserved as-is)
//! 2. Wrapped in standard markdown code fences in the output
//! 3. Displayed literally to the end user
//!
//! # Examples
//!
//! ## Basic Variable Substitution
//!
//! ```markdown
//! # {{ agpm.resource.name }}
//!
//! This agent is installed at: `{{ agpm.resource.install_path }}`
//! Version: {{ agpm.resource.version }}
//! ```
//!
//! ## Dependency Content Embedding (v0.4.7+)
//!
//! All dependencies automatically have `.content` field with processed content:
//!
//! ```markdown
//! ---
//! agpm.templating: true
//! dependencies:
//!   snippets:
//!     - path: snippets/best-practices.md
//!       name: best_practices
//! ---
//! # Code Reviewer
//!
//! ## Best Practices
//! {{ agpm.deps.snippets.best_practices.content }}
//! ```
//!
//! ## Project File Filter (v0.4.8+)
//!
//! Read project-specific files using the `content` filter:
//!
//! ```markdown
//! ---
//! agpm.templating: true
//! ---
//! # Team Agent
//!
//! ## Project Style Guide
//! {{ 'project/styleguide.md' | content }}
//!
//! ## Team Conventions
//! {{ 'docs/conventions.txt' | content }}
//! ```
//!
//! ## Combining Dependency Content + Project Files
//!
//! Use both features together for maximum flexibility:
//!
//! ```markdown
//! ---
//! agpm.templating: true
//! dependencies:
//!   snippets:
//!     - path: snippets/rust-patterns.md
//!       name: rust_patterns
//!     - path: snippets/error-handling.md
//!       name: error_handling
//! ---
//! # Rust Code Reviewer
//!
//! ## Shared Patterns (from AGPM repository)
//! {{ agpm.deps.snippets.rust_patterns.content }}
//!
//! ## Project-Specific Style Guide
//! {{ 'project/rust-style.md' | content }}
//!
//! ## Error Handling Best Practices
//! {{ agpm.deps.snippets.error_handling.content }}
//!
//! ## Team Conventions
//! {{ 'docs/team-conventions.txt' | content }}
//! ```
//!
//! **When to use each**:
//! - **Dependency content**: Versioned, shared resources from AGPM repos
//! - **Project files**: Team-specific, project-local documentation
//!
//! ## Literal Blocks for Documentation
//!
//! When creating documentation snippets that explain template syntax, use
//! `literal` blocks to prevent the examples from being rendered:
//!
//! ````markdown
//! ---
//! agpm.templating: true
//! ---
//! # AGPM Template Guide
//!
//! ## How to Embed Snippet Content
//!
//! To embed a snippet's content in your template, use this syntax:
//!
//! ```literal
//! {{ agpm.deps.snippets.best_practices.content }}
//! ```
//!
//! This will render the **current agent name**: {{ agpm.resource.name }}
//!
//! ## How to Loop Over Dependencies
//!
//! ```literal
//! {% for name, dep in agpm.deps.agents %}
//! - {{ name }}: {{ dep.version }}
//! {% endfor %}
//! ```
//!
//! The syntax examples above are displayed literally, while the agent name
//! below is dynamically rendered based on the context.
//! ````
//!
//! In this example:
//! - The `literal` blocks show template syntax examples without rendering them
//! - Regular template variables like `{{ agpm.resource.name }}` are still rendered
//! - This allows documentation to demonstrate template features while using them
//!
//! ## Recursive Project Files
//!
//! Project files can reference other project files (up to 10 levels):
//!
//! **Main agent** (`.claude/agents/reviewer.md`):
//! ```markdown
//! ---
//! agpm.templating: true
//! ---
//! # Code Reviewer
//!
//! {{ 'project/styleguide.md' | content }}
//! ```
//!
//! **Style guide** (`project/styleguide.md`):
//! ```markdown
//! # Coding Standards
//!
//! ## Rust-Specific Rules
//! {{ 'project/rust-style.md' | content }}
//! ```
//!
//! ## Dependency References
//!
//! Dependencies are accessible by name in the template context. The name is determined by:
//! 1. For manifest deps: the key in `[agents]`, `[snippets]`, etc.
//! 2. For transitive deps: the `name` field if specified, otherwise derived from path
//!
//! ```markdown
//! ## Dependencies
//!
//! This agent uses the following helper:
//! - {{ agpm.deps.snippets.helper.install_path }}
//!
//! {% if agpm.deps.agents %}
//! ## Related Agents
//! {% for agent in agpm.deps.agents %}
//! - {{ agent.name }} ({{ agent.version }})
//! {% endfor %}
//! {% endif %}
//! ```
//!
//! ### Custom Names for Transitive Dependencies
//!
//! ```yaml
//! ---
//! dependencies:
//!   agents:
//!     - path: "../shared/complex-path/helper.md"
//!       name: "helper"  # Use "helper" instead of deriving from path
//! ---
//! ```
//!
//! ## Conditional Content
//!
//! ```markdown
//! {% if agpm.resource.source == "community" %}
//! This resource is from the community repository.
//! {% elif agpm.resource.source %}
//! This resource is from the {{ agpm.resource.source }} source.
//! {% else %}
//! This is a local resource.
//! {% endif %}
//! ```

pub mod filters;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, to_string, to_value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tera::{Context as TeraContext, Tera};

use crate::core::ResourceType;
use crate::lockfile::LockFile;

/// Sentinel markers used to guard non-templated dependency content.
/// Content enclosed between these markers should be treated as literal text
/// and never passed through the templating engine.
const NON_TEMPLATED_LITERAL_GUARD_START: &str = "__AGPM_LITERAL_RAW_START__";
const NON_TEMPLATED_LITERAL_GUARD_END: &str = "__AGPM_LITERAL_RAW_END__";

/// Perform a deep merge of two JSON values.
///
/// Recursively merges `overrides` into `base`. For objects, fields from `overrides`
/// are added or replace fields in `base`. For arrays and primitives, `overrides`
/// completely replaces `base`.
///
/// # Arguments
///
/// * `base` - The base JSON value
/// * `overrides` - The override values to merge into base
///
/// # Returns
///
/// Returns the merged JSON value.
///
/// # Examples
///
/// ```rust,no_run
/// use serde_json::json;
/// use agpm_cli::templating::deep_merge_json;
///
/// let base = json!({ "project": { "name": "agpm", "language": "rust" } });
/// let overrides = json!({ "project": { "language": "python", "framework": "fastapi" } });
///
/// let result = deep_merge_json(base, &overrides);
/// // result: { "project": { "name": "agpm", "language": "python", "framework": "fastapi" } }
/// ```
pub fn deep_merge_json(mut base: Value, overrides: &Value) -> Value {
    match (base.as_object_mut(), overrides.as_object()) {
        (Some(base_obj), Some(override_obj)) => {
            // Both are objects - recursively merge
            for (key, override_value) in override_obj {
                match base_obj.get_mut(key) {
                    Some(base_value) if base_value.is_object() && override_value.is_object() => {
                        // Recursively merge nested objects
                        let merged = deep_merge_json(base_value.clone(), override_value);
                        base_obj.insert(key.clone(), merged);
                    }
                    _ => {
                        // For non-objects or missing keys, override completely
                        base_obj.insert(key.clone(), override_value.clone());
                    }
                }
            }
            base
        }
        (_, _) => {
            // If override is not an object, or base is not an object, override replaces base
            overrides.clone()
        }
    }
}

/// Convert Unix-style path (from lockfile) to platform-native format for display in templates.
///
/// Lockfiles always use Unix-style forward slashes for cross-platform compatibility,
/// but when rendering templates, we want to show paths in the platform's native format
/// so users see `.claude\agents\helper.md` on Windows and `.claude/agents/helper.md` on Unix.
///
/// # Arguments
///
/// * `unix_path` - Path string with forward slashes (from lockfile)
///
/// # Returns
///
/// Platform-native path string (backslashes on Windows, forward slashes on Unix)
///
/// # Examples
///
/// ```
/// # use agpm_cli::templating::to_native_path_display;
/// #[cfg(windows)]
/// assert_eq!(to_native_path_display(".claude/agents/test.md"), ".claude\\agents\\test.md");
///
/// #[cfg(not(windows))]
/// assert_eq!(to_native_path_display(".claude/agents/test.md"), ".claude/agents/test.md");
/// ```
pub fn to_native_path_display(unix_path: &str) -> String {
    #[cfg(windows)]
    {
        unix_path.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        unix_path.to_string()
    }
}

/// Cache key for rendered template content.
///
/// Uniquely identifies a rendered version of a resource based on:
/// - The source file path (canonical path to the resource)
/// - The resource type (Agent, Snippet, Command, etc.)
/// - Template variable overrides (hashed for efficient comparison)
///
/// This ensures that the same resource with different template_vars
/// produces different cache entries, while identical resources share
/// cached content across multiple parent resources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderCacheKey {
    /// Canonical path to the resource file in the source repository
    resource_path: String,
    /// Resource type (Agent, Snippet, etc.)
    resource_type: ResourceType,
    /// Hash of template_vars JSON (None for resources without overrides)
    template_vars_hash: Option<u64>,
}

impl RenderCacheKey {
    /// Create a new cache key with template variable hash
    fn new(
        resource_path: String,
        resource_type: ResourceType,
        template_vars: Option<&Value>,
    ) -> Self {
        let template_vars_hash = template_vars.map(|vars| {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            // Serialize to stable JSON string for hashing
            let json_str = serde_json::to_string(vars).unwrap_or_default();
            let mut hasher = DefaultHasher::new();
            json_str.hash(&mut hasher);
            hasher.finish()
        });

        Self {
            resource_path,
            resource_type,
            template_vars_hash,
        }
    }
}

/// Cache for rendered template content during installation.
///
/// This cache stores rendered content to avoid re-rendering the same
/// dependencies multiple times. It lives for the duration of a single
/// install operation and is cleared afterward.
///
/// # Performance Impact
///
/// For installations with many transitive dependencies (e.g., 145+ resources),
/// this cache prevents O(N²) rendering complexity by ensuring each unique
/// resource is rendered only once, regardless of how many parents depend on it.
///
/// # Cache Invalidation
///
/// The cache is cleared after each installation completes. It does not
/// persist across operations, ensuring that file changes are always reflected
/// in subsequent installations.
#[derive(Debug, Default)]
struct RenderCache {
    /// Map from cache key to rendered content
    cache: HashMap<RenderCacheKey, String>,
    /// Cache statistics
    hits: usize,
    misses: usize,
}

impl RenderCache {
    /// Create a new empty render cache
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Get cached rendered content if available
    fn get(&mut self, key: &RenderCacheKey) -> Option<&String> {
        if let Some(content) = self.cache.get(key) {
            self.hits += 1;
            Some(content)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert rendered content into the cache
    fn insert(&mut self, key: RenderCacheKey, content: String) {
        self.cache.insert(key, content);
    }

    /// Clear all cached content
    fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    /// Calculate hit rate as a percentage
    fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
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

/// Template renderer with Tera engine and custom functions.
///
/// This struct wraps a Tera instance with AGPM-specific configuration,
/// custom functions, and filters. It provides a safe, sandboxed environment
/// for rendering Markdown templates.
///
/// # Security
///
/// The renderer is configured with security restrictions:
/// - No file system access via includes/extends (except content filter)
/// - No network access
/// - Sandboxed template execution
/// - Custom functions are carefully vetted
/// - Project file access restricted to project directory with validation
pub struct TemplateRenderer {
    /// The underlying Tera template engine
    tera: Tera,
    /// Whether templating is enabled globally
    enabled: bool,
}

/// Metadata about a resource for template context.
///
/// This struct represents the information available about a resource
/// in the template context. It includes both the resource's own metadata
/// and its resolved installation information.
#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceTemplateData {
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
    /// Processed content of the resource file.
    ///
    /// Contains the file content with metadata stripped:
    /// - For Markdown: Content without YAML frontmatter
    /// - For JSON: Content without metadata fields
    ///
    /// This field is available for all dependencies, enabling template
    /// embedding via `{{ agpm.deps.<type>.<name>.content }}`.
    ///
    /// Note: This field is large and should not be printed in debug output.
    /// Use the Debug impl which shows only the content length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl std::fmt::Debug for ResourceTemplateData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceTemplateData")
            .field("resource_type", &self.resource_type)
            .field("name", &self.name)
            .field("install_path", &self.install_path)
            .field("source", &self.source)
            .field("version", &self.version)
            .field("resolved_commit", &self.resolved_commit)
            .field("checksum", &self.checksum)
            .field("path", &self.path)
            .field("content", &self.content.as_ref().map(|c| format!("<{} bytes>", c.len())))
            .finish()
    }
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
    /// let context = builder
    ///     .build_context("agent", ResourceType::Agent, Some(&overrides))
    ///     .await?;
    ///
    /// // Result: project.name preserved, language replaced, framework added
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build_context(
        &self,
        resource_id: &crate::lockfile::ResourceId,
        resource_type: ResourceType,
    ) -> Result<TeraContext> {
        self.build_context_with_visited(
            resource_id,
            resource_type,
            &mut std::collections::HashSet::new(),
        )
        .await
    }

    async fn build_context_with_visited(
        &self,
        resource_id: &crate::lockfile::ResourceId,
        resource_type: ResourceType,
        rendering_stack: &mut std::collections::HashSet<String>,
    ) -> Result<TeraContext> {
        tracing::info!(
            "[BUILD_CONTEXT] Starting context build for '{}' (type: {:?})",
            resource_id.name,
            resource_type
        );

        let mut context = TeraContext::new();

        // Build the nested agpm structure
        let mut agpm = Map::new();

        // Get the current resource to access its declared dependencies
        let current_resource = self.lockfile.find_resource_by_id(resource_id).with_context(|| {
            format!(
                "Resource '{}' of type {:?} not found in lockfile (source: {:?}, tool: {:?})",
                resource_id.name, resource_type, resource_id.source, resource_id.tool
            )
        })?;

        tracing::info!(
            "[BUILD_CONTEXT] Found resource '{}' with {} dependencies",
            resource_id.name,
            current_resource.dependencies.len()
        );

        // Build current resource data
        let resource_data = self.build_resource_data(&resource_id.name, resource_type)?;
        agpm.insert("resource".to_string(), to_value(resource_data)?);

        // Build dependency data from ALL lockfile resources + current resource's declared dependencies
        tracing::info!(
            "[BUILD_CONTEXT] Building dependencies data for '{}'...",
            resource_id.name
        );
        let deps_data = self.build_dependencies_data(current_resource, rendering_stack).await
            .with_context(|| {
                format!(
                    "Failed to build dependencies data for resource '{}' (type: {:?})",
                    resource_id.name,
                    resource_type
                )
            })?;
        tracing::info!(
            "[BUILD_CONTEXT] Successfully built dependencies data with {} types",
            deps_data.len()
        );
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
        if let Some(overrides) = &resource_id.template_vars {
            tracing::debug!(
                "Applying template variable overrides for resource '{}'",
                resource_id.name
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
                serde_json::to_string_pretty(overrides).unwrap_or_else(|_| "{}".to_string())
            );
        }

        Ok(context)
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
    ) -> Result<ResourceTemplateData> {
        let entry =
            self.lockfile.find_resource(resource_name, resource_type).with_context(|| {
                format!(
                    "Resource '{}' of type {:?} not found in lockfile",
                    resource_name, resource_type
                )
            })?;

        Ok(ResourceTemplateData {
            resource_type: resource_type.to_string(),
            name: resource_name.to_string(),
            install_path: to_native_path_display(&entry.installed_at),
            source: entry.source.clone(),
            version: entry.version.clone(),
            resolved_commit: entry.resolved_commit.clone(),
            checksum: entry.checksum.clone(),
            path: entry.path.clone(),
            content: None, // Will be populated when content extraction is implemented
        })
    }

    /// Extract and process content from a resource file.
    ///
    /// Reads the source file and processes it based on file type:
    /// - Markdown (.md): Strips YAML frontmatter, returns content only
    /// - JSON (.json): Removes metadata fields like `dependencies`
    /// - Other files: Returns raw content
    ///
    /// # Arguments
    ///
    /// * `resource` - The locked resource to extract content from
    ///
    /// # Returns
    ///
    /// Returns `Some(content)` if extraction succeeded, `None` on error (with warning logged)
    async fn extract_content(&self, resource: &crate::lockfile::LockedResource) -> Option<String> {
        tracing::debug!(
            "Attempting to extract content for resource '{}' (type: {:?})",
            resource.name,
            resource.resource_type
        );

        // Determine source path
        let source_path = if let Some(source_name) = &resource.source {
            let url = resource.url.as_ref()?;

            // Check if this is a local directory source
            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            tracing::debug!(
                "Resource '{}': source='{}', url='{}', is_local={}",
                resource.name,
                source_name,
                url,
                is_local_source
            );

            if is_local_source {
                // Local directory source - use URL as path directly
                let path = std::path::PathBuf::from(url).join(&resource.path);
                tracing::debug!("Using local source path: {}", path.display());
                path
            } else {
                // Git-based source - get worktree path
                let sha = resource.resolved_commit.as_deref()?;

                tracing::debug!(
                    "Resource '{}': Getting worktree for SHA {}...",
                    resource.name,
                    &sha[..8.min(sha.len())]
                );

                // Use centralized worktree path construction
                let worktree_dir = match self.cache.get_worktree_path(url, sha) {
                    Ok(path) => {
                        tracing::debug!("Worktree path: {}", path.display());
                        path
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to construct worktree path for resource '{}': {}",
                            resource.name,
                            e
                        );
                        return None;
                    }
                };

                let full_path = worktree_dir.join(&resource.path);
                tracing::debug!(
                    "Full source path for '{}': {} (worktree exists: {})",
                    resource.name,
                    full_path.display(),
                    worktree_dir.exists()
                );
                full_path
            }
        } else {
            // Local file - path is relative to project or absolute
            let local_path = std::path::Path::new(&resource.path);
            let resolved_path = if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir.join(local_path)
            };

            tracing::debug!(
                "Resource '{}': Using local file path: {}",
                resource.name,
                resolved_path.display()
            );

            resolved_path
        };

        // Read file content
        let content = match tokio::fs::read_to_string(&source_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to read content for resource '{}' from {}: {}",
                    resource.name,
                    source_path.display(),
                    e
                );
                return None;
            }
        };

        // Process based on file type
        let processed_content = if resource.path.ends_with(".md") {
            // Markdown: strip frontmatter and guard non-templated content that contains template syntax
            match crate::markdown::MarkdownDocument::parse(&content) {
                Ok(doc) => {
                    let templating_enabled =
                        Self::is_markdown_templating_enabled(doc.metadata.as_ref());
                    let mut stripped_content = doc.content;

                    if !templating_enabled
                        && Self::content_contains_template_syntax(&stripped_content)
                    {
                        tracing::debug!(
                            "Protecting non-templated markdown content for '{}'",
                            resource.name
                        );
                        stripped_content = Self::wrap_content_in_literal_guard(stripped_content);
                    }

                    stripped_content
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse markdown for resource '{}': {}. Using raw content.",
                        resource.name,
                        e
                    );
                    content
                }
            }
        } else if resource.path.ends_with(".json") {
            // JSON: parse and remove metadata fields
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(mut json) => {
                    if let Some(obj) = json.as_object_mut() {
                        // Remove metadata fields that shouldn't be in embedded content
                        obj.remove("dependencies");
                    }
                    serde_json::to_string_pretty(&json).unwrap_or(content)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse JSON for resource '{}': {}. Using raw content.",
                        resource.name,
                        e
                    );
                    content
                }
            }
        } else {
            // Other files: use raw content
            content
        };

        Some(processed_content)
    }

    /// Determine whether templating is explicitly enabled in Markdown frontmatter.
    fn is_markdown_templating_enabled(
        metadata: Option<&crate::markdown::MarkdownMetadata>,
    ) -> bool {
        metadata
            .and_then(|md| md.extra.get("agpm"))
            .and_then(|agpm| agpm.as_object())
            .and_then(|agpm_obj| agpm_obj.get("templating"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    /// Detect if content contains Tera template syntax markers.
    fn content_contains_template_syntax(content: &str) -> bool {
        content.contains("{{") || content.contains("{%") || content.contains("{#")
    }

    /// Wrap non-templated content in a literal fence so it renders safely without being evaluated.
    fn wrap_content_in_literal_guard(content: String) -> String {
        let mut wrapped = String::with_capacity(
            content.len()
                + NON_TEMPLATED_LITERAL_GUARD_START.len()
                + NON_TEMPLATED_LITERAL_GUARD_END.len()
                + 2, // newline separators
        );

        wrapped.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
        wrapped.push('\n');
        wrapped.push_str(&content);
        if !content.ends_with('\n') {
            wrapped.push('\n');
        }
        wrapped.push_str(NON_TEMPLATED_LITERAL_GUARD_END);

        wrapped
    }

    /// Extract custom dependency names from a resource's frontmatter.
    ///
    /// Parses the resource file to extract the `dependencies` declaration with `name:` fields
    /// and maps dependency references to their custom names.
    ///
    /// # Returns
    ///
    /// A HashMap mapping dependency references (e.g., "snippet/rust-best-practices") to custom
    /// names (e.g., "best_practices") as declared in the resource's YAML frontmatter.
    async fn extract_dependency_custom_names(
        &self,
        resource: &crate::lockfile::LockedResource,
    ) -> HashMap<String, String> {
        let mut custom_names = HashMap::new();

        // Get the resolved dependencies from the lockfile
        // These are in the format "type/name" where name is the resolved path
        let lockfile_deps = &resource.dependencies;

        // Debug: log ALL resources to understand what's being processed
        if !lockfile_deps.is_empty() {
            tracing::info!(
                "[EXTRACT_CUSTOM] Processing resource '{}' (type: {:?}) with {} lockfile dependencies",
                resource.name,
                resource.resource_type,
                lockfile_deps.len()
            );
            for dep in lockfile_deps {
                tracing::info!("  [EXTRACT_CUSTOM] Lockfile dep: '{}'", dep);
            }
        }

        // Build a lookup structure upfront to avoid O(n³) nested loops
        // Map: type -> Vec<(basename, full_dep_ref)>
        let mut lockfile_lookup: HashMap<&str, Vec<(String, String)>> = HashMap::new();

        for lockfile_dep_ref in lockfile_deps {
            // Parse lockfile dependency ref: "type/name" or "type/name@version"
            let parts: Vec<&str> = lockfile_dep_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }

            let lockfile_type = parts[0];
            // Strip version suffix if present (format: name@version)
            let lockfile_name = parts[1].split('@').next().unwrap_or(parts[1]);

            // Extract basename from lockfile name
            let lockfile_basename = std::path::Path::new(lockfile_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(lockfile_name)
                .to_string();

            lockfile_lookup
                .entry(lockfile_type)
                .or_insert_with(Vec::new)
                .push((lockfile_basename, lockfile_dep_ref.to_string()));
        }

        // Determine source path (same logic as extract_content)
        let source_path = if let Some(_source_name) = &resource.source {
            // Has source - check if local or Git
            let url = match resource.url.as_ref() {
                Some(u) => u,
                None => return custom_names,
            };

            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local source
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git source
                let sha = match resource.resolved_commit.as_deref() {
                    Some(s) => s,
                    None => return custom_names,
                };
                match self.cache.get_worktree_path(url, sha) {
                    Ok(worktree_dir) => worktree_dir.join(&resource.path),
                    Err(_) => return custom_names,
                }
            }
        } else {
            // Local file
            let local_path = std::path::Path::new(&resource.path);
            if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir.join(local_path)
            }
        };

        // Read and parse the file based on type
        if resource.path.ends_with(".md") {
            // Parse markdown frontmatter
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                if let Ok(doc) = crate::markdown::MarkdownDocument::parse(&content) {
                    if let Some(metadata) = doc.metadata {
                        // Extract dependencies from frontmatter
                        if let Some(deps_map) = metadata.dependencies {
                            // Process each resource type (agents, snippets, commands, etc.)
                            for (resource_type_str, deps_array) in deps_map {
                                // Convert frontmatter type to lockfile type (singular)
                                let lockfile_type = match resource_type_str.as_str() {
                                    "agents" | "agent" => "agent",
                                    "snippets" | "snippet" => "snippet",
                                    "commands" | "command" => "command",
                                    "scripts" | "script" => "script",
                                    "hooks" | "hook" => "hook",
                                    "mcp-servers" | "mcp-server" => "mcp-server",
                                    _ => continue, // Skip unknown types
                                };

                                // Get lockfile entries for this type only (O(1) lookup instead of O(n) iteration)
                                let type_entries = match lockfile_lookup.get(lockfile_type) {
                                    Some(entries) => entries,
                                    None => continue, // No lockfile deps of this type
                                };

                                // deps_array is Vec<DependencySpec>
                                for dep_spec in deps_array {
                                    let path = &dep_spec.path;
                                    if let Some(custom_name) = &dep_spec.name {
                                        // Extract basename from the path (without extension)
                                        let basename = std::path::Path::new(path)
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or(path);

                                        tracing::info!(
                                            "[EXTRACT_CUSTOM] Found custom name '{}' for path '{}' (basename: '{}')",
                                            custom_name,
                                            path,
                                            basename
                                        );

                                        // Check if basename has template variables
                                        if basename.contains("{{") {
                                            // Template variable in basename - try suffix matching
                                            // e.g., "{{ agpm.project.language }}-best-practices" -> "-best-practices"
                                            if let Some(static_suffix_start) = basename.find("}}") {
                                                let static_suffix = &basename[static_suffix_start + 2..];

                                                // Search for any lockfile basename ending with this suffix
                                                for (lockfile_basename, lockfile_dep_ref) in type_entries {
                                                    if lockfile_basename.ends_with(static_suffix) {
                                                        custom_names.insert(
                                                            lockfile_dep_ref.clone(),
                                                            custom_name.to_string(),
                                                        );
                                                    }
                                                }
                                            }
                                        } else {
                                            // No template variables - exact basename match (O(n) but only within type)
                                            for (lockfile_basename, lockfile_dep_ref) in type_entries {
                                                if lockfile_basename == basename {
                                                    custom_names.insert(
                                                        lockfile_dep_ref.clone(),
                                                        custom_name.to_string(),
                                                    );
                                                    break; // Found exact match, no need to continue
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // TODO: Add JSON support if needed

        custom_names
    }

    /// Generate dependency name from a path (matching resolver logic).
    ///
    /// For local transitive dependencies, the resolver uses the full relative path
    /// (without extension) as the resource name to maintain uniqueness.
    #[allow(dead_code)]
    fn generate_dependency_name_from_path(&self, path: &str) -> String {
        // Strip file extension - this matches what the resolver stores as the name
        path.strip_suffix(".md").or_else(|| path.strip_suffix(".json")).unwrap_or(path).to_string()
    }

    /// Helper function to add a custom name alias to the dependencies map.
    ///
    /// This function searches for an already-processed resource in the `deps` map and creates
    /// an alias entry with the custom name. The resource should have already been added to
    /// `deps` with its path-based key during the main processing loop.
    ///
    /// Note: This function doesn't need to do lockfile lookups with ResourceId because it
    /// searches within the already-built `deps` map. The deps map was built from the lockfile
    /// with all the correct template_vars and content.
    fn add_custom_alias(
        deps: &mut BTreeMap<String, BTreeMap<String, ResourceTemplateData>>,
        dep_ref: &str,
        custom_name: &str,
    ) {
        // Parse dependency reference format: "type/name" or "type/name@version"
        let parts: Vec<&str> = dep_ref.splitn(2, '/').collect();
        if parts.len() != 2 {
            tracing::debug!(
                "Skipping invalid dep_ref format '{}' for custom name '{}'",
                dep_ref,
                custom_name
            );
            return;
        }

        let dep_type_str = parts[0];
        // Strip version suffix if present (format: name@version)
        let dep_name = parts[1].split('@').next().unwrap_or(parts[1]);

        // Convert to ResourceType enum to get plural form
        let dep_type = match dep_type_str {
            "agent" => ResourceType::Agent,
            "snippet" => ResourceType::Snippet,
            "command" => ResourceType::Command,
            "script" => ResourceType::Script,
            "hook" => ResourceType::Hook,
            "mcp-server" => ResourceType::McpServer,
            _ => {
                tracing::debug!(
                    "Skipping unknown resource type '{}' in dep_ref '{}' for custom name '{}'",
                    dep_type_str,
                    dep_ref,
                    custom_name
                );
                return;
            }
        };

        let type_str_plural = dep_type.to_plural().to_string();

        // Search for the resource in the deps map (already populated from lockfile)
        if let Some(type_deps) = deps.get_mut(&type_str_plural) {
            // The resource should already exist in the map with its path-based key
            // Find it by matching the ResourceTemplateData.name field (which is the lockfile name)
            let existing_data = type_deps
                .values()
                .find(|data| {
                    // Match by the actual lockfile resource name
                    data.name == dep_name
                })
                .cloned();

            if let Some(data) = existing_data {
                // Sanitize the alias (replace hyphens with underscores for Tera)
                let sanitized_alias = custom_name.replace('-', "_");

                tracing::info!(
                    "[ADD_ALIAS] ✓ Added {} alias '{}' -> resource '{}' (path: {})",
                    type_str_plural,
                    sanitized_alias,
                    dep_name,
                    data.path
                );

                // Add an alias entry pointing to the same data
                type_deps.insert(sanitized_alias.clone(), data);
            } else {
                tracing::error!(
                    "[ADD_ALIAS] ❌ NOT FOUND: {} resource '{}' for alias '{}'.\n  \
                    Dep ref: '{}'\n  \
                    Available {} (first 5): {}",
                    type_str_plural,
                    dep_name,
                    custom_name,
                    dep_ref,
                    type_deps.len(),
                    type_deps.iter().take(5).map(|(k, v)| format!("'{}' (name='{}')", k, v.name)).collect::<Vec<_>>().join(", ")
                );
            }
        } else {
            tracing::debug!(
                "Resource type '{}' not found in deps map when adding custom alias '{}' for '{}'",
                type_str_plural,
                custom_name,
                dep_ref
            );
        }
    }

    /// Build dependency data for the template context.
    ///
    /// This creates a nested structure containing:
    /// 1. ALL resources from the lockfile (path-based names) - for universal access
    /// 2. Current resource's declared dependencies (custom alias names) - for scoped access
    ///
    /// This dual approach ensures:
    /// - Any resource can access any other resource via path-based names
    /// - Resources can use custom aliases for their dependencies without collisions
    ///
    /// # Arguments
    ///
    /// * `current_resource` - The resource being rendered (for scoped alias mapping)
    async fn build_dependencies_data(
        &self,
        current_resource: &crate::lockfile::LockedResource,
        rendering_stack: &mut std::collections::HashSet<String>,
    ) -> Result<BTreeMap<String, BTreeMap<String, ResourceTemplateData>>> {
        let mut deps = BTreeMap::new();

        // Helper closure to process a single resource
        let process_resource = |resource: &crate::lockfile::LockedResource,
                                dep_type: ResourceType|
         -> (String, String, ResourceTemplateData) {
            let type_str_plural = dep_type.to_plural().to_string();
            let type_str_singular = dep_type.to_string();

            // Determine the key to use for universal access in the template context
            // DO NOT use manifest_alias - it's only for pattern aliases from manifest,
            // not transitive custom names which are extracted during template rendering
            let key_name = if resource.name.contains('/') || resource.name.contains('\\') {
                // Name looks like a path - extract basename without extension
                std::path::Path::new(&resource.name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&resource.name)
                    .to_string()
            } else {
                // Use name as-is
                resource.name.clone()
            };

            // Sanitize the key name by replacing hyphens with underscores
            // to avoid Tera interpreting them as minus operators
            let sanitized_key = key_name.replace('-', "_");

            (
                type_str_plural,
                sanitized_key,
                ResourceTemplateData {
                    resource_type: type_str_singular,
                    name: resource.name.clone(),
                    install_path: to_native_path_display(&resource.installed_at),
                    source: resource.source.clone(),
                    version: resource.version.clone(),
                    resolved_commit: resource.resolved_commit.clone(),
                    checksum: resource.checksum.clone(),
                    path: resource.path.clone(),
                    content: None, // Will be filled in asynchronously
                },
            )
        };

        // Collect ALL transitive dependencies (not just direct dependencies!)
        // Use a set to track which dependencies we've already added to avoid duplicates
        let mut resources_to_process: Vec<(&crate::lockfile::LockedResource, ResourceType, bool)> = Vec::new();
        let mut visited_dep_ids = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<String> =
            current_resource.dependencies.iter().cloned().collect();

        while let Some(dep_id) = queue.pop_front() {
            // Skip if we've already processed this dependency
            if !visited_dep_ids.insert(dep_id.clone()) {
                continue;
            }

            // Parse dependency ID format: "type/name" or "type/name@version"
            // (e.g., "snippet/helper", "agent/foo@v1.0.0")
            if let Some((type_str, name_with_version)) = dep_id.split_once('/') {
                // Strip the version suffix if present (format: "name@version")
                let name = if let Some((base_name, _version)) = name_with_version.split_once('@') {
                    base_name
                } else {
                    name_with_version
                };
                // Convert type string to ResourceType
                let resource_type = match type_str {
                    "agent" => ResourceType::Agent,
                    "snippet" => ResourceType::Snippet,
                    "command" => ResourceType::Command,
                    "script" => ResourceType::Script,
                    "hook" => ResourceType::Hook,
                    "mcp-server" => ResourceType::McpServer,
                    _ => {
                        tracing::warn!(
                            "Unknown resource type '{}' in dependency '{}' for resource '{}'",
                            type_str,
                            dep_id,
                            current_resource.name
                        );
                        continue;
                    }
                };

                // Look up the dependency in the lockfile
                if let Some(dep_resource) = self.lockfile.find_resource(name, resource_type) {
                    // Add this dependency to resources to process (true = declared dependency)
                    resources_to_process.push((dep_resource, resource_type, true));

                    tracing::debug!(
                        "  [TRANSITIVE] Found dependency '{}' with {} dependencies: {:?}",
                        name,
                        dep_resource.dependencies.len(),
                        dep_resource.dependencies
                    );

                    // Add its dependencies to the queue for recursive processing
                    for transitive_dep in &dep_resource.dependencies {
                        queue.push_back(transitive_dep.clone());
                    }
                } else {
                    tracing::warn!(
                        "Dependency '{}' (type: {:?}) not found in lockfile for resource '{}'",
                        name,
                        resource_type,
                        current_resource.name
                    );
                }
            } else {
                tracing::warn!(
                    "Invalid dependency ID format '{}' for resource '{}' (expected 'type/name')",
                    dep_id,
                    current_resource.name
                );
            }
        }

        // Add ALL lockfile resources (not just transitive dependencies)
        // This ensures templates can reference any resource in the lockfile
        // These are added with is_dependency=false so they don't get rendered recursively

        // Track which resources we've already added to avoid duplicates
        let mut already_added: std::collections::HashSet<(String, ResourceType)> =
            resources_to_process.iter()
                .map(|(r, rt, _)| (r.name.clone(), *rt))
                .collect();

        for resource in &self.lockfile.agents {
            if already_added.insert((resource.name.clone(), ResourceType::Agent)) {
                resources_to_process.push((resource, ResourceType::Agent, false));
            }
        }
        for resource in &self.lockfile.commands {
            if already_added.insert((resource.name.clone(), ResourceType::Command)) {
                resources_to_process.push((resource, ResourceType::Command, false));
            }
        }
        for resource in &self.lockfile.snippets {
            if already_added.insert((resource.name.clone(), ResourceType::Snippet)) {
                resources_to_process.push((resource, ResourceType::Snippet, false));
            }
        }
        for resource in &self.lockfile.scripts {
            if already_added.insert((resource.name.clone(), ResourceType::Script)) {
                resources_to_process.push((resource, ResourceType::Script, false));
            }
        }
        for resource in &self.lockfile.hooks {
            if already_added.insert((resource.name.clone(), ResourceType::Hook)) {
                resources_to_process.push((resource, ResourceType::Hook, false));
            }
        }
        for resource in &self.lockfile.mcp_servers {
            if already_added.insert((resource.name.clone(), ResourceType::McpServer)) {
                resources_to_process.push((resource, ResourceType::McpServer, false));
            }
        }

        tracing::debug!(
            "Building dependencies data with {} total resources from lockfile",
            resources_to_process.len()
        );

        // Debug: log all resources being processed
        for (resource, dep_type, is_dep) in &resources_to_process {
            tracing::debug!(
                "  [LOCKFILE] Resource: {} (type: {:?}, install: {:?}, is_dependency: {})",
                resource.name,
                dep_type,
                resource.install,
                is_dep
            );
        }

        // Get current resource ID for filtering
        let current_resource_id =
            format!("{}::{:?}", current_resource.name, current_resource.resource_type);

        // Process each resource (excluding the current resource to prevent self-reference)
        for (resource, dep_type, is_dependency) in &resources_to_process {
            let resource_id = format!("{}::{:?}", resource.name, dep_type);

            // Skip if this is the current resource (prevent self-dependency)
            if resource_id == current_resource_id {
                tracing::debug!(
                    "  Skipping current resource: {} (preventing self-reference)",
                    resource.name
                );
                continue;
            }

            tracing::debug!("  Processing resource: {} ({})", resource.name, dep_type);

            let (type_str_plural, sanitized_key, mut template_data) =
                process_resource(resource, *dep_type);

            // Extract and render content from source file
            // Declared dependencies should be rendered with their own context before being made available
            // Non-dependencies just get raw content extraction (to avoid circular dependency issues)
            let raw_content = self.extract_content(resource).await;

            // Check if the dependency should be rendered
            // Only render if this is a declared dependency AND content has template syntax
            let should_render = if *is_dependency {
                if let Some(content) = &raw_content {
                    // Don't render if content has literal guards (from templating: false)
                    if content.contains(NON_TEMPLATED_LITERAL_GUARD_START) {
                        false
                    } else {
                        // Only render if the content has template syntax
                        Self::content_contains_template_syntax(content)
                    }
                } else {
                    false
                }
            } else {
                // Not a declared dependency - don't render to avoid circular deps
                false
            };

            if should_render {
                // Build cache key to check if we've already rendered this exact resource
                let cache_key = RenderCacheKey::new(
                    resource.path.clone(),
                    *dep_type,
                    resource.template_vars.as_ref(),
                );

                // Check cache first
                if let Ok(mut cache) = self.render_cache.lock() {
                    if let Some(cached_content) = cache.get(&cache_key) {
                        tracing::debug!(
                            "Render cache hit for '{}' ({})",
                            resource.name,
                            dep_type
                        );
                        template_data.content = Some(cached_content.clone());

                        // Insert into the nested structure and continue to next resource
                        let type_deps = deps.entry(type_str_plural.clone()).or_insert_with(BTreeMap::new);
                        type_deps.insert(sanitized_key.clone(), template_data);

                        tracing::debug!(
                            "  Added cached resource: {}[{}] -> {}",
                            type_str_plural,
                            sanitized_key,
                            resource.path
                        );
                        continue;
                    }
                }

                tracing::debug!(
                    "Render cache miss for '{}' ({}), rendering...",
                    resource.name,
                    dep_type
                );

                // Check if we're already rendering this dependency (cycle detection)
                let dep_id = format!("{}::{:?}", resource.name, dep_type);
                if rendering_stack.contains(&dep_id) {
                    let chain: Vec<String> = rendering_stack.iter().cloned().collect();
                    anyhow::bail!(
                        "Circular dependency detected while rendering '{}'. \
                        Dependency chain: {} -> {}",
                        resource.name,
                        chain.join(" -> "),
                        dep_id
                    );
                }

                // Add to rendering stack
                rendering_stack.insert(dep_id.clone());

                // Build a template context for this dependency so it can be rendered with its own dependencies
                let dep_resource_id = crate::lockfile::ResourceId {
                    name: resource.name.clone(),
                    source: resource.source.clone(),
                    tool: resource.tool.clone(),
                    template_vars: resource.template_vars.clone(),
                };
                let render_result = Box::pin(self.build_context_with_visited(
                    &dep_resource_id,
                    *dep_type,
                    rendering_stack,
                ))
                .await;

                // Remove from stack after rendering (whether success or failure)
                rendering_stack.remove(&dep_id);

                match render_result {
                    Ok(dep_context) => {
                        // Render the dependency's content
                        if let Some(content) = raw_content {
                            let mut renderer = TemplateRenderer::new(
                                true,
                                self.project_dir.clone(),
                                None,
                            ).with_context(|| {
                                format!(
                                    "Failed to create template renderer for dependency '{}' (type: {:?})",
                                    resource.name,
                                    dep_type
                                )
                            })?;

                            let rendered = renderer.render_template(&content, &dep_context)
                                .with_context(|| {
                                    format!(
                                        "Failed to render dependency '{}' (type: {:?}). \
                                        This is a HARD FAILURE - dependency content MUST render successfully.\n\
                                        Resource: {} (source: {}, path: {})",
                                        resource.name,
                                        dep_type,
                                        resource.name,
                                        resource.source.as_deref().unwrap_or("local"),
                                        resource.path
                                    )
                                })?;

                            tracing::debug!(
                                "Successfully rendered dependency content for '{}'",
                                resource.name
                            );

                            // Store in cache for future use
                            if let Ok(mut cache) = self.render_cache.lock() {
                                cache.insert(cache_key.clone(), rendered.clone());
                                tracing::debug!(
                                    "Stored rendered content in cache for '{}'",
                                    resource.name
                                );
                            }

                            template_data.content = Some(rendered);
                        } else {
                            // No content extracted - set to None explicitly
                            template_data.content = None;
                        }
                    }
                    Err(e) => {
                        // Hard failure - context building must succeed for dependency rendering
                        return Err(e.context(format!(
                            "Failed to build template context for dependency '{}' (type: {:?}). \
                            This is a HARD FAILURE - all dependencies must have valid contexts.\n\
                            Resource: {} (source: {}, path: {})",
                            resource.name,
                            dep_type,
                            resource.name,
                            resource.source.as_deref().unwrap_or("local"),
                            resource.path
                        )));
                    }
                }
            } else {
                // No rendering needed, use raw content (guards will be collapsed after parent renders)
                template_data.content = raw_content;
            }

            // Insert into the nested structure
            let type_deps = deps.entry(type_str_plural.clone()).or_insert_with(BTreeMap::new);
            type_deps.insert(sanitized_key.clone(), template_data);

            tracing::debug!(
                "  Added resource: {}[{}] -> {}",
                type_str_plural,
                sanitized_key,
                resource.path
            );
        }

        // Add custom alias mappings for the entire dependency tree
        // Each resource in the tree defines custom names for its own dependencies,
        // and we need all of them available when rendering (because embedded content
        // from transitive dependencies may reference their own named dependencies).
        tracing::debug!(
            "Extracting custom dependency names from entire dependency tree for: '{}'",
            current_resource.name
        );

        // Walk the dependency tree and collect custom names from each resource
        let mut to_process: Vec<String> = current_resource.dependencies.clone();
        let mut processed = std::collections::HashSet::new();

        // Also process the current resource itself
        let current_custom_names = self.extract_dependency_custom_names(current_resource).await;
        if !current_custom_names.is_empty() || current_resource.name.contains("golang") {
            tracing::info!(
                "[CUSTOM_NAMES] Extracted {} custom names from current resource '{}' (type: {:?})",
                current_custom_names.len(),
                current_resource.name,
                current_resource.resource_type
            );
            for (dep_ref, custom_name) in &current_custom_names {
                tracing::info!("  [CUSTOM_NAMES] Will add alias: '{}' -> '{}'", dep_ref, custom_name);
            }
        }
        for (dep_ref, custom_name) in current_custom_names {
            Self::add_custom_alias(&mut deps, &dep_ref, &custom_name);
        }

        // Process all transitive dependencies
        while let Some(dep_ref) = to_process.pop() {
            if !processed.insert(dep_ref.clone()) {
                continue; // Already processed
            }

            // Parse dependency reference format: "type/name"
            let parts: Vec<&str> = dep_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }

            let dep_type_str = parts[0];
            let dep_name = parts[1];

            // Convert to ResourceType enum
            let dep_type = match dep_type_str {
                "agent" => ResourceType::Agent,
                "snippet" => ResourceType::Snippet,
                "command" => ResourceType::Command,
                "script" => ResourceType::Script,
                "hook" => ResourceType::Hook,
                "mcp-server" => ResourceType::McpServer,
                _ => continue,
            };

            // Find the dependency resource in the lockfile
            // Note: We search by name only since dep_ref doesn't include template_vars.
            // The first match should be correct for extracting transitive custom names,
            // as custom names apply to all variants of a resource.
            let dep_resource = match self.lockfile.find_resource(dep_name, dep_type) {
                Some(res) => res,
                None => {
                    tracing::warn!(
                        "Dependency '{}' not found in lockfile for '{}'",
                        dep_ref,
                        current_resource.name
                    );
                    continue;
                }
            };

            // Extract custom names from this dependency (for ITS dependencies)
            let dep_custom_names = self.extract_dependency_custom_names(dep_resource).await;
            for (child_dep_ref, custom_name) in dep_custom_names {
                Self::add_custom_alias(&mut deps, &child_dep_ref, &custom_name);
            }

            // Add this dependency's own dependencies to the queue
            to_process.extend(dep_resource.dependencies.clone());
        }

        // Debug: Print what we built
        tracing::debug!(
            "Built dependencies data with {} resource types for '{}'",
            deps.len(),
            current_resource.name
        );
        for (resource_type, resources) in &deps {
            tracing::debug!("  Type {}: {} resources", resource_type, resources.len());
            if resource_type == "snippets" {
                for (key, data) in resources {
                    tracing::debug!("    - key='{}', name='{}', path='{}'", key, data.name, data.path);
                }
            } else {
                for name in resources.keys() {
                    tracing::debug!("    - {}", name);
                }
            }
        }

        Ok(deps)
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
            let resources = self.lockfile.get_resources_by_type(resource_type);
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

impl TemplateRenderer {
    /// Create a new template renderer with AGPM-specific configuration.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether templating is enabled globally
    /// * `project_dir` - Project root directory for content filter validation
    /// * `max_content_file_size` - Maximum file size in bytes for content filter (None for no limit)
    ///
    /// # Returns
    ///
    /// Returns a configured `TemplateRenderer` instance with custom filters registered.
    ///
    /// # Filters
    ///
    /// The following custom filters are registered:
    /// - `content`: Read project-specific files with path validation and size limits
    pub fn new(
        enabled: bool,
        project_dir: PathBuf,
        max_content_file_size: Option<u64>,
    ) -> Result<Self> {
        let mut tera = Tera::default();

        // Register custom filters
        tera.register_filter(
            "content",
            filters::create_content_filter(project_dir.clone(), max_content_file_size),
        );

        Ok(Self {
            tera,
            enabled,
        })
    }

    /// Protect literal blocks from template rendering by replacing them with placeholders.
    ///
    /// This method scans for ```literal fenced code blocks and replaces them with
    /// unique placeholders that won't be affected by template rendering. The original
    /// content is stored in a HashMap that can be used to restore the blocks later.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to process
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - Modified content with placeholders instead of literal blocks
    /// - HashMap mapping placeholder IDs to original content
    ///
    /// # Examples
    ///
    /// ````markdown
    /// # Documentation Example
    ///
    /// Use this syntax in templates:
    ///
    /// ```literal
    /// {{ agpm.deps.snippets.example.content }}
    /// ```
    /// ````
    ///
    /// The content inside the literal block will be protected from rendering.
    fn protect_literal_blocks(&self, content: &str) -> (String, HashMap<String, String>) {
        let mut placeholders = HashMap::new();
        let mut counter = 0;
        let mut result = String::with_capacity(content.len());

        // Split content by lines to find both ```literal fences and RAW guards
        let mut in_literal_fence = false;
        let mut in_raw_guard = false;
        let mut current_block = String::new();
        let lines = content.lines();

        for line in lines {
            let trimmed = line.trim();

            if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                // Start of RAW guard block
                in_raw_guard = true;
                current_block.clear();
                tracing::debug!("Found start of RAW guard block");
                // Skip the guard line
            } else if in_raw_guard && trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                // End of RAW guard block
                in_raw_guard = false;

                // Generate unique placeholder
                let placeholder_id = format!("__AGPM_LITERAL_BLOCK_{}__", counter);
                counter += 1;

                // Store original content (keep the guards for later processing)
                let guarded_content = format!(
                    "{}\n{}\n{}",
                    NON_TEMPLATED_LITERAL_GUARD_START,
                    current_block,
                    NON_TEMPLATED_LITERAL_GUARD_END
                );
                placeholders.insert(placeholder_id.clone(), guarded_content);

                // Insert placeholder
                result.push_str(&placeholder_id);
                result.push('\n');

                tracing::debug!(
                    "Protected RAW guard block with placeholder {} ({} bytes)",
                    placeholder_id,
                    current_block.len()
                );

                current_block.clear();
                // Skip the guard line
            } else if in_raw_guard {
                // Inside RAW guard - accumulate content
                if !current_block.is_empty() {
                    current_block.push('\n');
                }
                current_block.push_str(line);
            } else if trimmed.starts_with("```literal") {
                // Start of ```literal fence
                in_literal_fence = true;
                current_block.clear();
                tracing::debug!("Found start of literal fence");
                // Skip the fence line
            } else if in_literal_fence && trimmed.starts_with("```") {
                // End of ```literal fence
                in_literal_fence = false;

                // Generate unique placeholder
                let placeholder_id = format!("__AGPM_LITERAL_BLOCK_{}__", counter);
                counter += 1;

                // Store original content
                placeholders.insert(placeholder_id.clone(), current_block.clone());

                // Insert placeholder
                result.push_str(&placeholder_id);
                result.push('\n');

                tracing::debug!(
                    "Protected literal fence with placeholder {} ({} bytes)",
                    placeholder_id,
                    current_block.len()
                );

                current_block.clear();
                // Skip the fence line
            } else if in_literal_fence {
                // Inside ```literal fence - accumulate content
                if !current_block.is_empty() {
                    current_block.push('\n');
                }
                current_block.push_str(line);
            } else {
                // Regular content - pass through
                result.push_str(line);
                result.push('\n');
            }
        }

        // Handle unclosed blocks (add back as-is)
        if in_literal_fence {
            tracing::warn!("Unclosed literal fence found - treating as regular content");
            result.push_str("```literal\n");
            result.push_str(&current_block);
        }
        if in_raw_guard {
            tracing::warn!("Unclosed RAW guard found - treating as regular content");
            result.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
            result.push('\n');
            result.push_str(&current_block);
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        tracing::debug!("Protected {} literal block(s)", placeholders.len());
        (result, placeholders)
    }

    /// Restore literal blocks by replacing placeholders with original content.
    ///
    /// This method takes rendered content and restores any literal blocks that were
    /// protected during the rendering process.
    ///
    /// # Arguments
    ///
    /// * `content` - The rendered content containing placeholders
    /// * `placeholders` - HashMap mapping placeholder IDs to original content
    ///
    /// # Returns
    ///
    /// Returns the content with placeholders replaced by original literal blocks,
    /// wrapped in markdown code fences for proper display.
    fn restore_literal_blocks(
        &self,
        content: &str,
        placeholders: HashMap<String, String>,
    ) -> String {
        let mut result = content.to_string();

        for (placeholder_id, original_content) in placeholders {
            if original_content.starts_with(NON_TEMPLATED_LITERAL_GUARD_START) {
                result = result.replace(&placeholder_id, &original_content);
            } else {
                // Wrap in markdown code fence for display
                let replacement = format!("```\n{}\n```", original_content);
                result = result.replace(&placeholder_id, &replacement);
            }

            tracing::debug!(
                "Restored literal block {} ({} bytes)",
                placeholder_id,
                original_content.len()
            );
        }

        result
    }

    /// Collapse literal fences that were injected to protect non-templated dependency content.
    ///
    /// Any block that starts with ```literal, contains the sentinel marker on its first line,
    /// and ends with ``` will be replaced by the inner content without the sentinel or fences.
    fn collapse_non_templated_literal_guards(content: String) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_guard = false;

        for chunk in content.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches(['\r', '\n']);

            if !in_guard {
                if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                    in_guard = true;
                } else {
                    result.push_str(chunk);
                }
            } else if trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                in_guard = false;
            } else {
                result.push_str(chunk);
            }
        }

        // If guard never closed, re-append the start marker and captured content to avoid dropping data.
        if in_guard {
            result.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
        }

        result
    }

    /// Render a Markdown template with the given context.
    ///
    /// This method supports recursive template rendering where project files
    /// can reference other project files using the `content` filter.
    /// Rendering continues up to [`filters::MAX_RENDER_DEPTH`] levels deep.
    ///
    /// # Arguments
    ///
    /// * `template_content` - The raw Markdown template content
    /// * `context` - The template context containing variables
    ///
    /// # Returns
    ///
    /// Returns the rendered Markdown content.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Template syntax is invalid
    /// - Context variables are missing
    /// - Custom functions/filters fail
    /// - Recursive rendering exceeds maximum depth (10 levels)
    ///
    /// # Literal Blocks
    ///
    /// Content wrapped in ```literal fences will be protected from
    /// template rendering and displayed literally:
    ///
    /// ````markdown
    /// ```literal
    /// {{ agpm.deps.snippets.example.content }}
    /// ```
    /// ````
    ///
    /// This is useful for documentation that shows template syntax examples.
    ///
    /// # Recursive Rendering
    ///
    /// When a template contains `content` filter references, those files
    /// may themselves contain template syntax. The renderer automatically
    /// detects this and performs multiple rendering passes until either:
    /// - No template syntax remains in the output
    /// - Maximum depth is reached (error)
    ///
    /// Example recursive template chain:
    /// ```markdown
    /// # Main Agent
    /// {{ 'docs/guide.md' | content }}
    /// ```
    ///
    /// Where `docs/guide.md` contains:
    /// ```markdown
    /// # Guide
    /// {{ 'docs/common.md' | content }}
    /// ```
    ///
    /// This will render up to 10 levels deep.
    pub fn render_template(
        &mut self,
        template_content: &str,
        context: &TeraContext,
    ) -> Result<String> {
        tracing::debug!("render_template called, enabled={}", self.enabled);

        if !self.enabled {
            // If templating is disabled, return content as-is
            tracing::debug!("Templating disabled, returning content as-is");
            return Ok(template_content.to_string());
        }

        // Step 1: Protect literal blocks before any rendering
        let (protected_content, placeholders) = self.protect_literal_blocks(template_content);

        // Check if content contains template syntax (after protecting literals)
        if !self.contains_template_syntax(&protected_content) {
            // No template syntax found, restore literals and return
            tracing::debug!(
                "No template syntax found after protecting literals, returning content"
            );
            return Ok(self.restore_literal_blocks(&protected_content, placeholders));
        }

        // Log the template context for debugging
        tracing::debug!("Rendering template with context");
        Self::log_context_as_kv(context);

        // Step 2: Multi-pass rendering for recursive templates
        // This allows project files to reference other project files
        let mut current_content = protected_content;
        let mut depth = 0;
        let max_depth = filters::MAX_RENDER_DEPTH;

        let rendered = loop {
            depth += 1;

            // Check depth limit
            if depth > max_depth {
                bail!(
                    "Template rendering exceeded maximum recursion depth of {}. \
                     This usually indicates circular dependencies between project files. \
                     Please check your content filter references for cycles.",
                    max_depth
                );
            }

            tracing::debug!("Rendering pass {} of max {}", depth, max_depth);

            // Render the current content
            let rendered = self.tera.render_str(&current_content, context).map_err(|e| {
                // Extract detailed error information from Tera error
                let error_msg = Self::format_tera_error(&e);

                // Output the detailed error to stderr for immediate visibility
                eprintln!("Template rendering error:\n{}", error_msg);

                // Include the context in the error message for user visibility
                let context_str = Self::format_context_as_string(context);
                anyhow::Error::new(e).context(format!(
                    "Template rendering failed at depth {}:\n{}\n\nTemplate context:\n{}",
                    depth, error_msg, context_str
                ))
            })?;

            // Check if the rendered output still contains template syntax OUTSIDE code fences
            // This prevents re-rendering template syntax that was embedded as code examples
            if !self.contains_template_syntax_outside_fences(&rendered) {
                // No more template syntax outside fences - we're done with rendering
                tracing::debug!("Template rendering complete after {} pass(es)", depth);
                break rendered;
            }

            // More template syntax found outside fences - prepare for next iteration
            tracing::debug!("Template syntax detected in output, continuing to pass {}", depth + 1);
            current_content = rendered;
        };

        // Step 3: Restore literal blocks after all rendering is complete
        let restored = self.restore_literal_blocks(&rendered, placeholders);

        // Step 4: Collapse any literal guards that were added for non-templated dependencies
        Ok(Self::collapse_non_templated_literal_guards(restored))
    }

    /// Format a Tera error with detailed information about what went wrong.
    ///
    /// Tera errors can contain various types of issues:
    /// - Missing variables (e.g., "Variable `foo` not found")
    /// - Syntax errors (e.g., "Unexpected end of template")
    /// - Filter/function errors (e.g., "Filter `unknown` not found")
    ///
    /// This function extracts the root cause and formats it in a user-friendly way,
    /// filtering out unhelpful internal template names like '__tera_one_off'.
    ///
    /// # Arguments
    ///
    /// * `error` - The Tera error to format
    fn format_tera_error(error: &tera::Error) -> String {
        use std::error::Error;

        let mut messages = Vec::new();

        // Walk the entire error chain and collect all messages
        let mut all_messages = vec![error.to_string()];
        let mut current_error: Option<&dyn Error> = error.source();
        while let Some(err) = current_error {
            all_messages.push(err.to_string());
            current_error = err.source();
        }

        // Process messages to extract useful information
        for msg in all_messages {
            // Clean up the message by removing internal template names
            let cleaned = msg
                .replace("while rendering '__tera_one_off'", "")
                .replace("Failed to render '__tera_one_off'", "Template rendering failed")
                .replace("Failed to parse '__tera_one_off'", "Template syntax error")
                .replace("'__tera_one_off'", "template")
                .trim()
                .to_string();

            // Only keep non-empty, useful messages
            if !cleaned.is_empty()
                && cleaned != "Template rendering failed"
                && cleaned != "Template syntax error"
            {
                messages.push(cleaned);
            }
        }

        // If we got useful messages, return them
        if !messages.is_empty() {
            messages.join("\n  → ")
        } else {
            // Fallback: extract just the error kind
            "Template syntax error (see details above)".to_string()
        }
    }

    /// Format the template context as a string for error messages.
    ///
    /// # Arguments
    ///
    /// * `context` - The Tera context to format
    fn format_context_as_string(context: &TeraContext) -> String {
        let context_clone = context.clone();
        let json_value = context_clone.into_json();
        let mut output = String::new();

        // Recursively format the JSON structure with indentation
        fn format_value(key: &str, value: &serde_json::Value, indent: usize) -> Vec<String> {
            let prefix = "  ".repeat(indent);
            let mut lines = Vec::new();

            match value {
                serde_json::Value::Object(map) => {
                    lines.push(format!("{}{}:", prefix, key));
                    for (k, v) in map {
                        lines.extend(format_value(k, v, indent + 1));
                    }
                }
                serde_json::Value::Array(arr) => {
                    lines.push(format!("{}{}: [{} items]", prefix, key, arr.len()));
                    // Only show first few items to avoid spam
                    for (i, item) in arr.iter().take(3).enumerate() {
                        lines.extend(format_value(&format!("[{}]", i), item, indent + 1));
                    }
                    if arr.len() > 3 {
                        lines.push(format!("{}  ... {} more items", prefix, arr.len() - 3));
                    }
                }
                serde_json::Value::String(s) => {
                    // Truncate long strings
                    if s.len() > 100 {
                        lines.push(format!(
                            "{}{}: \"{}...\" ({} chars)",
                            prefix,
                            key,
                            &s[..97],
                            s.len()
                        ));
                    } else {
                        lines.push(format!("{}{}: \"{}\"", prefix, key, s));
                    }
                }
                serde_json::Value::Number(n) => {
                    lines.push(format!("{}{}: {}", prefix, key, n));
                }
                serde_json::Value::Bool(b) => {
                    lines.push(format!("{}{}: {}", prefix, key, b));
                }
                serde_json::Value::Null => {
                    lines.push(format!("{}{}: null", prefix, key));
                }
            }
            lines
        }

        if let serde_json::Value::Object(map) = &json_value {
            for (key, value) in map {
                output.push_str(&format_value(key, value, 1).join("\n"));
                output.push('\n');
            }
        }

        output
    }

    /// Log the template context as key-value pairs at debug level.
    ///
    /// # Arguments
    ///
    /// * `context` - The Tera context to log
    fn log_context_as_kv(context: &TeraContext) {
        let formatted = Self::format_context_as_string(context);
        for line in formatted.lines() {
            tracing::debug!("{}", line);
        }
    }

    /// Check if content contains Tera template syntax.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the content contains template delimiters.
    fn contains_template_syntax(&self, content: &str) -> bool {
        let has_vars = content.contains("{{");
        let has_tags = content.contains("{%");
        let has_comments = content.contains("{#");
        let result = has_vars || has_tags || has_comments;
        tracing::debug!(
            "Template syntax check: vars={}, tags={}, comments={}, result={}",
            has_vars,
            has_tags,
            has_comments,
            result
        );
        result
    }

    /// Check if content contains template syntax outside of code fences.
    ///
    /// This is used after rendering to determine if another pass is needed.
    /// It ignores template syntax inside code fences to prevent re-rendering
    /// content that has already been processed (like embedded dependency content).
    fn contains_template_syntax_outside_fences(&self, content: &str) -> bool {
        let mut in_code_fence = false;
        let mut in_guard = 0usize;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                in_guard = in_guard.saturating_add(1);
                continue;
            } else if trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                in_guard = in_guard.saturating_sub(1);
                continue;
            }

            if in_guard > 0 {
                continue;
            }

            // Track code fence boundaries
            if trimmed.starts_with("```") {
                in_code_fence = !in_code_fence;
                continue;
            }

            // Skip lines inside code fences
            if in_code_fence {
                continue;
            }

            // Check for template syntax in non-fenced content
            if line.contains("{{") || line.contains("{%") || line.contains("{#") {
                tracing::debug!(
                    "Template syntax found outside code fences: {:?}",
                    &line[..line.len().min(80)]
                );
                return true;
            }
        }

        tracing::debug!("No template syntax found outside code fences");
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{LockFile, LockedResource};

    fn create_test_lockfile() -> LockFile {
        let mut lockfile = LockFile::default();

        // Add a test agent
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum".to_string(),
            installed_at: ".claude/agents/test-agent.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: None,
        });

        lockfile
    }

    #[tokio::test]
    async fn test_template_context_builder() {
        let lockfile = create_test_lockfile();

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();
        let builder =
            TemplateContextBuilder::new(Arc::new(lockfile), None, Arc::new(cache), project_dir);

        let resource_id = crate::lockfile::ResourceId {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: None,
        };
        let _context = builder.build_context(&resource_id, ResourceType::Agent).await.unwrap();

        // If we got here without panicking, context building succeeded
        // The actual context structure is tested implicitly by the renderer tests
    }

    #[test]
    fn test_template_renderer() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Test rendering without template syntax
        let result = renderer.render_template("# Plain Markdown", &TeraContext::new()).unwrap();
        assert_eq!(result, "# Plain Markdown");

        // Test rendering with template syntax
        let mut context = TeraContext::new();
        context.insert("test_var", "test_value");

        let result = renderer.render_template("# {{ test_var }}", &context).unwrap();
        assert_eq!(result, "# test_value");
    }

    #[test]
    fn test_template_renderer_disabled() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(false, project_dir, None).unwrap();

        let mut context = TeraContext::new();
        context.insert("test_var", "test_value");

        // Should return content as-is when disabled
        let result = renderer.render_template("# {{ test_var }}", &context).unwrap();
        assert_eq!(result, "# {{ test_var }}");
    }

    #[test]
    fn test_template_error_formatting() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();
        let context = TeraContext::new();

        // Test with missing variable - should produce detailed error
        let result = renderer.render_template("# {{ missing_var }}", &context);
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_msg = format!("{}", error);

        // Error should NOT contain "__tera_one_off"
        assert!(
            !error_msg.contains("__tera_one_off"),
            "Error should not expose internal Tera template names"
        );

        // Error should contain useful information about the missing variable
        assert!(
            error_msg.contains("missing_var") || error_msg.contains("Variable"),
            "Error should mention the problematic variable or that a variable is missing. Got: {}",
            error_msg
        );
    }

    #[test]
    fn test_to_native_path_display() {
        // Test Unix-style path conversion
        let unix_path = ".claude/agents/test.md";
        let native_path = to_native_path_display(unix_path);

        #[cfg(windows)]
        {
            assert_eq!(native_path, ".claude\\agents\\test.md");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(native_path, ".claude/agents/test.md");
        }
    }

    #[test]
    fn test_to_native_path_display_nested() {
        // Test deeply nested path
        let unix_path = ".claude/agents/ai/helpers/test.md";
        let native_path = to_native_path_display(unix_path);

        #[cfg(windows)]
        {
            assert_eq!(native_path, ".claude\\agents\\ai\\helpers\\test.md");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(native_path, ".claude/agents/ai/helpers/test.md");
        }
    }

    #[tokio::test]
    async fn test_template_context_uses_native_paths() {
        let mut lockfile = create_test_lockfile();

        // Add another resource with a nested path
        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "snippets/utils/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum".to_string(),
            installed_at: ".agpm/snippets/utils/test.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Snippet,
            tool: Some("agpm".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: None,
        });

        // Add the snippet as a dependency of the test-agent
        if let Some(agent) = lockfile.agents.first_mut() {
            agent.dependencies.push("snippet/test-snippet".to_string());
        }

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();
        let builder =
            TemplateContextBuilder::new(Arc::new(lockfile), None, Arc::new(cache), project_dir);
        let resource_id = crate::lockfile::ResourceId {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: None,
        };
        let context = builder.build_context(&resource_id, ResourceType::Agent).await.unwrap();

        // Extract the agpm.resource.install_path from context
        let agpm_value = context.get("agpm").expect("agpm context should exist");
        let agpm_obj = agpm_value.as_object().expect("agpm should be an object");
        let resource_value = agpm_obj.get("resource").expect("resource should exist");
        let resource_obj = resource_value.as_object().expect("resource should be an object");
        let install_path = resource_obj
            .get("install_path")
            .expect("install_path should exist")
            .as_str()
            .expect("install_path should be a string");

        // Verify the path uses platform-native separators
        #[cfg(windows)]
        {
            assert_eq!(install_path, ".claude\\agents\\test-agent.md");
            assert!(install_path.contains('\\'), "Windows paths should use backslashes");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(install_path, ".claude/agents/test-agent.md");
            assert!(install_path.contains('/'), "Unix paths should use forward slashes");
        }

        // Also verify dependency paths
        let deps_value = agpm_obj.get("deps").expect("deps should exist");
        let deps_obj = deps_value.as_object().expect("deps should be an object");
        let snippets = deps_obj.get("snippets").expect("snippets should exist");
        let snippets_obj = snippets.as_object().expect("snippets should be an object");
        let test_snippet = snippets_obj.get("test_snippet").expect("test_snippet should exist");
        let snippet_obj = test_snippet.as_object().expect("test_snippet should be an object");
        let snippet_path = snippet_obj
            .get("install_path")
            .expect("install_path should exist")
            .as_str()
            .expect("install_path should be a string");

        #[cfg(windows)]
        {
            assert_eq!(snippet_path, ".agpm\\snippets\\utils\\test.md");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(snippet_path, ".agpm/snippets/utils/test.md");
        }
    }

    // Tests for literal block functionality (Phase 1)

    #[test]
    fn test_protect_literal_blocks_basic() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let content = r#"# Documentation

Use this syntax:

```literal
{{ agpm.deps.snippets.example.content }}
```

That's how you embed content."#;

        let (protected, placeholders) = renderer.protect_literal_blocks(content);

        // Should have one placeholder
        assert_eq!(placeholders.len(), 1);

        // Protected content should contain placeholder
        assert!(protected.contains("__AGPM_LITERAL_BLOCK_0__"));

        // Protected content should NOT contain the template syntax
        assert!(!protected.contains("{{ agpm.deps.snippets.example.content }}"));

        // Placeholder should contain the original content
        let placeholder_content = placeholders.get("__AGPM_LITERAL_BLOCK_0__").unwrap();
        assert!(placeholder_content.contains("{{ agpm.deps.snippets.example.content }}"));
    }

    #[test]
    fn test_protect_literal_blocks_multiple() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let content = r#"# First Example

```literal
{{ first.example }}
```

# Second Example

```literal
{{ second.example }}
```"#;

        let (protected, placeholders) = renderer.protect_literal_blocks(content);

        // Should have two placeholders
        assert_eq!(placeholders.len(), 2);

        // Both placeholders should be in the protected content
        assert!(protected.contains("__AGPM_LITERAL_BLOCK_0__"));
        assert!(protected.contains("__AGPM_LITERAL_BLOCK_1__"));

        // Original template syntax should not be in protected content
        assert!(!protected.contains("{{ first.example }}"));
        assert!(!protected.contains("{{ second.example }}"));
    }

    #[test]
    fn test_restore_literal_blocks() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let mut placeholders = HashMap::new();
        placeholders.insert(
            "__AGPM_LITERAL_BLOCK_0__".to_string(),
            "{{ agpm.deps.snippets.example.content }}".to_string(),
        );

        let content = "# Example\n\n__AGPM_LITERAL_BLOCK_0__\n\nDone.";
        let restored = renderer.restore_literal_blocks(content, placeholders);

        // Should contain the original content in a code fence
        assert!(restored.contains("```\n{{ agpm.deps.snippets.example.content }}\n```"));

        // Should NOT contain the placeholder
        assert!(!restored.contains("__AGPM_LITERAL_BLOCK_0__"));
    }

    #[test]
    fn test_literal_blocks_integration_with_rendering() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Agent: {{ agent_name }}

## Documentation

Here's how to use template syntax:

```literal
{{ agpm.deps.snippets.helper.content }}
```

The agent name is: {{ agent_name }}"#;

        let mut context = TeraContext::new();
        context.insert("agent_name", "test-agent");

        let result = renderer.render_template(template, &context).unwrap();

        // The agent_name variable should be rendered
        assert!(result.contains("# Agent: test-agent"));
        assert!(result.contains("The agent name is: test-agent"));

        // The literal block should be preserved and wrapped in code fence
        assert!(result.contains("```\n{{ agpm.deps.snippets.helper.content }}\n```"));

        // The literal block should NOT be rendered (still has template syntax)
        assert!(result.contains("{{ agpm.deps.snippets.helper.content }}"));
    }

    #[test]
    fn test_literal_blocks_with_complex_template_syntax() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Documentation

```literal
{% for item in agpm.deps.agents %}
{{ item.name }}: {{ item.version }}
{% endfor %}
```"#;

        let context = TeraContext::new();
        let result = renderer.render_template(template, &context).unwrap();

        // Should preserve the for loop syntax
        assert!(result.contains("{% for item in agpm.deps.agents %}"));
        assert!(result.contains("{{ item.name }}"));
        assert!(result.contains("{% endfor %}"));
    }

    #[test]
    fn test_literal_blocks_empty() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Example

```literal
```

Done."#;

        let context = TeraContext::new();
        let result = renderer.render_template(template, &context).unwrap();

        // Should handle empty literal blocks gracefully
        assert!(result.contains("# Example"));
        assert!(result.contains("Done."));
    }

    #[test]
    fn test_literal_blocks_unclosed() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let content = r#"# Example

```literal
{{ template.syntax }}
This block is not closed"#;

        let (protected, placeholders) = renderer.protect_literal_blocks(content);

        // Should have no placeholders (unclosed block is treated as regular content)
        assert_eq!(placeholders.len(), 0);

        // Content should be preserved as-is
        assert!(protected.contains("```literal"));
        assert!(protected.contains("{{ template.syntax }}"));
    }

    #[test]
    fn test_literal_blocks_with_indentation() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let content = r#"# Example

    ```literal
    {{ indented.template }}
    ```"#;

        let (_protected, placeholders) = renderer.protect_literal_blocks(content);

        // Should detect indented literal blocks
        assert_eq!(placeholders.len(), 1);

        // Should preserve the indented template syntax
        let placeholder_content = placeholders.get("__AGPM_LITERAL_BLOCK_0__").unwrap();
        assert!(placeholder_content.contains("{{ indented.template }}"));
    }

    #[test]
    fn test_literal_blocks_in_transitive_dependency_content() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();

        // Create a dependency file with literal blocks containing template syntax
        let dep_content = r#"---
agpm.templating: true
---
# Dependency Documentation

Here's a template example:

```literal
{{ nonexistent_variable }}
{{ agpm.deps.something.else }}
```

This should appear literally."#;

        // Write the dependency file
        let dep_path = project_dir.join("dependency.md");
        fs::write(&dep_path, dep_content).unwrap();

        // First, render the dependency content (simulating what happens when processing a dependency)
        let mut dep_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let dep_context = TeraContext::new();
        let rendered_dep = dep_renderer.render_template(dep_content, &dep_context).unwrap();

        // The rendered dependency should have the literal block converted to a regular code fence
        assert!(rendered_dep.contains("```\n{{ nonexistent_variable }}"));
        assert!(rendered_dep.contains("{{ agpm.deps.something.else }}\n```"));

        // Now simulate embedding this in a parent resource
        let parent_template = r#"# Parent Resource

## Embedded Documentation

{{ dependency_content }}

## End"#;

        // Create context with the rendered dependency content
        let mut parent_context = TeraContext::new();
        parent_context.insert("dependency_content", &rendered_dep);

        // Render the parent (with templating enabled)
        let mut parent_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let final_output =
            parent_renderer.render_template(parent_template, &parent_context).unwrap();

        // Verify the final output contains the template syntax literally
        assert!(
            final_output.contains("{{ nonexistent_variable }}"),
            "Template syntax from literal block should appear literally in final output"
        );
        assert!(
            final_output.contains("{{ agpm.deps.something.else }}"),
            "Template syntax from literal block should appear literally in final output"
        );

        // Verify it's in a code fence
        assert!(
            final_output.contains("```\n{{ nonexistent_variable }}"),
            "Literal content should be in a code fence"
        );

        // Verify it doesn't cause rendering errors
        assert!(!final_output.contains("__AGPM_LITERAL_BLOCK_"), "No placeholders should remain");
    }

    #[test]
    fn test_literal_blocks_with_nested_dependencies() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Simulate a dependency that was already rendered with literal blocks
        let dep_content = r#"# Helper Snippet

Use this syntax:

```
{{ agpm.deps.snippets.example.content }}
{{ missing.variable }}
```

Done."#;

        // Now embed this in a parent template
        let parent_template = r#"# Main Agent

## Documentation

{{ helper_content }}

The agent uses templating."#;

        let mut context = TeraContext::new();
        context.insert("helper_content", dep_content);

        let result = renderer.render_template(parent_template, &context).unwrap();

        // The template syntax from the dependency should be preserved
        assert!(result.contains("{{ agpm.deps.snippets.example.content }}"));
        assert!(result.contains("{{ missing.variable }}"));

        // It should be in a code fence
        assert!(result.contains("```\n{{ agpm.deps.snippets.example.content }}"));
    }

    #[tokio::test]
    async fn test_non_templated_dependency_content_is_guarded() {
        use tempfile::TempDir;
        use tokio::fs;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();

        let snippets_dir = project_dir.join("snippets");
        fs::create_dir_all(&snippets_dir).await.unwrap();
        let snippet_path = snippets_dir.join("non-templated.md");
        fs::write(
            &snippet_path,
            r#"---
agpm:
  templating: false
---
# Example Snippet

This should show {{ agpm.deps.some.content }} literally.
"#,
        )
        .await
        .unwrap();

        let mut lockfile = LockFile::default();
        lockfile.commands.push(LockedResource {
            name: "test-command".to_string(),
            source: None,
            url: None,
            path: "commands/test.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test-command".to_string(),
            installed_at: ".claude/commands/test.md".to_string(),
            dependencies: vec!["snippet/non_templated".to_string()],
            resource_type: ResourceType::Command,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: None,
        });
        lockfile.snippets.push(LockedResource {
            name: "non_templated".to_string(),
            source: None,
            url: None,
            path: "snippets/non-templated.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:test-snippet".to_string(),
            installed_at: ".agpm/snippets/non-templated.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Snippet,
            tool: Some("agpm".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: None,
        });

        let cache = crate::cache::Cache::new().unwrap();
        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            None,
            Arc::new(cache),
            project_dir.clone(),
        );
        let resource_id = crate::lockfile::ResourceId {
            name: "test-command".to_string(),
            source: None,
            tool: Some("claude-code".to_string()),
            template_vars: None,
        };
        let context = builder.build_context(&resource_id, ResourceType::Command).await.unwrap();

        let mut renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let template = r#"# Combined Output

{{ agpm.deps.snippets.non_templated.content }}
"#;
        let rendered = renderer.render_template(template, &context).unwrap();

        assert!(
            rendered.contains("# Example Snippet"),
            "Rendered output should include the snippet heading"
        );
        assert!(
            rendered.contains("{{ agpm.deps.some.content }}"),
            "Template syntax inside non-templated dependency should remain literal"
        );
        assert!(
            !rendered.contains(NON_TEMPLATED_LITERAL_GUARD_START)
                && !rendered.contains(NON_TEMPLATED_LITERAL_GUARD_END),
            "Internal literal guard markers should not leak into rendered output"
        );
        assert!(
            !rendered.contains("```literal"),
            "Synthetic literal fences should be removed after rendering"
        );
    }

    #[tokio::test]
    async fn test_template_vars_override() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Add a second agent with template_vars to test overrides
        let template_vars = json!({
            "project": {
                "language": "python",
                "framework": "fastapi"
            },
            "custom": {
                "style": "functional"
            }
        });

        lockfile.agents.push(LockedResource {
            name: "test-agent-python".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum2".to_string(),
            installed_at: ".claude/agents/test-agent-python.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: Some(template_vars.clone()),
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            map.insert("framework".to_string(), toml::Value::String("tokio".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        // Build context without template_vars
        let resource_id_no_override = crate::lockfile::ResourceId {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: None,
        };
        let context_without_override =
            builder.build_context(&resource_id_no_override, ResourceType::Agent).await.unwrap();

        // Build context WITH template_vars (different lockfile entry)
        let resource_id_with_override = crate::lockfile::ResourceId {
            name: "test-agent-python".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: Some(template_vars.clone()),
        };
        let context_with_override = builder
            .build_context(&resource_id_with_override, ResourceType::Agent)
            .await
            .unwrap();

        // Test without overrides - should use project defaults
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = "Language: {{ project.language }}, Framework: {{ project.framework }}";

        let rendered_without =
            renderer.render_template(template, &context_without_override).unwrap();
        assert_eq!(rendered_without, "Language: rust, Framework: tokio");

        // Test with overrides - should use overridden values
        let rendered_with = renderer.render_template(template, &context_with_override).unwrap();
        assert_eq!(rendered_with, "Language: python, Framework: fastapi");

        // Test new namespace from overrides
        let custom_template = "Style: {{ custom.style }}";
        let rendered_custom =
            renderer.render_template(custom_template, &context_with_override).unwrap();
        assert_eq!(rendered_custom, "Style: functional");
    }

    #[tokio::test]
    async fn test_template_vars_deep_merge() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Override only some database fields, leaving others unchanged
        let template_vars = json!({
            "project": {
                "database": {
                    "host": "db.example.com",
                    "ssl": true
                }
            }
        });

        // Add a second agent with template_vars for deep merge testing
        lockfile.agents.push(LockedResource {
            name: "test-agent-merged".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum3".to_string(),
            installed_at: ".claude/agents/test-agent-merged.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: Some(template_vars.clone()),
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with nested project config
        let project_config = {
            let mut map = toml::map::Map::new();
            let mut db_table = toml::map::Map::new();
            db_table.insert("type".to_string(), toml::Value::String("postgres".into()));
            db_table.insert("host".to_string(), toml::Value::String("localhost".into()));
            db_table.insert("port".to_string(), toml::Value::Integer(5432));
            map.insert("database".to_string(), toml::Value::Table(db_table));
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        let resource_id = crate::lockfile::ResourceId {
            name: "test-agent-merged".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: Some(template_vars.clone()),
        };
        let context = builder
            .build_context(&resource_id, ResourceType::Agent)
            .await
            .unwrap();

        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Test that merge kept original values and added new ones
        let template = r#"
DB Type: {{ project.database.type }}
DB Host: {{ project.database.host }}
DB Port: {{ project.database.port }}
DB SSL: {{ project.database.ssl }}
Language: {{ project.language }}
"#;

        let rendered = renderer.render_template(template, &context).unwrap();

        // Original values should be preserved
        assert!(rendered.contains("DB Type: postgres"));
        assert!(rendered.contains("DB Port: 5432"));
        assert!(rendered.contains("Language: rust"));

        // Overridden value should be used
        assert!(rendered.contains("DB Host: db.example.com"));

        // New value should be added
        assert!(rendered.contains("DB SSL: true"));
    }

    #[tokio::test]
    async fn test_template_vars_empty_object_noop() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Empty object should be a no-op
        let template_vars = json!({});

        lockfile.agents.push(LockedResource {
            name: "test-agent-empty".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:empty".to_string(),
            installed_at: ".claude/agents/test-agent-empty.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: Some(template_vars.clone()),
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            map.insert("version".to_string(), toml::Value::String("1.0".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        let resource_id = crate::lockfile::ResourceId {
            name: "test-agent-empty".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: Some(template_vars.clone()),
        };

        let context = builder
            .build_context(&resource_id, ResourceType::Agent)
            .await
            .unwrap();

        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Empty template_vars should not change project config
        let template = "Language: {{ project.language }}, Version: {{ project.version }}";
        let rendered = renderer.render_template(template, &context).unwrap();
        assert_eq!(rendered, "Language: rust, Version: 1.0");
    }

    #[tokio::test]
    async fn test_template_vars_null_values() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Null value should replace field with JSON null
        let template_vars = json!({
            "project": {
                "optional_field": null
            }
        });

        lockfile.agents.push(LockedResource {
            name: "test-agent-null".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:null".to_string(),
            installed_at: ".claude/agents/test-agent-null.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: Some(template_vars.clone()),
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        let resource_id = crate::lockfile::ResourceId {
            name: "test-agent-null".to_string(),
            source: Some("community".to_string()),
            tool: Some("claude-code".to_string()),
            template_vars: Some(template_vars.clone()),
        };

        let context = builder
            .build_context(&resource_id, ResourceType::Agent)
            .await
            .unwrap();

        // Verify null is present in context
        let agpm_value = context.get("agpm").expect("agpm should exist");
        let agpm_obj = agpm_value.as_object().expect("agpm should be an object");

        // Check both agpm.project and project namespaces
        let project_value = agpm_obj.get("project").expect("project should exist");
        let project_obj = project_value.as_object().expect("project should be an object");
        assert!(project_obj.get("optional_field").is_some());
        assert!(project_obj["optional_field"].is_null());

        // Also verify in top-level project namespace
        let top_project = context.get("project").expect("top-level project should exist");
        let top_project_obj = top_project.as_object().expect("should be object");
        assert!(top_project_obj.get("optional_field").is_some());
        assert!(top_project_obj["optional_field"].is_null());
    }
}
