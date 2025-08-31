//! Add command implementation for CCPM
//!
//! This module provides functionality to add sources and dependencies
//! to a CCPM project manifest. It supports both Git repository sources
//! and various types of resource dependencies (agents, snippets, commands, MCP servers).

use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use regex::Regex;
use std::path::Path;

use crate::cache::Cache;
use crate::cli::resource_ops::{
    create_lock_entry, fetch_resource_content, get_resource_target_path, install_resource_file,
    update_settings_for_hook, update_settings_for_mcp_server, validate_resource_content,
};
use crate::lockfile::LockFile;
use crate::manifest::{
    find_manifest_with_optional, DetailedDependency, Manifest, ResourceDependency,
};
use crate::models::{
    AgentDependency, CommandDependency, DependencyType, HookDependency, McpServerDependency,
    ScriptDependency, SnippetDependency, SourceSpec,
};
use crate::utils::fs::atomic_write;

/// Command to add sources and dependencies to a CCPM project.
#[derive(Args)]
pub struct AddCommand {
    /// The specific add operation to perform
    #[command(subcommand)]
    command: AddSubcommand,
}

/// Subcommands for the add command.
#[derive(Subcommand)]
enum AddSubcommand {
    /// Add a new Git repository source to the manifest
    Source {
        /// Name for the source
        name: String,
        /// Git repository URL
        url: String,
    },

    /// Add a resource dependency to the manifest
    #[command(subcommand)]
    Dep(DependencySubcommand),
}

/// Dependency subcommands for different resource types
#[derive(Subcommand)]
enum DependencySubcommand {
    /// Add an agent dependency
    Agent(AgentDependency),

    /// Add a snippet dependency
    Snippet(SnippetDependency),

    /// Add a command dependency
    Command(CommandDependency),

    /// Add a script dependency
    Script(ScriptDependency),

    /// Add a hook dependency
    Hook(HookDependency),

    /// Add an MCP server dependency
    McpServer(McpServerDependency),
}

impl AddCommand {
    /// Execute the add command with an optional manifest path.
    ///
    /// This method allows specifying a custom path to the ccpm.toml manifest file.
    /// If no path is provided, it will search for ccpm.toml in the current directory
    /// and parent directories.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional path to the ccpm.toml file
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the add operation completed successfully
    /// - `Err(anyhow::Error)` if the operation fails (e.g., invalid manifest, source not found)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ccpm::cli::add::{AddCommand, AddSubcommand};
    /// use std::path::PathBuf;
    ///
    /// let cmd = AddCommand {
    ///     command: AddSubcommand::Source {
    ///         name: "my-source".to_string(),
    ///         url: "https://github.com/example/repo.git".to_string(),
    ///     }
    /// };
    ///
    /// // Use default manifest location
    /// cmd.execute_with_manifest_path(None).await?;
    ///
    /// // Or specify custom manifest path
    /// cmd.execute_with_manifest_path(Some(PathBuf::from("/path/to/ccpm.toml"))).await?;
    /// ```
    pub async fn execute_with_manifest_path(
        self,
        manifest_path: Option<std::path::PathBuf>,
    ) -> Result<()> {
        match self.command {
            AddSubcommand::Source { name, url } => {
                add_source_with_manifest_path(SourceSpec { name, url }, manifest_path).await
            }
            AddSubcommand::Dep(dep_command) => {
                let dep_type = match dep_command {
                    DependencySubcommand::Agent(agent) => DependencyType::Agent(agent),
                    DependencySubcommand::Snippet(snippet) => DependencyType::Snippet(snippet),
                    DependencySubcommand::Command(command) => DependencyType::Command(command),
                    DependencySubcommand::Script(script) => DependencyType::Script(script),
                    DependencySubcommand::Hook(hook) => DependencyType::Hook(hook),
                    DependencySubcommand::McpServer(mcp) => DependencyType::McpServer(mcp),
                };
                add_dependency_with_manifest_path(dep_type, manifest_path).await
            }
        }
    }
}

/// Add a new source to the manifest with optional manifest path
async fn add_source_with_manifest_path(
    source: SourceSpec,
    manifest_path: Option<std::path::PathBuf>,
) -> Result<()> {
    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Check if source already exists
    if manifest.sources.contains_key(&source.name) {
        return Err(anyhow!(
            "Source '{}' already exists in manifest",
            source.name
        ));
    }

    // Add the source
    manifest
        .sources
        .insert(source.name.clone(), source.url.clone());

    // Save the manifest
    atomic_write(
        &manifest_path,
        toml::to_string_pretty(&manifest)?.as_bytes(),
    )?;

    println!(
        "{}",
        format!("Added source '{}' → {}", source.name, source.url).green()
    );

    Ok(())
}

