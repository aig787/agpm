//! Resource dependency types and implementations.
//!
//! This module provides the core dependency specification types used in AGPM manifests:
//! - `ResourceDependency`: Enum supporting both simple path-only and detailed specifications
//! - `DetailedDependency`: Full dependency specification with all configuration options

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::manifest::dependency_spec::DependencySpec;

/// A resource dependency specification supporting multiple formats.
///
/// Dependencies can be specified in two main formats to balance simplicity
/// with flexibility. The enum uses Serde's `untagged` attribute to automatically
/// deserialize the correct variant based on the TOML structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceDependency {
    /// Simple path-only dependency, typically for local files.
    ///
    /// This variant represents the simplest dependency format where only
    /// a file path is specified. It's primarily used for local dependencies
    /// that exist in the filesystem relative to the project.
    ///
    /// # Format
    ///
    /// ```toml
    /// dependency-name = "path/to/file.md"
    /// ```
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Relative paths from manifest directory
    /// helper = "../shared/helper.md"
    /// custom = "./local/custom.md"
    ///
    /// # Absolute paths (not recommended)
    /// system = "/usr/local/share/agent.md"
    /// ```
    ///
    /// # Limitations
    ///
    /// - Cannot specify version constraints
    /// - Cannot reference remote Git sources
    /// - Must be a valid filesystem path
    /// - Path must exist at installation time
    Simple(String),

    /// Detailed dependency specification with full control.
    ///
    /// This variant provides complete control over dependency specification,
    /// supporting both local and remote dependencies with version constraints,
    /// Git references, and explicit source mapping.
    ///
    /// See [`DetailedDependency`] for field-level documentation.
    ///
    /// Note: This variant is boxed to reduce the overall size of the enum.
    Detailed(Box<DetailedDependency>),
}

