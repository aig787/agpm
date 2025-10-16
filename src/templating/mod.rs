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
//!
//! # Template Context
//!
//! Templates are rendered with a structured context containing:
//! - `agpm.resource`: Current resource information (name, type, install path, etc.)
//! - `agpm.deps`: Nested dependency information by resource type and name
//!
//! # Syntax Restrictions
//!
//! For security and safety, the following Tera features are disabled:
//! - `{% include %}` tags (no file system access)
//! - `{% extends %}` tags (no template inheritance)
//! - `{% import %}` tags (no external template imports)
//! - Custom functions that access the file system or network
//!
//! # Supported Features
//!
//! - Variable substitution: `{{ agpm.resource.install_path }}`
//! - Conditional logic: `{% if agpm.resource.source %}...{% endif %}`
//! - Loops: `{% for name, dep in agpm.deps.agents %}...{% endfor %}`
//! - Standard Tera filters (string manipulation, formatting)
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
//! ## Dependency References
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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, to_string, to_value};
use std::collections::HashMap;
use std::sync::Arc;
use tera::{Context as TeraContext, Tera};

use crate::core::ResourceType;
use crate::lockfile::LockFile;

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
/// - No file system access via includes/extends
/// - No network access
/// - Sandboxed template execution
/// - Custom functions are carefully vetted
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl TemplateContextBuilder {
    /// Create a new template context builder.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The resolved lockfile, wrapped in Arc for efficient sharing
    pub fn new(lockfile: Arc<LockFile>) -> Self {
        Self {
            lockfile,
        }
    }

    /// Build the complete template context for a specific resource.
    ///
    /// # Arguments
    ///
    /// * `resource_name` - Name of the resource being rendered
    /// * `resource_type` - Type of the resource (agents, snippets, etc.)
    ///
    /// # Returns
    ///
    /// Returns a Tera `Context` containing all available template variables.
    pub fn build_context(
        &self,
        resource_name: &str,
        resource_type: ResourceType,
    ) -> Result<TeraContext> {
        let mut context = TeraContext::new();

        // Build the nested agpm structure
        let mut agpm = Map::new();

        // Build current resource data
        let resource_data = self.build_resource_data(resource_name, resource_type)?;
        agpm.insert("resource".to_string(), to_value(resource_data)?);

        // Build dependency data
        let deps_data = self.build_dependencies_data()?;
        agpm.insert("deps".to_string(), to_value(deps_data)?);

        // Insert the complete agpm object
        context.insert("agpm", &agpm);

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
        })
    }

    /// Build dependency data for the template context.
    ///
    /// This creates a nested structure of all dependencies by resource type and name.
    fn build_dependencies_data(
        &self,
    ) -> Result<HashMap<String, HashMap<String, ResourceTemplateData>>> {
        let mut deps = HashMap::new();

        // Process each resource type
        for resource_type in [
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::Script,
            ResourceType::Hook,
            ResourceType::McpServer,
        ] {
            let type_str_plural = resource_type.to_plural().to_string();
            let type_str_singular = resource_type.to_string();
            let mut type_deps = HashMap::new();

            let resources = self.lockfile.get_resources_by_type(resource_type);
            for resource in resources {
                let template_data = ResourceTemplateData {
                    resource_type: type_str_singular.clone(),
                    name: resource.name.clone(),
                    install_path: to_native_path_display(&resource.installed_at),
                    source: resource.source.clone(),
                    version: resource.version.clone(),
                    resolved_commit: resource.resolved_commit.clone(),
                    checksum: resource.checksum.clone(),
                    path: resource.path.clone(),
                };
                // Sanitize the key name by replacing hyphens with underscores
                // to avoid Tera interpreting them as minus operators
                let sanitized_key = resource.name.replace('-', "_");
                type_deps.insert(sanitized_key, template_data);
            }

            if !type_deps.is_empty() {
                deps.insert(type_str_plural, type_deps);
            }
        }

        // Debug: Print what we're building
        tracing::debug!("Built dependencies data with {} resource types", deps.len());
        for (resource_type, resources) in &deps {
            tracing::debug!("  Type {}: {} resources", resource_type, resources.len());
            for name in resources.keys() {
                tracing::debug!("    - {}", name);
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
    /// use std::path::Path;
    /// use std::sync::Arc;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
    /// let builder = TemplateContextBuilder::new(Arc::new(lockfile));
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
    ///
    /// # Returns
    ///
    /// Returns a configured `TemplateRenderer` instance.
    pub fn new(enabled: bool) -> Result<Self> {
        let tera = Tera::default();

        Ok(Self {
            tera,
            enabled,
        })
    }

    /// Render a Markdown template with the given context.
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

        // Check if content contains template syntax
        if !self.contains_template_syntax(template_content) {
            // No template syntax found, return as-is
            tracing::debug!("No template syntax found, returning content as-is");
            return Ok(template_content.to_string());
        }

        // Log the template context for debugging
        tracing::debug!("Rendering template with context");
        Self::log_context_as_kv(context, "debug");

        // Render the template
        self.tera
            .render_str(template_content, context)
            .map_err(|e| {
                // Use Display format for more user-friendly error messages
                // The Tera error already contains detailed information about:
                // - Missing variables (e.g., "Variable `foo` not found")
                // - Syntax errors (e.g., "Unexpected end of template")
                // - Filter/function errors (e.g., "Filter `unknown` not found")
                // Preserve this information in the error chain
                tracing::error!("Template rendering failed. Context was:");
                Self::log_context_as_kv(context, "error");
                anyhow::Error::new(e)
                    .context("Template rendering failed - check syntax and variable names")
            })
    }

    /// Log the template context as key-value pairs for better readability.
    ///
    /// # Arguments
    ///
    /// * `context` - The Tera context to log
    /// * `level` - The log level to use ("debug" or "error")
    fn log_context_as_kv(context: &TeraContext, level: &str) {
        // Clone context and convert to JSON for iteration
        let context_clone = context.clone();
        let json_value = context_clone.into_json();

        // Helper to log at the appropriate level
        let log_fn = |msg: String| match level {
            "error" => tracing::error!("{}", msg),
            _ => tracing::debug!("{}", msg),
        };

        // Recursively log the JSON structure with indentation
        fn log_value(key: &str, value: &serde_json::Value, indent: usize, log_fn: &dyn Fn(String)) {
            let prefix = "  ".repeat(indent);
            match value {
                serde_json::Value::Object(map) => {
                    log_fn(format!("{}{}:", prefix, key));
                    for (k, v) in map {
                        log_value(k, v, indent + 1, log_fn);
                    }
                }
                serde_json::Value::Array(arr) => {
                    log_fn(format!("{}{}: [{} items]", prefix, key, arr.len()));
                    // Only show first few items to avoid spam
                    for (i, item) in arr.iter().take(3).enumerate() {
                        log_value(&format!("[{}]", i), item, indent + 1, log_fn);
                    }
                    if arr.len() > 3 {
                        log_fn(format!("{}  ... {} more items", prefix, arr.len() - 3));
                    }
                }
                serde_json::Value::String(s) => {
                    // Truncate long strings
                    if s.len() > 100 {
                        log_fn(format!("{}{}: \"{}...\" ({} chars)", prefix, key, &s[..97], s.len()));
                    } else {
                        log_fn(format!("{}{}: \"{}\"", prefix, key, s));
                    }
                }
                serde_json::Value::Number(n) => {
                    log_fn(format!("{}{}: {}", prefix, key, n));
                }
                serde_json::Value::Bool(b) => {
                    log_fn(format!("{}{}: {}", prefix, key, b));
                }
                serde_json::Value::Null => {
                    log_fn(format!("{}{}: null", prefix, key));
                }
            }
        }

        if let serde_json::Value::Object(map) = &json_value {
            for (key, value) in map {
                log_value(key, value, 1, &log_fn);
            }
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
        });

        lockfile
    }

    #[test]
    fn test_template_context_builder() {
        let lockfile = create_test_lockfile();

        let builder = TemplateContextBuilder::new(Arc::new(lockfile));

        let _context = builder.build_context("test-agent", ResourceType::Agent).unwrap();

        // If we got here without panicking, context building succeeded
        // The actual context structure is tested implicitly by the renderer tests
    }

    #[test]
    fn test_template_renderer() {
        let mut renderer = TemplateRenderer::new(true).unwrap();

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
        let mut renderer = TemplateRenderer::new(false).unwrap();

        let mut context = TeraContext::new();
        context.insert("test_var", "test_value");

        // Should return content as-is when disabled
        let result = renderer.render_template("# {{ test_var }}", &context).unwrap();
        assert_eq!(result, "# {{ test_var }}");
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

    #[test]
    fn test_template_context_uses_native_paths() {
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
        });

        let builder = TemplateContextBuilder::new(Arc::new(lockfile));
        let context = builder.build_context("test-agent", ResourceType::Agent).unwrap();

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
}