/// Add a dependency to the manifest and install it with optional manifest path
async fn add_dependency_with_manifest_path(
    dep_type: DependencyType,
    manifest_path: Option<std::path::PathBuf>,
) -> Result<()> {
    let common = dep_type.common();
    let (name, dependency) = parse_dependency_spec(&common.spec, &common.name)?;

    // Find manifest file
    let manifest_path = find_manifest_with_optional(manifest_path)?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Determine the resource type
    let resource_type = dep_type.resource_type();

    // Handle MCP servers (now using standard ResourceDependency)
    if let DependencyType::McpServer(_) = &dep_type {
        // Check if dependency already exists
        if manifest.mcp_servers.contains_key(&name) && !common.force {
            return Err(anyhow!(
                "MCP server '{}' already exists in manifest. Use --force to overwrite",
                name
            ));
        }

        // Add to manifest (MCP servers now use standard ResourceDependency)
        manifest
            .mcp_servers
            .insert(name.clone(), dependency.clone());
    } else {
        // Handle regular resources (agents, snippets, commands, scripts, hooks)
        let section = match &dep_type {
            DependencyType::Agent(_) => &mut manifest.agents,
            DependencyType::Snippet(_) => &mut manifest.snippets,
            DependencyType::Command(_) => &mut manifest.commands,
            DependencyType::Script(_) => &mut manifest.scripts,
            DependencyType::Hook(_) => &mut manifest.hooks,
            DependencyType::McpServer(_) => unreachable!(), // Handled above
        };

        // Check if dependency already exists
        if section.contains_key(&name) && !common.force {
            return Err(anyhow!(
                "{} '{}' already exists in manifest. Use --force to overwrite",
                resource_type,
                name
            ));
        }

        // Add to manifest
        section.insert(name.clone(), dependency.clone());
    }

    // Save the manifest
    atomic_write(
        &manifest_path,
        toml::to_string_pretty(&manifest)?.as_bytes(),
    )?;

    println!("{}", format!("Added {resource_type} '{name}'").green());

    // Auto-install the dependency
    println!("{}", "Installing dependency...".cyan());
    install_single_dependency(&name, &dependency, resource_type, &manifest, &manifest_path).await?;

    Ok(())
}