/// Detailed dependency specification with full control over source resolution.
///
/// This struct provides fine-grained control over dependency specification,
/// supporting both local filesystem paths and remote Git repository resources
/// with flexible version constraints and Git reference handling.
///
/// # Field Relationships
///
/// The fields work together with specific validation rules:
/// - If `source` is specified: Must have either `version` or `git`
/// - If `source` is omitted: Dependency is local, `version` and `git` are ignored
/// - `path` is always required and cannot be empty
///
/// # Examples
///
/// ## Remote Dependencies
///
/// ```toml
/// [agents]
/// # Semantic version constraint
/// stable = { source = "official", path = "agents/stable.md", version = "v1.0.0" }
///
/// # Latest version (not recommended for production)
/// latest = { source = "community", path = "agents/utils.md", version = "latest" }
///
/// # Specific Git branch
/// cutting-edge = { source = "official", path = "agents/new.md", git = "develop" }
///
/// # Specific commit SHA (maximum reproducibility)
/// pinned = { source = "community", path = "agents/tool.md", git = "a1b2c3d4e5f6..." }
///
/// # Git tag
/// release = { source = "official", path = "agents/release.md", git = "v2.0-release" }
/// ```
///
/// ## Local Dependencies
///
/// ```toml
/// [agents]
/// # Local file (version/git fields ignored if present)
/// local-helper = { path = "../shared/helper.md" }
/// custom = { path = "./local/custom.md" }
/// ```
///
/// # Version Resolution Priority
///
/// When both `version` and `git` are specified, `git` takes precedence:
///
/// ```toml
/// # This will use the "develop" branch, not "v1.0.0"
/// conflicted = { source = "repo", path = "file.md", version = "v1.0.0", git = "develop" }
/// ```
///
/// # Path Format
///
/// Paths are interpreted differently based on context:
/// - **Remote dependencies**: Path within the Git repository
/// - **Local dependencies**: Filesystem path relative to manifest directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    /// Source repository name referencing the `[sources]` section.
    ///
    /// When specified, this dependency will be resolved from a Git repository.
    /// The name must exactly match a key in the manifest's `[sources]` table.
    ///
    /// **Omit this field** to create a local filesystem dependency.
    ///
    /// # Examples
    ///
    /// ```toml
    /// # References this source definition:
    /// [sources]
    /// official = "https://github.com/org/repo.git"
    ///
    /// [agents]
    /// remote-agent = { source = "official", path = "agents/tool.md", version = "v1.0.0" }
    /// local-agent = { path = "../local/tool.md" }  # No source = local dependency
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Path to the resource file or glob pattern for multiple resources.
    ///
    /// For **remote dependencies**: Path within the Git repository\
    /// For **local dependencies**: Filesystem path relative to manifest directory\
    /// For **pattern dependencies**: Glob pattern to match multiple resources
    ///
    /// This field supports both individual file paths and glob patterns:
    /// - Individual file: `"agents/helper.md"`
    /// - Pattern matching: `"agents/*.md"`, `"**/*.md"`, `"agents/[a-z]*.md"`
    ///
    /// Pattern dependencies are detected by the presence of glob characters
    /// (`*`, `?`, `[`) in the path. When a pattern is detected, AGPM will
    /// expand it to match all resources in the source repository.
    ///
    /// # Examples
    ///
    /// ```toml
    /// # Remote: single file in git repo
    /// remote = { source = "repo", path = "agents/helper.md", version = "v1.0.0" }
    ///
    /// # Local: filesystem path
    /// local = { path = "../shared/helper.md" }
    ///
    /// # Pattern: all agents in AI folder
    /// ai_agents = { source = "repo", path = "agents/ai/*.md", version = "v1.0.0" }
    ///
    /// # Pattern: all agents recursively
    /// all_agents = { source = "repo", path = "agents/**/*.md", version = "v1.0.0" }
    /// ```
    pub path: String,

    /// Version constraint for Git tag resolution.
    ///
    /// Specifies which version of the resource to use when resolving from
    /// a Git repository. This field is ignored for local dependencies.
    ///
    /// **Note**: If both `version` and `git` are specified, `git` takes precedence.
    ///
    /// # Supported Formats
    ///
    /// - `"v1.0.0"` - Exact semantic version tag
    /// - `"1.0.0"` - Exact version (v prefix optional)
    /// - `"^1.0.0"` - Semantic version constraint (highest compatible 1.x.x)
    /// - `"latest"` - Git tag or branch named "latest" (not special - just a name)
    /// - `"main"` - Use main/master branch HEAD
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// stable = { source = "repo", path = "agent.md", version = "v1.0.0" }
    /// flexible = { source = "repo", path = "agent.md", version = "^1.0.0" }
    /// latest-tag = { source = "repo", path = "agent.md", version = "latest" }  # If repo has a "latest" tag
    /// main = { source = "repo", path = "agent.md", version = "main" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Git branch to track.
    ///
    /// Specifies a Git branch to use when resolving the dependency.
    /// Branch references are mutable and will update to the latest commit on each update.
    /// This field is ignored for local dependencies.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Track the main branch
    /// dev = { source = "repo", path = "agent.md", branch = "main" }
    ///
    /// # Track a feature branch
    /// experimental = { source = "repo", path = "agent.md", branch = "feature/new-capability" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Git commit hash (revision).
    ///
    /// Specifies an exact Git commit to use when resolving the dependency.
    /// Provides maximum reproducibility as commits are immutable.
    /// This field is ignored for local dependencies.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Pin to exact commit (full hash)
    /// pinned = { source = "repo", path = "agent.md", rev = "a1b2c3d4e5f67890abcdef1234567890abcdef12" }
    ///
    /// # Pin to exact commit (abbreviated)
    /// stable = { source = "repo", path = "agent.md", rev = "abc123def" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,

    /// Command to execute for MCP servers.
    ///
    /// This field is specific to MCP server dependencies and specifies
    /// the command that will be executed to run the MCP server.
    /// Only used for entries in the `[mcp-servers]` section.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [mcp-servers]
    /// github = { source = "repo", path = "mcp/github.toml", version = "v1.0.0", command = "npx" }
    /// sqlite = { path = "./local/sqlite.toml", command = "uvx" }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments to pass to the MCP server command.
    ///
    /// This field is specific to MCP server dependencies and provides
    /// the arguments that will be passed to the command when starting
    /// the MCP server. Only used for entries in the `[mcp-servers]` section.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [mcp-servers]
    /// github = {
    ///     source = "repo",
    ///     path = "mcp/github.toml",
    ///     version = "v1.0.0",
    ///     command = "npx",
    ///     args = ["-y", "@modelcontextprotocol/server-github"]
    /// }
    /// sqlite = {
    ///     path = "./local/sqlite.toml",
    ///     command = "uvx",
    ///     args = ["mcp-server-sqlite", "--db", "./data/local.db"]
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Custom target directory for this dependency.
    ///
    /// Overrides the default installation directory for this specific dependency.
    /// The path is relative to the `.claude` directory for consistency and security.
    /// If not specified, the dependency will be installed to the default location
    /// based on its resource type.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Install to .claude/custom/tools/ instead of default .claude/agents/
    /// special-agent = {
    ///     source = "repo",
    ///     path = "agent.md",
    ///     version = "v1.0.0",
    ///     target = "custom/tools"
    /// }
    ///
    /// # Install to .claude/integrations/ai/
    /// integration = {
    ///     source = "repo",
    ///     path = "integration.md",
    ///     version = "v2.0.0",
    ///     target = "integrations/ai"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Custom filename for this dependency.
    ///
    /// Overrides the default filename (which is based on the dependency key).
    /// The filename should include the desired file extension. If not specified,
    /// the dependency will be installed using the key name with an automatically
    /// determined extension based on the resource type.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Install as "ai-assistant.md" instead of "my-ai.md"
    /// my-ai = {
    ///     source = "repo",
    ///     path = "agent.md",
    ///     version = "v1.0.0",
    ///     filename = "ai-assistant.md"
    /// }
    ///
    /// # Install with a different extension
    /// doc-agent = {
    ///     source = "repo",
    ///     path = "documentation.md",
    ///     version = "v2.0.0",
    ///     filename = "docs-helper.txt"
    /// }
    ///
    /// [scripts]
    /// # Rename a script during installation
    /// analyzer = {
    ///     source = "repo",
    ///     path = "scripts/data-analyzer-v3.py",
    ///     version = "v1.0.0",
    ///     filename = "analyze.py"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

    /// Transitive dependencies on other resources.
    ///
    /// This field is populated from metadata extracted from the resource file itself
    /// (YAML frontmatter in .md files or JSON fields in .json files).
    /// Maps resource type to list of dependency specifications.
    ///
    /// Example:
    /// ```toml
    /// # This would be extracted from the file's frontmatter/JSON, not specified in agpm.toml
    /// # { "agents": [{"path": "agents/helper.md", "version": "v1.0.0"}] }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, Vec<DependencySpec>>>,

    /// Tool type (claude-code, opencode, agpm, or custom).
    ///
    /// Specifies which target AI coding assistant tool this resource is for. This determines
    /// where the resource is installed and how it's configured.
    ///
    /// When `None`, defaults are applied based on resource type:
    /// - Snippets default to "agpm" (shared infrastructure)
    /// - All other resources default to "claude-code"
    ///
    /// Omitted from TOML serialization when not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,

    /// Control directory structure preservation during installation.
    ///
    /// When `true`, only the filename is used for installation (e.g., `nested/dir/file.md` → `file.md`).
    /// When `false`, the full relative path is preserved (e.g., `nested/dir/file.md` → `nested/dir/file.md`).
    ///
    /// Default values by resource type (from tool configuration):
    /// - `agents`: `true` (flatten by default - no nested directories)
    /// - `commands`: `true` (flatten by default - no nested directories)
    /// - All others: `false` (preserve directory structure)
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Default behavior (flatten=true) - installs as "helper.md"
    /// agent1 = { source = "repo", path = "agents/subdir/helper.md", version = "v1.0.0" }
    ///
    /// # Preserve structure - installs as "subdir/helper.md"
    /// agent2 = { source = "repo", path = "agents/subdir/helper.md", version = "v1.0.0", flatten = false }
    ///
    /// [snippets]
    /// # Default behavior (flatten=false) - installs as "utils/helper.md"
    /// snippet1 = { source = "repo", path = "snippets/utils/helper.md", version = "v1.0.0" }
    ///
    /// # Flatten - installs as "helper.md"
    /// snippet2 = { source = "repo", path = "snippets/utils/helper.md", version = "v1.0.0", flatten = true }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatten: Option<bool>,

    /// Control whether the dependency should be installed to disk.
    ///
    /// When `false`, the dependency is resolved, fetched, and tracked in the lockfile,
    /// but the file is not written to the project directory. Instead, its content is
    /// made available in template context via `agpm.deps.<type>.<name>.content`.
    ///
    /// This is useful for snippet embedding use cases where you want to include
    /// content inline rather than as a separate file.
    ///
    /// Defaults to `true` (install the file).
    ///
    /// # Examples
    ///
    /// ```toml
    /// [snippets]
    /// # Embed content directly without creating a file
    /// best_practices = {
    ///     source = "repo",
    ///     path = "snippets/rust-best-practices.md",
    ///     version = "v1.0.0",
    ///     install = false
    /// }
    /// ```
    ///
    /// Then use in template:
    /// ```markdown
    /// {{ agpm.deps.snippets.best_practices.content }}
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<bool>,

    /// Template variable overrides for this specific resource.
    ///
    /// Allows specializing generic resources for different use cases by overriding
    /// template variables. These variables are merged with (and take precedence over)
    /// the global `[project]` configuration when rendering this resource and resolving
    /// its transitive dependencies.
    ///
    /// This enables creating multiple variants of the same resource without duplication.
    /// For example, a single `backend-engineer.md` agent can be specialized for different
    /// languages by providing different `template_vars` for each variant.
    ///
    /// The structure matches the template namespace hierarchy (e.g., `{ "project": { "language": "golang" } }`).
    ///
    /// # Examples
    ///
    /// ```toml
    /// [agents]
    /// # Generic backend engineer agent specialized for different languages
    /// backend-engineer-golang = {
    ///     source = "community",
    ///     path = "agents/backend-engineer.md",
    ///     version = "v1.0.0",
    ///     filename = "backend-engineer-golang.md",
    ///     template_vars = { project = { language = "golang" } }
    /// }
    ///
    /// backend-engineer-python = {
    ///     source = "community",
    ///     path = "agents/backend-engineer.md",
    ///     version = "v1.0.0",
    ///     filename = "backend-engineer-python.md",
    ///     template_vars = { project = { language = "python", framework = "fastapi" } }
    /// }
    /// ```
    ///
    /// The agent at `agents/backend-engineer.md` can use templates like:
    /// ```markdown
    /// # Backend Engineer for {{ agpm.project.language }}
    ///
    /// ---
    /// dependencies:
    ///   snippets:
    ///     - path: ../best-practices/{{ agpm.project.language }}-best-practices.md
    /// ---
    /// ```
    ///
    /// Each variant will resolve its transitive dependencies using its specific `template_vars`,
    /// so the golang variant resolves `golang-best-practices.md` while python resolves
    /// `python-best-practices.md`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_vars: Option<serde_json::Value>,
}

