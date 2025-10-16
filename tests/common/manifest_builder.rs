//! Fluent builder for creating agpm.toml manifests in tests
//!
//! This module provides a type-safe, fluent API for constructing test manifests,
//! eliminating manual TOML string formatting and reducing test boilerplate.
//!
//! # Quick Examples
//!
//! ## Simple manifest with one agent
//! ```rust
//! use crate::common::ManifestBuilder;
//!
//! let manifest = ManifestBuilder::new()
//!     .add_source("official", "file:///path/to/repo.git")
//!     .add_agent("my-agent", |d| d
//!         .source("official")
//!         .path("agents/my-agent.md")
//!         .version("v1.0.0")
//!     )
//!     .build();
//! ```
//!
//! ## Manifest with multiple resources
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_sources(&[
//!         ("official", &official_url),
//!         ("community", &community_url),
//!     ])
//!     .add_standard_agent("my-agent", "official", "agents/my-agent.md")
//!     .add_standard_agent("helper", "community", "agents/helper.md")
//!     .add_snippet("utils", |d| d
//!         .source("official")
//!         .path("snippets/utils.md")
//!         .version("v2.0.0")
//!         .tool("agpm")
//!     )
//!     .build();
//! ```
//!
//! ## Local dependencies (no source/version)
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_local_agent("local-helper", "../local/agents/helper.md")
//!     .build();
//! ```
//!
//! ## Pattern-based dependencies
//! ```rust
//! let manifest = ManifestBuilder::new()
//!     .add_source("community", &url)
//!     .add_agent_pattern("ai-agents", "community", "agents/ai/*.md", "v1.0.0")
//!     .build();
//! ```

use std::collections::HashMap;

/// Builder for creating test manifests with type safety
///
/// This builder uses a fluent API pattern to construct agpm.toml manifests
/// programmatically, ensuring type safety and eliminating string formatting errors.
#[derive(Default, Debug)]
pub struct ManifestBuilder {
    sources: HashMap<String, String>,
    target_config: Option<TargetConfig>,
    agents: Vec<DependencyEntry>,
    snippets: Vec<DependencyEntry>,
    commands: Vec<DependencyEntry>,
    scripts: Vec<DependencyEntry>,
    hooks: Vec<DependencyEntry>,
    mcp_servers: Vec<DependencyEntry>,
}

/// Configuration for the [target] section
#[derive(Debug, Clone)]
struct TargetConfig {
    agents: Option<String>,
    snippets: Option<String>,
    commands: Option<String>,
    scripts: Option<String>,
    hooks: Option<String>,
    mcp_servers: Option<String>,
    gitignore: Option<bool>,
}

/// Builder for configuring the [target] section
#[derive(Default, Debug)]
pub struct TargetConfigBuilder {
    agents: Option<String>,
    snippets: Option<String>,
    commands: Option<String>,
    scripts: Option<String>,
    hooks: Option<String>,
    mcp_servers: Option<String>,
    gitignore: Option<bool>,
}

impl TargetConfigBuilder {
    /// Set the agents target path
    pub fn agents(mut self, path: &str) -> Self {
        self.agents = Some(path.to_string());
        self
    }

    /// Set the snippets target path
    pub fn snippets(mut self, path: &str) -> Self {
        self.snippets = Some(path.to_string());
        self
    }

    /// Set the commands target path
    pub fn commands(mut self, path: &str) -> Self {
        self.commands = Some(path.to_string());
        self
    }

    /// Set the scripts target path
    pub fn scripts(mut self, path: &str) -> Self {
        self.scripts = Some(path.to_string());
        self
    }

    /// Set the hooks target path
    pub fn hooks(mut self, path: &str) -> Self {
        self.hooks = Some(path.to_string());
        self
    }

    /// Set the mcp-servers target path
    pub fn mcp_servers(mut self, path: &str) -> Self {
        self.mcp_servers = Some(path.to_string());
        self
    }

    /// Enable or disable gitignore management
    pub fn gitignore(mut self, enabled: bool) -> Self {
        self.gitignore = Some(enabled);
        self
    }

    fn build(self) -> TargetConfig {
        TargetConfig {
            agents: self.agents,
            snippets: self.snippets,
            commands: self.commands,
            scripts: self.scripts,
            hooks: self.hooks,
            mcp_servers: self.mcp_servers,
            gitignore: self.gitignore,
        }
    }
}

/// A single dependency entry with all possible fields
#[derive(Debug, Clone)]
struct DependencyEntry {
    name: String,
    source: Option<String>,
    path: String,
    version: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    tool: Option<String>,
    target: Option<String>,
}

/// Builder for configuring a single dependency
///
/// This builder is used within the resource-specific methods (add_agent, add_snippet, etc.)
/// to configure individual dependencies with a fluent API.
#[derive(Debug)]
pub struct DependencyBuilder {
    name: String,
    source: Option<String>,
    path: Option<String>,
    version: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    tool: Option<String>,
    target: Option<String>,
}

impl DependencyBuilder {
    /// Set the source repository name
    pub fn source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Set the path to the resource in the repository
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Set the version constraint (e.g., "v1.0.0", "^v2.0", "main")
    pub fn version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    /// Set the branch reference (e.g., "main", "develop")
    pub fn branch(mut self, branch: &str) -> Self {
        self.branch = Some(branch.to_string());
        self
    }

    /// Set the commit reference (e.g., "abc123def")
    pub fn rev(mut self, rev: &str) -> Self {
        self.rev = Some(rev.to_string());
        self
    }

    /// Set the target tool (e.g., "claude-code", "opencode", "agpm")
    pub fn tool(mut self, tool: &str) -> Self {
        self.tool = Some(tool.to_string());
        self
    }

    /// Set a custom installation target path
    pub fn target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// Build the dependency entry (internal use)
    fn build(self) -> DependencyEntry {
        DependencyEntry {
            name: self.name,
            source: self.source,
            path: self.path.expect("path is required for dependency"),
            version: self.version,
            branch: self.branch,
            rev: self.rev,
            tool: self.tool,
            target: self.target,
        }
    }
}

impl ManifestBuilder {
    /// Create a new empty manifest builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source repository
    ///
    /// # Example
    /// ```rust
    /// builder.add_source("official", "file:///path/to/repo.git")
    /// ```
    pub fn add_source(mut self, name: &str, url: &str) -> Self {
        self.sources.insert(name.to_string(), url.to_string());
        self
    }

    /// Add multiple sources at once
    ///
    /// # Example
    /// ```rust
    /// builder.add_sources(&[
    ///     ("official", &official_url),
    ///     ("community", &community_url),
    /// ])
    /// ```
    pub fn add_sources(mut self, sources: &[(&str, &str)]) -> Self {
        for (name, url) in sources {
            self.sources.insert(name.to_string(), url.to_string());
        }
        self
    }

    /// Configure the [target] section with custom paths and gitignore setting
    ///
    /// # Example
    /// ```rust
    /// builder.with_target_config(|t| t
    ///     .agents(".claude/agents")
    ///     .snippets(".agpm/snippets")
    ///     .gitignore(true)
    /// )
    /// ```
    pub fn with_target_config<F>(mut self, config: F) -> Self
    where
        F: FnOnce(TargetConfigBuilder) -> TargetConfigBuilder,
    {
        let builder = TargetConfigBuilder::default();
        self.target_config = Some(config(builder).build());
        self
    }

    /// Set gitignore field in [target] section (convenience method)
    ///
    /// # Example
    /// ```rust
    /// builder.with_gitignore(true)
    /// ```
    pub fn with_gitignore(mut self, enabled: bool) -> Self {
        if let Some(ref mut config) = self.target_config {
            config.gitignore = Some(enabled);
        } else {
            self.target_config = Some(TargetConfig {
                agents: None,
                snippets: None,
                commands: None,
                scripts: None,
                hooks: None,
                mcp_servers: None,
                gitignore: Some(enabled),
            });
        }
        self
    }

    /// Add an agent dependency with full configuration
    ///
    /// # Example
    /// ```rust
    /// builder.add_agent("my-agent", |d| d
    ///     .source("official")
    ///     .path("agents/my-agent.md")
    ///     .version("v1.0.0")
    /// )
    /// ```
    pub fn add_agent<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.agents.push(entry);
        self
    }

    /// Add a snippet dependency with full configuration
    pub fn add_snippet<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.snippets.push(entry);
        self
    }

    /// Add a command dependency with full configuration
    pub fn add_command<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.commands.push(entry);
        self
    }

    /// Add a script dependency with full configuration
    pub fn add_script<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.scripts.push(entry);
        self
    }

    /// Add a hook dependency with full configuration
    pub fn add_hook<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.hooks.push(entry);
        self
    }

    /// Add an MCP server dependency with full configuration
    pub fn add_mcp_server<F>(mut self, name: &str, config: F) -> Self
    where
        F: FnOnce(DependencyBuilder) -> DependencyBuilder,
    {
        let builder = DependencyBuilder {
            name: name.to_string(),
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            tool: None,
            target: None,
        };
        let entry = config(builder).build();
        self.mcp_servers.push(entry);
        self
    }

    /// Build the final TOML string
    ///
    /// Constructs a valid agpm.toml manifest from the builder state.
    pub fn build(self) -> String {
        // Helper to escape string values for TOML (backslashes need to be doubled)
        fn escape_toml_string(s: &str) -> String {
            s.replace('\\', "\\\\")
        }

        let mut toml = String::new();

        // Sources section
        if !self.sources.is_empty() {
            toml.push_str("[sources]\n");
            for (name, url) in &self.sources {
                toml.push_str(&format!("{} = \"{}\"\n", name, escape_toml_string(url)));
            }
            toml.push('\n');
        }

        // Helper to format dependency sections
        fn format_dependencies(toml: &mut String, section: &str, deps: &[DependencyEntry]) {
            if !deps.is_empty() {
                toml.push_str(&format!("[{}]\n", section));
                for dep in deps {
                    toml.push_str(&format!("{} = {{ ", dep.name));

                    if let Some(source) = &dep.source {
                        toml.push_str(&format!("source = \"{}\", ", escape_toml_string(source)));
                    }

                    toml.push_str(&format!("path = \"{}\"", escape_toml_string(&dep.path)));

                    if let Some(version) = &dep.version {
                        toml.push_str(&format!(", version = \"{}\"", escape_toml_string(version)));
                    }

                    if let Some(branch) = &dep.branch {
                        toml.push_str(&format!(", branch = \"{}\"", escape_toml_string(branch)));
                    }

                    if let Some(rev) = &dep.rev {
                        toml.push_str(&format!(", rev = \"{}\"", escape_toml_string(rev)));
                    }

                    if let Some(tool) = &dep.tool {
                        toml.push_str(&format!(", tool = \"{}\"", escape_toml_string(tool)));
                    }

                    if let Some(target) = &dep.target {
                        toml.push_str(&format!(", target = \"{}\"", escape_toml_string(target)));
                    }

                    toml.push_str(" }\n");
                }
                toml.push('\n');
            }
        }

        // Format all resource sections
        format_dependencies(&mut toml, "agents", &self.agents);
        format_dependencies(&mut toml, "snippets", &self.snippets);
        format_dependencies(&mut toml, "commands", &self.commands);
        format_dependencies(&mut toml, "scripts", &self.scripts);
        format_dependencies(&mut toml, "hooks", &self.hooks);
        format_dependencies(&mut toml, "mcp-servers", &self.mcp_servers);

        // Target configuration section
        if let Some(config) = self.target_config {
            let mut has_fields = false;
            let mut target_section = String::from("[target]\n");

            if let Some(path) = config.agents {
                target_section.push_str(&format!("agents = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.snippets {
                target_section.push_str(&format!("snippets = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.commands {
                target_section.push_str(&format!("commands = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.scripts {
                target_section.push_str(&format!("scripts = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.hooks {
                target_section.push_str(&format!("hooks = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(path) = config.mcp_servers {
                target_section
                    .push_str(&format!("mcp-servers = \"{}\"\n", escape_toml_string(&path)));
                has_fields = true;
            }
            if let Some(enabled) = config.gitignore {
                target_section.push_str(&format!("gitignore = {}\n", enabled));
                has_fields = true;
            }

            if has_fields {
                toml.push_str(&target_section);
                toml.push('\n');
            }
        }

        toml
    }
}

// Convenience methods for common patterns
impl ManifestBuilder {
    /// Quick add: agent with standard v1.0.0 version from source
    ///
    /// # Example
    /// ```rust
    /// builder.add_standard_agent("my-agent", "official", "agents/my-agent.md")
    /// ```
    pub fn add_standard_agent(self, name: &str, source: &str, path: &str) -> Self {
        self.add_agent(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: snippet with standard v1.0.0 version from source
    pub fn add_standard_snippet(self, name: &str, source: &str, path: &str) -> Self {
        self.add_snippet(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: command with standard v1.0.0 version from source
    pub fn add_standard_command(self, name: &str, source: &str, path: &str) -> Self {
        self.add_command(name, |d| d.source(source).path(path).version("v1.0.0"))
    }

    /// Quick add: local agent dependency (no source/version)
    ///
    /// # Example
    /// ```rust
    /// builder.add_local_agent("local-helper", "../local/agents/helper.md")
    /// ```
    pub fn add_local_agent(self, name: &str, path: &str) -> Self {
        self.add_agent(name, |d| d.path(path))
    }

    /// Quick add: local snippet dependency (no source/version)
    pub fn add_local_snippet(self, name: &str, path: &str) -> Self {
        self.add_snippet(name, |d| d.path(path))
    }

    /// Quick add: local command dependency (no source/version)
    pub fn add_local_command(self, name: &str, path: &str) -> Self {
        self.add_command(name, |d| d.path(path))
    }

    /// Quick add: pattern-based agent dependency
    ///
    /// # Example
    /// ```rust
    /// builder.add_agent_pattern("ai-agents", "community", "agents/ai/*.md", "v1.0.0")
    /// ```
    pub fn add_agent_pattern(self, name: &str, source: &str, pattern: &str, version: &str) -> Self {
        self.add_agent(name, |d| d.source(source).path(pattern).version(version))
    }

    /// Quick add: pattern-based snippet dependency
    pub fn add_snippet_pattern(
        self,
        name: &str,
        source: &str,
        pattern: &str,
        version: &str,
    ) -> Self {
        self.add_snippet(name, |d| d.source(source).path(pattern).version(version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manifest() {
        let manifest = ManifestBuilder::new().build();
        assert_eq!(manifest, "");
    }

    #[test]
    fn test_sources_only() {
        let manifest = ManifestBuilder::new()
            .add_source("official", "file:///path/to/repo.git")
            .add_source("community", "https://github.com/user/repo.git")
            .build();

        assert!(manifest.contains("[sources]"));
        assert!(manifest.contains("official = \"file:///path/to/repo.git\""));
        assert!(manifest.contains("community = \"https://github.com/user/repo.git\""));
    }

    #[test]
    fn test_add_sources_bulk() {
        let manifest = ManifestBuilder::new()
            .add_sources(&[
                ("official", "file:///official.git"),
                ("community", "file:///community.git"),
            ])
            .build();

        assert!(manifest.contains("[sources]"));
        assert!(manifest.contains("official = \"file:///official.git\""));
        assert!(manifest.contains("community = \"file:///community.git\""));
    }

    #[test]
    fn test_single_agent() {
        let manifest = ManifestBuilder::new()
            .add_source("official", "file:///repo.git")
            .add_agent("my-agent", |d| {
                d.source("official").path("agents/my-agent.md").version("v1.0.0")
            })
            .build();

        assert!(manifest.contains("[sources]"));
        assert!(manifest.contains("[agents]"));
        assert!(manifest.contains("my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\" }"));
    }

    #[test]
    fn test_standard_agent_convenience() {
        let manifest = ManifestBuilder::new()
            .add_source("official", "file:///repo.git")
            .add_standard_agent("my-agent", "official", "agents/my-agent.md")
            .build();

        assert!(manifest.contains("my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\" }"));
    }

    #[test]
    fn test_local_agent() {
        let manifest = ManifestBuilder::new()
            .add_local_agent("local-helper", "../local/agents/helper.md")
            .build();

        assert!(manifest.contains("[agents]"));
        assert!(manifest.contains("local-helper = { path = \"../local/agents/helper.md\" }"));
        assert!(!manifest.contains("source"));
        assert!(!manifest.contains("version"));
    }

    #[test]
    fn test_agent_with_tool() {
        let manifest = ManifestBuilder::new()
            .add_agent("opencode-agent", |d| {
                d.source("official").path("agents/helper.md").version("v1.0.0").tool("opencode")
            })
            .build();

        assert!(manifest.contains("tool = \"opencode\""));
    }

    #[test]
    fn test_agent_with_target() {
        let manifest = ManifestBuilder::new()
            .add_agent("special", |d| {
                d.source("official")
                    .path("agents/special.md")
                    .version("v1.0.0")
                    .target("custom/special.md")
            })
            .build();

        assert!(manifest.contains("target = \"custom/special.md\""));
    }

    #[test]
    fn test_multiple_resource_types() {
        let manifest = ManifestBuilder::new()
            .add_source("official", "file:///repo.git")
            .add_standard_agent("agent1", "official", "agents/agent1.md")
            .add_standard_snippet("snippet1", "official", "snippets/snippet1.md")
            .add_standard_command("cmd1", "official", "commands/cmd1.md")
            .build();

        assert!(manifest.contains("[agents]"));
        assert!(manifest.contains("[snippets]"));
        assert!(manifest.contains("[commands]"));
        assert!(manifest.contains("agent1"));
        assert!(manifest.contains("snippet1"));
        assert!(manifest.contains("cmd1"));
    }

    #[test]
    fn test_pattern_dependency() {
        let manifest = ManifestBuilder::new()
            .add_source("community", "file:///repo.git")
            .add_agent_pattern("ai-agents", "community", "agents/ai/*.md", "v1.0.0")
            .build();

        assert!(manifest.contains("ai-agents = { source = \"community\", path = \"agents/ai/*.md\", version = \"v1.0.0\" }"));
    }

    #[test]
    fn test_complex_manifest() {
        let manifest = ManifestBuilder::new()
            .add_sources(&[
                ("official", "file:///official.git"),
                ("community", "file:///community.git"),
            ])
            .add_standard_agent("agent1", "official", "agents/agent1.md")
            .add_agent("agent2", |d| {
                d.source("community").path("agents/agent2.md").version("v2.0.0").tool("opencode")
            })
            .add_local_agent("local-agent", "../local/agents/local.md")
            .add_standard_snippet("snippet1", "official", "snippets/snippet1.md")
            .add_mcp_server("fs", |d| {
                d.source("official").path("mcp-servers/filesystem.json").version("v1.0.0")
            })
            .build();

        // Verify structure
        assert!(manifest.contains("[sources]"));
        assert!(manifest.contains("[agents]"));
        assert!(manifest.contains("[snippets]"));
        assert!(manifest.contains("[mcp-servers]"));

        // Verify content
        assert!(manifest.contains("official = \"file:///official.git\""));
        assert!(manifest.contains("community = \"file:///community.git\""));
        assert!(manifest.contains("agent1"));
        assert!(manifest.contains("agent2"));
        assert!(manifest.contains("tool = \"opencode\""));
        assert!(manifest.contains("local-agent"));
        assert!(manifest.contains("snippet1"));
        assert!(manifest.contains("fs"));
    }

    #[test]
    #[should_panic(expected = "path is required")]
    fn test_missing_path_panics() {
        ManifestBuilder::new()
            .add_agent("broken", |d| d.source("official").version("v1.0.0"))
            .build();
    }
}