/// Parse a dependency specification string into a name and `ResourceDependency`
fn parse_dependency_spec(
    spec: &str,
    custom_name: &Option<String>,
) -> Result<(String, ResourceDependency)> {
    // Check if this is a Windows absolute path (e.g., C:\path\to\file)
    // or Unix absolute path (e.g., /path/to/file)
    let is_absolute_path = {
        #[cfg(windows)]
        {
            // Windows: Check for drive letter (C:) or UNC path (\\server)
            spec.len() >= 3
                && spec.chars().nth(1) == Some(':')
                && spec
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                || spec.starts_with("\\\\")
        }
        #[cfg(not(windows))]
        {
            // Unix: Check for leading /
            spec.starts_with('/')
        }
    };

    // Check if it's a local file path
    let is_local_path = is_absolute_path || spec.starts_with("file:") || Path::new(spec).exists();

    // Pattern: source:path@version or source:path
    // But only apply if it's not a local path
    let remote_pattern = Regex::new(r"^([^:]+):([^@]+)(?:@(.+))?$")?;

    if !is_local_path && remote_pattern.is_match(spec) {
        let captures = remote_pattern.captures(spec).unwrap();
        // Remote dependency
        let source = captures.get(1).unwrap().as_str().to_string();
        let path = captures.get(2).unwrap().as_str().to_string();
        let version = captures.get(3).map(|m| m.as_str().to_string());

        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        Ok((
            name,
            ResourceDependency::Detailed(DetailedDependency {
                source: Some(source),
                path,
                version,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        ))
    } else if is_local_path {
        // Local dependency
        let path = if spec.starts_with("file:") {
            spec.trim_start_matches("file:")
        } else {
            spec
        };

        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        Ok((name, ResourceDependency::Simple(path.to_string())))
    } else {
        // Treat as simple path
        let name = custom_name.clone().unwrap_or_else(|| {
            Path::new(spec)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        Ok((name, ResourceDependency::Simple(spec.to_string())))
    }
}

/// Install a single dependency that was just added
async fn install_single_dependency(
    name: &str,
    dependency: &ResourceDependency,
    resource_type: &str,
    manifest: &Manifest,
    manifest_path: &Path,
) -> Result<()> {
    let project_root = manifest_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid manifest path"))?;
    let cache = Cache::new()?;

    // Step 1: Get the source file content
    let (source_path, content) = fetch_resource_content(dependency, manifest, &cache).await?;

    // Step 2: Validate content based on resource type
    validate_resource_content(&content, resource_type, name)?;

    // Step 3: Determine target path and install the file
    let target_path = if resource_type == "script" {
        // For scripts, preserve the original extension
        let extension = source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("sh");
        project_root
            .join(&manifest.target.scripts)
            .join(format!("{}.{}", name, extension))
    } else {
        get_resource_target_path(name, resource_type, manifest, project_root)?
    };

    install_resource_file(&target_path, &content)?;

    // Step 4: Update lockfile
    let lockfile_path = manifest_path_to_lockfile(manifest_path);
    let mut lockfile = if lockfile_path.exists() {
        LockFile::load(&lockfile_path)?
    } else {
        LockFile::new()
    };

    let lock_entry = create_lock_entry(
        name,
        dependency,
        manifest,
        &target_path,
        &content,
        None, // Not needed for direct installations
    )?;

    // Add to appropriate section
    match resource_type {
        "agent" => lockfile.agents.push(lock_entry),
        "snippet" => lockfile.snippets.push(lock_entry),
        "command" => lockfile.commands.push(lock_entry),
        "script" => lockfile.scripts.push(lock_entry),
        "hook" => lockfile.hooks.push(lock_entry),
        "mcp-server" => lockfile.mcp_servers.push(lock_entry),
        _ => {}
    }

    lockfile.save(&lockfile_path)?;

    // Step 5: Update settings.local.json if needed (hooks and MCP servers)
    if resource_type == "hook" {
        update_settings_for_hook(name, &content, project_root)?;
    } else if resource_type == "mcp-server" {
        update_settings_for_mcp_server(name, &content, project_root)?;
    }

    println!(
        "{}",
        format!(
            "✓ Installed {} '{}' to {}",
            resource_type,
            name,
            target_path.display()
        )
        .green()
    );

    Ok(())
}

/// Convert manifest path to lockfile path
fn manifest_path_to_lockfile(manifest_path: &Path) -> std::path::PathBuf {
    manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("ccpm.lock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DependencySpec;
    use tempfile::TempDir;

    // Helper function to create a test manifest with basic structure
    fn create_test_manifest(manifest_path: &Path) {
        let manifest_content = r#"[sources]

[target]
agents = ".claude/agents"
snippets = ".claude/ccpm/snippets"
commands = ".claude/commands"

[agents]

[snippets]

[commands]

[mcp-servers]
"#;
        // Ensure parent directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(manifest_path, manifest_content).unwrap();
    }

    // Helper function to create a test manifest with existing sources and dependencies
    fn create_test_manifest_with_content(manifest_path: &Path) {
        let manifest_content = r#"[sources]
existing = "https://github.com/existing/repo.git"

[target]
agents = ".claude/agents"
snippets = ".claude/ccpm/snippets"
commands = ".claude/commands"

[agents]
existing-agent = "../local/agent.md"

[snippets]
existing-snippet = { source = "existing", path = "snippets/utils.md", version = "v1.0.0" }

[commands]
existing-command = { source = "existing", path = "commands/deploy.md", version = "v1.0.0" }

[mcp-servers]
existing-mcp = "../local/mcp-servers/existing.json"
"#;
        // Ensure parent directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(manifest_path, manifest_content).unwrap();
    }

    // Test existing functions
    #[test]
    fn test_parse_remote_dependency() {
        let (name, dep) =
            parse_dependency_spec("official:agents/reviewer.md@v1.0.0", &None).unwrap();

        assert_eq!(name, "reviewer");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("official".to_string()));
            assert_eq!(detailed.path, "agents/reviewer.md");
            assert_eq!(detailed.version, Some("v1.0.0".to_string()));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_local_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "# Test").unwrap();

        let (name, dep) =
            parse_dependency_spec(test_file.to_str().unwrap(), &Some("my-agent".to_string()))
                .unwrap();

        assert_eq!(name, "my-agent");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, test_file.to_str().unwrap());
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_with_custom_name() {
        let (name, _) = parse_dependency_spec(
            "official:snippets/utils.md@v1.0.0",
            &Some("my-utils".to_string()),
        )
        .unwrap();

        assert_eq!(name, "my-utils");
    }

    #[test]
    fn test_parse_dependency_without_version() {
        let (name, dep) = parse_dependency_spec("source:path/to/file.md", &None).unwrap();
        assert_eq!(name, "file");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source.as_deref(), Some("source"));
            assert_eq!(detailed.path, "path/to/file.md");
            assert!(detailed.version.is_none());
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_dependency_with_branch() {
        let (name, dep) = parse_dependency_spec("src:file.md@main", &None).unwrap();
        assert_eq!(name, "file");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.version.as_deref(), Some("main"));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_manifest_path_to_lockfile() {
        use std::path::PathBuf;

        let manifest = PathBuf::from("/project/ccpm.toml");
        let lockfile = manifest_path_to_lockfile(&manifest);
        assert_eq!(lockfile, PathBuf::from("/project/ccpm.lock"));

        let manifest2 = PathBuf::from("./ccpm.toml");
        let lockfile2 = manifest_path_to_lockfile(&manifest2);
        assert_eq!(lockfile2, PathBuf::from("./ccpm.lock"));
    }

    // NEW COMPREHENSIVE TESTS

    #[tokio::test]
    async fn test_execute_add_source() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Source {
                name: "test-source".to_string(),
                url: "https://github.com/test/repo.git".to_string(),
            },
        };

        let result = add_command
            .execute_with_manifest_path(Some(manifest_path.clone()))
            .await;

        assert!(result.is_ok(), "Failed to execute add source: {result:?}");

        // Verify source was added to manifest
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("test-source"));
        assert_eq!(
            manifest.sources.get("test-source").unwrap(),
            "https://github.com/test/repo.git"
        );
    }

    #[tokio::test]
    async fn test_execute_add_agent_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local agent file for testing
        let agent_file = temp_dir.path().join("test-agent.md");
        std::fs::write(&agent_file, "# Test Agent\nThis is a test agent.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Agent(AgentDependency {
                common: DependencySpec {
                    spec: agent_file.to_string_lossy().to_string(),
                    name: Some("my-test-agent".to_string()),
                    force: false,
                },
            })),
        };

        // Execute the command - this should now succeed with local files
        let result = add_command
            .execute_with_manifest_path(Some(manifest_path.clone()))
            .await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local agent: {result:?}");

        // Verify the agent was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.agents.contains_key("my-test-agent"));
    }

    #[tokio::test]
    async fn test_execute_add_snippet_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("test-snippet.md");
        std::fs::write(&snippet_file, "# Test Snippet\nUseful code snippet.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Snippet(SnippetDependency {
                common: DependencySpec {
                    spec: snippet_file.to_string_lossy().to_string(),
                    name: Some("my-snippet".to_string()),
                    force: false,
                },
            })),
        };

        let result = add_command
            .execute_with_manifest_path(Some(manifest_path.clone()))
            .await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local snippet: {result:?}");

        // Verify the snippet was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.snippets.contains_key("my-snippet"));
    }

    #[tokio::test]
    async fn test_execute_add_command_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("test-command.md");
        std::fs::write(&command_file, "# Test Command\nUseful command.").unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Command(CommandDependency {
                common: DependencySpec {
                    spec: command_file.to_string_lossy().to_string(),
                    name: Some("my-command".to_string()),
                    force: false,
                },
            })),
        };

        let result = add_command
            .execute_with_manifest_path(Some(manifest_path.clone()))
            .await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local command: {result:?}");

        // Verify the command was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.commands.contains_key("my-command"));
    }

    #[tokio::test]
    async fn test_execute_add_mcp_server_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "npx",
            "args": ["-y", "@test/mcp-server"],
            "env": {}
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        // Change to temp directory

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::McpServer(McpServerDependency {
                common: DependencySpec {
                    spec: mcp_file_path.to_string_lossy().to_string(),
                    name: Some("test-mcp".to_string()),
                    force: false,
                },
            })),
        };

        let result = add_command
            .execute_with_manifest_path(Some(manifest_path.clone()))
            .await;

        assert!(result.is_ok(), "Failed to add MCP server: {result:?}");

        // Verify the manifest was updated
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("test-mcp"));

        // Check that the file was installed
        let installed_path = temp_dir
            .path()
            .join(".claude/ccpm/mcp-servers/test-mcp.json");
        assert!(
            installed_path.exists(),
            "MCP server config should be installed"
        );

        // Check that settings.local.json was updated
        let settings_path = temp_dir.path().join(".claude/settings.local.json");
        assert!(settings_path.exists(), "Settings file should be created");
    }

    #[tokio::test]
    async fn test_add_source_success() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory

        let source = SourceSpec {
            name: "new-source".to_string(),
            url: "https://github.com/new/repo.git".to_string(),
        };

        let result = add_source_with_manifest_path(source, Some(manifest_path.clone())).await;
        assert!(result.is_ok(), "Failed to add source: {result:?}");

        // Verify source was added
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("new-source"));
        assert_eq!(
            manifest.sources.get("new-source").unwrap(),
            "https://github.com/new/repo.git"
        );
    }

    #[tokio::test]
    async fn test_add_source_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory

        let source = SourceSpec {
            name: "existing".to_string(),
            url: "https://github.com/different/repo.git".to_string(),
        };

        let result = add_source_with_manifest_path(source, Some(manifest_path.clone())).await;
        assert!(result.is_err());

        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("Source 'existing' already exists"));
    }

    #[test]
    fn test_parse_dependency_spec_file_prefix() {
        // Test file: prefix - now correctly treated as local path
        let (name, dep) = parse_dependency_spec("file:/path/to/agent.md", &None).unwrap();
        assert_eq!(name, "agent");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "/path/to/agent.md"); // Path without file: prefix
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_simple_path() {
        // Test simple path when file doesn't exist
        let (name, dep) = parse_dependency_spec("nonexistent/path.md", &None).unwrap();
        assert_eq!(name, "path");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "nonexistent/path.md");
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_custom_name_simple() {
        let (name, dep) =
            parse_dependency_spec("simple/path.md", &Some("custom-name".to_string())).unwrap();
        assert_eq!(name, "custom-name");
        if let ResourceDependency::Simple(path) = dep {
            assert_eq!(path, "simple/path.md");
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_path_without_extension() {
        let (name, dep) = parse_dependency_spec("source:agents/noext@v1.0", &None).unwrap();
        assert_eq!(name, "noext");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("source".to_string()));
            assert_eq!(detailed.path, "agents/noext");
            assert_eq!(detailed.version, Some("v1.0".to_string()));
        } else {
            panic!("Expected detailed dependency");
        }
    }

    #[test]
    fn test_parse_dependency_spec_unknown_fallback() {
        let (name, dep) = parse_dependency_spec("malformed::", &None).unwrap();
        // The regex captures :: as the path part after the first colon
        assert_eq!(name, ":"); // Path ":" produces ":" as filename
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("malformed".to_string())); // "malformed" is the source
            assert_eq!(detailed.path, ":"); // ":" is the path
        } else {
            panic!("Expected detailed dependency");
        }
    }

    // Mock test for install_single_dependency - since we can't easily mock the Cache and Git operations,
    // we'll test the error cases and the MCP server special case
    #[tokio::test]
    async fn test_install_single_dependency_mcp_server() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "node",
            "args": ["server.js", "--port=3000"],
            "env": {
                "NODE_ENV": "production"
            }
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        // Change to temp directory

        // Load manifest and create dependency
        let manifest = Manifest::load(&manifest_path).unwrap();
        let dependency = ResourceDependency::Simple(mcp_file_path.to_string_lossy().to_string());

        let result = install_single_dependency(
            "test-mcp",
            &dependency,
            "mcp-server",
            &manifest,
            &manifest_path,
        )
        .await;

        // MCP servers should install both the file and update settings
        assert!(
            result.is_ok(),
            "MCP server installation should succeed: {result:?}"
        );

        // Check that the MCP server config was created
        let mcp_config_path = temp_dir
            .path()
            .join(".claude/ccpm/mcp-servers/test-mcp.json");
        assert!(
            mcp_config_path.exists(),
            "MCP server config file should be created"
        );

        // Check that settings.local.json was updated
        let settings_path = temp_dir.path().join(".claude/settings.local.json");
        assert!(settings_path.exists(), "Settings file should be created");

        let settings = crate::mcp::ClaudeSettings::load_or_default(&settings_path).unwrap();
        assert!(settings.mcp_servers.is_some());
        assert!(settings
            .mcp_servers
            .as_ref()
            .unwrap()
            .contains_key("test-mcp"));
    }

    #[tokio::test]
    async fn test_install_single_dependency_invalid_resource_type() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        let manifest = Manifest::load(&manifest_path).unwrap();

        // Create a test file so we get to the resource type check
        let test_file = temp_dir.path().join("test.md");
        std::fs::write(&test_file, "# Test content").unwrap();

        let dependency = ResourceDependency::Simple(test_file.to_string_lossy().to_string());

        let result = install_single_dependency(
            "test",
            &dependency,
            "invalid-type", // Invalid resource type
            &manifest,
            &manifest_path,
        )
        .await;

        // Should return error for unknown resource type
        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // Should contain error about unknown resource type
        assert!(error_msg.contains("Unknown resource type: invalid-type"));
    }

    #[tokio::test]
    async fn test_install_single_dependency_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        let manifest = Manifest::load(&manifest_path).unwrap();
        let dependency = ResourceDependency::Detailed(DetailedDependency {
            source: Some("nonexistent-source".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            command: None,
            branch: None,
            rev: None,
            args: None,
            target: None,
            filename: None,
        });

        let result = install_single_dependency(
            "test-agent",
            &dependency,
            "agent",
            &manifest,
            &manifest_path,
        )
        .await;

        // Should return error for source not found in manifest
        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("Source 'nonexistent-source' not found in manifest"));
    }

    #[tokio::test]
    async fn test_add_dependency_agent_with_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local agent file for testing
        let agent_file = temp_dir.path().join("new-agent.md");
        std::fs::write(&agent_file, "# New Agent\nReplacement agent.").unwrap();

        // Change to temp directory

        let dep_type = DependencyType::Agent(AgentDependency {
            common: DependencySpec {
                spec: agent_file.to_string_lossy().to_string(),
                name: Some("existing-agent".to_string()), // Same name as existing
                force: true,                              // Force overwrite
            },
        });

        // This should succeed with force flag and overwrite the existing agent
        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        // This should succeed since we're using force flag and a local file
        assert!(
            result.is_ok(),
            "Failed to add agent with force flag: {result:?}"
        );

        // Verify the agent was overwritten
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.agents.contains_key("existing-agent"));
    }

    #[tokio::test]
    async fn test_add_dependency_mcp_server_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "different-command different args".to_string(),
                name: Some("existing-mcp".to_string()), // Same name as existing
                force: false,                           // Don't force overwrite
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the mcp server already exists
        assert!(
            error_msg.contains("existing-mcp")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_snippet_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("new-snippet.md");
        std::fs::write(&snippet_file, "# New Snippet\nReplacement snippet.").unwrap();

        // Change to temp directory

        let dep_type = DependencyType::Snippet(SnippetDependency {
            common: DependencySpec {
                spec: snippet_file.to_string_lossy().to_string(),
                name: Some("existing-snippet".to_string()), // Same name as existing
                force: false,                               // Don't force overwrite
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the snippet already exists
        assert!(
            error_msg.contains("existing-snippet")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_command_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("new-command.md");
        std::fs::write(&command_file, "# New Command\nReplacement command.").unwrap();

        // Change to temp directory

        let dep_type = DependencyType::Command(CommandDependency {
            common: DependencySpec {
                spec: command_file.to_string_lossy().to_string(),
                name: Some("existing-command".to_string()), // Same name as existing
                force: false,                               // Don't force overwrite
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the command already exists
        assert!(
            error_msg.contains("existing-command")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_mcp_server_with_file() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory

        // Create a test MCP server JSON file
        let mcp_config = serde_json::json!({
            "command": "node",
            "args": ["server.js", "--port=3000"],
            "env": {
                "NODE_ENV": "production"
            }
        });
        let mcp_file_path = temp_dir.path().join("test-mcp.json");
        std::fs::write(&mcp_file_path, mcp_config.to_string()).unwrap();

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: mcp_file_path.to_string_lossy().to_string(),
                name: Some("file-mcp".to_string()),
                force: false,
            },
        });

        let result = add_dependency_with_manifest_path(dep_type, Some(manifest_path.clone())).await;

        assert!(
            result.is_ok(),
            "Failed to add MCP server with file: {result:?}"
        );

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("file-mcp"));

        // Check that the file was installed
        let installed_path = temp_dir
            .path()
            .join(".claude/ccpm/mcp-servers/file-mcp.json");
        assert!(
            installed_path.exists(),
            "MCP server config should be installed"
        );

        // Check that settings.local.json was updated
        let settings_path = temp_dir.path().join(".claude/settings.local.json");
        assert!(settings_path.exists(), "Settings file should be created");
    }
}