impl ResourceDependency {
    /// Get the source repository name if this is a remote dependency.
    ///
    /// Returns the source name for remote dependencies (those that reference
    /// a Git repository), or `None` for local dependencies (those that reference
    /// local filesystem paths).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - no source
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.get_source().is_none());
    ///
    /// // Remote dependency - has source
    /// let remote = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(remote.get_source(), Some("official"));
    /// assert_eq!(remote.get_source(), Some("official"));
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is commonly used to:
    /// - Determine if dependency resolution should use Git vs filesystem
    /// - Validate that referenced sources exist in the manifest
    /// - Filter dependencies by type (local vs remote)
    /// - Generate dependency graphs and reports
    #[must_use]
    pub fn get_source(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.source.as_deref(),
        }
    }

    /// Get the custom target directory for this dependency.
    ///
    /// Returns the custom target directory if specified, or `None` if the
    /// dependency should use the default installation location for its resource type.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Dependency with custom target
    /// let custom = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     target: Some("custom/tools".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(custom.get_target(), Some("custom/tools"));
    ///
    /// // Dependency without custom target
    /// let default = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(default.get_target().is_none());
    /// ```
    #[must_use]
    pub fn get_target(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.target.as_deref(),
        }
    }

    /// Get the tool for this dependency.
    ///
    /// Returns the tool string if specified, or None if not specified.
    /// When None is returned, the caller should apply resource-type-specific defaults.
    ///
    /// # Returns
    ///
    /// - `Some(tool)` if tool is explicitly specified
    /// - `None` if no tool is configured (use resource-type default)
    #[must_use]
    pub fn get_tool(&self) -> Option<&str> {
        match self {
            Self::Detailed(d) => d.tool.as_deref(),
            Self::Simple(_) => None,
        }
    }

    /// Get the custom filename for this dependency.
    ///
    /// Returns the custom filename if specified, or `None` if the
    /// dependency should use the default filename based on the dependency key.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Dependency with custom filename
    /// let custom = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     filename: Some("ai-assistant.md".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     install: None,
    ///     flatten: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(custom.get_filename(), Some("ai-assistant.md"));
    ///
    /// // Dependency without custom filename
    /// let default = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(default.get_filename().is_none());
    /// ```
    #[must_use]
    pub fn get_filename(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.filename.as_deref(),
        }
    }

    /// Get the flatten flag for this dependency.
    ///
    /// Returns the flatten setting if explicitly specified, or `None` if the
    /// dependency should use the default flatten behavior based on tool configuration.
    ///
    /// When `flatten = true`: Only the filename is used (e.g., `nested/dir/file.md` → `file.md`)
    /// When `flatten = false`: Full path is preserved (e.g., `nested/dir/file.md` → `nested/dir/file.md`)
    ///
    /// # Default Behavior (from tool configuration)
    ///
    /// - **Agents**: Default to `true` (flatten)
    /// - **Commands**: Default to `true` (flatten)
    /// - **All others**: Default to `false` (preserve structure)
    #[must_use]
    pub fn get_flatten(&self) -> Option<bool> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.flatten,
        }
    }

    /// Get the install flag for this dependency.
    ///
    /// Returns the install setting if explicitly specified, or `None` to use the
    /// default behavior (install = true).
    ///
    /// When `install = false`: Dependency is resolved and content made available in
    /// template context, but file is not written to disk.
    ///
    /// When `install = true` (or `None`): Dependency is installed as a file.
    ///
    /// # Returns
    ///
    /// - `Some(false)` - Do not install the file, only make content available
    /// - `Some(true)` - Install the file normally
    /// - `None` - Use default behavior (install = true)
    #[must_use]
    pub fn get_install(&self) -> Option<bool> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.install,
        }
    }

    /// Get the template variable overrides for this resource.
    ///
    /// Returns the resource-specific template variables that override the global
    /// `[project]` configuration. These variables are used when:
    /// - Rendering the resource file itself
    /// - Resolving the resource's transitive dependencies
    ///
    /// This allows creating specialized variants of generic resources without duplication.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    /// use serde_json::json;
    ///
    /// // Resource with template variable overrides
    /// let resource = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("community".to_string()),
    ///     path: "agents/backend-engineer.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: Some("backend-engineer-golang.md".to_string()),
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(json!({ "project": { "language": "golang" } })),
    /// }));
    ///
    /// assert!(resource.get_template_vars().is_some());
    /// ```
    pub fn get_template_vars(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.template_vars.as_ref(),
        }
    }

    /// Get the path to the resource file.
    ///
    /// Returns the path component of the dependency, which is interpreted
    /// differently based on whether this is a local or remote dependency:
    ///
    /// - **Local dependencies**: Filesystem path relative to the manifest directory
    /// - **Remote dependencies**: Path within the Git repository
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - filesystem path
    /// let local = ResourceDependency::Simple("../shared/helper.md".to_string());
    /// assert_eq!(local.get_path(), "../shared/helper.md");
    ///
    /// // Remote dependency - repository path
    /// let remote = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/code-reviewer.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(remote.get_path(), "agents/code-reviewer.md");
    /// ```
    ///
    /// # Path Resolution
    ///
    /// The returned path should be processed appropriately based on the dependency type:
    /// - Local paths may need resolution against the manifest directory
    /// - Remote paths are used directly within the cloned repository
    /// - All paths should use forward slashes (/) for cross-platform compatibility
    #[must_use]
    pub fn get_path(&self) -> &str {
        match self {
            Self::Simple(path) => path,
            Self::Detailed(d) => &d.path,
        }
    }

    /// Check if this is a pattern-based dependency.
    ///
    /// Returns `true` if this dependency uses a glob pattern to match
    /// multiple resources, `false` if it specifies a single resource path.
    ///
    /// Patterns are detected by the presence of glob characters (`*`, `?`, `[`)
    /// in the path field.
    #[must_use]
    pub fn is_pattern(&self) -> bool {
        let path = self.get_path();
        path.contains('*') || path.contains('?') || path.contains('[')
    }

    /// Get the version constraint for dependency resolution.
    ///
    /// Returns the version constraint that should be used when resolving this
    /// dependency from a Git repository. For local dependencies, always returns `None`.
    ///
    /// # Priority Rules
    ///
    /// If both `version` and `git` fields are present in a detailed dependency,
    /// the `git` field takes precedence:
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),  // This is ignored
    ///     branch: Some("develop".to_string()),   // This takes precedence over version
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    ///
    /// assert_eq!(dep.get_version(), Some("develop"));
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency - no version
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.get_version().is_none());
    ///
    /// // Remote dependency with version
    /// let versioned = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(versioned.get_version(), Some("v1.0.0"));
    ///
    /// // Remote dependency with branch reference
    /// let branch_ref = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("repo".to_string()),
    ///     path: "file.md".to_string(),
    ///     version: None,
    ///     branch: Some("main".to_string()),
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert_eq!(branch_ref.get_version(), Some("main"));
    /// ```
    ///
    /// # Version Formats
    ///
    /// Supported version constraint formats include:
    /// - Semantic versions: `"v1.0.0"`, `"1.2.3"`
    /// - Semantic version ranges: `"^1.0.0"`, `"~2.1.0"`
    /// - Branch names: `"main"`, `"develop"`, `"latest"`, `"feature/new"`
    /// - Git tags: `"release-2023"`, `"stable"`
    /// - Commit SHAs: `"a1b2c3d4e5f6..."`
    #[must_use]
    pub fn get_version(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => {
                // Precedence: rev > branch > version
                d.rev.as_deref().or(d.branch.as_deref()).or(d.version.as_deref())
            }
        }
    }

    /// Check if this is a local filesystem dependency.
    ///
    /// Returns `true` if this dependency refers to a local file (no Git source),
    /// or `false` if it's a remote dependency that will be resolved from a
    /// Git repository.
    ///
    /// This is a convenience method equivalent to `self.get_source().is_none()`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{ResourceDependency, DetailedDependency};
    ///
    /// // Local dependency
    /// let local = ResourceDependency::Simple("../local/file.md".to_string());
    /// assert!(local.is_local());
    ///
    /// // Remote dependency
    /// let remote = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: Some("official".to_string()),
    ///     path: "agents/tool.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert!(!remote.is_local());
    ///
    /// // Local detailed dependency (no source specified)
    /// let local_detailed = ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///     source: None,
    ///     path: "../shared/tool.md".to_string(),
    ///     version: None,
    ///     branch: None,
    ///     rev: None,
    ///     command: None,
    ///     args: None,
    ///     target: None,
    ///     filename: None,
    ///     dependencies: None,
    ///     tool: Some("claude-code".to_string()),
    ///     flatten: None,
    ///     install: None,
    ///     template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    /// }));
    /// assert!(local_detailed.is_local());
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful for:
    /// - Choosing between filesystem and Git resolution strategies
    /// - Validation logic (local deps can't have versions)
    /// - Installation planning (local deps don't need caching)
    /// - Progress reporting (different steps for local vs remote)
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.get_source().is_none()
    }
}
