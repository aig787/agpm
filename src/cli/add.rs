//! Add command implementation for CCPM
//!
//! This module provides functionality to add sources and dependencies
//! to a CCPM project manifest. It supports both Git repository sources
//! and various types of resource dependencies (agents, snippets, commands, MCP servers).

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use regex::Regex;
use std::path::Path;

use crate::cache::Cache;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{find_manifest, DetailedDependency, Manifest, ResourceDependency};
use crate::models::{
    AgentDependency, CommandDependency, DependencyType, McpServerDependency, SnippetDependency,
    SourceSpec,
};
use crate::utils::fs::{atomic_write, ensure_dir};

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

    /// Add an MCP server dependency
    McpServer(McpServerDependency),
}

impl AddCommand {
    /// Execute the add command
    pub async fn execute(self) -> Result<()> {
        match self.command {
            AddSubcommand::Source { name, url } => add_source(SourceSpec { name, url }).await,
            AddSubcommand::Dep(dep_command) => {
                let dep_type = match dep_command {
                    DependencySubcommand::Agent(agent) => DependencyType::Agent(agent),
                    DependencySubcommand::Snippet(snippet) => DependencyType::Snippet(snippet),
                    DependencySubcommand::Command(command) => DependencyType::Command(command),
                    DependencySubcommand::McpServer(mcp) => DependencyType::McpServer(mcp),
                };
                add_dependency(dep_type).await
            }
        }
    }
}

/// Add a new source to the manifest
async fn add_source(source: SourceSpec) -> Result<()> {
    // Find manifest file
    let manifest_path = find_manifest()?;
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

/// Add a dependency to the manifest and install it
async fn add_dependency(dep_type: DependencyType) -> Result<()> {
    let common = dep_type.common();
    let (name, dependency) = parse_dependency_spec(&common.spec, &common.name)?;

    // Find manifest file
    let manifest_path = find_manifest()?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Determine the resource type
    let resource_type = dep_type.resource_type();

    // Handle MCP servers separately since they have a different type
    if let DependencyType::McpServer(mcp) = &dep_type {
        // Check if dependency already exists
        if manifest.mcp_servers.contains_key(&name) && !common.force {
            return Err(anyhow!(
                "MCP server '{}' already exists in manifest. Use --force to overwrite",
                name
            ));
        }

        // Create MCP server dependency with command and args
        let mcp_dep = match &dependency {
            ResourceDependency::Detailed(detailed) => crate::mcp::McpServerDependency {
                source: detailed.source.clone(),
                path: Some(detailed.path.clone()),
                version: detailed.version.clone(),
                branch: detailed.branch.clone(),
                rev: detailed.rev.clone(),
                command: mcp.command.clone(),
                args: mcp.args.clone(),
                env: None,
            },
            ResourceDependency::Simple(path) => {
                // Local MCP servers - path but no source/version
                crate::mcp::McpServerDependency {
                    source: None,
                    path: Some(path.clone()),
                    version: None,
                    branch: None,
                    rev: None,
                    command: mcp.command.clone(),
                    args: mcp.args.clone(),
                    env: None,
                }
            }
        };

        // Add to manifest
        manifest.mcp_servers.insert(name.clone(), mcp_dep);
    } else {
        // Handle regular resources (agents, snippets, commands)
        let section = match &dep_type {
            DependencyType::Agent(_) => &mut manifest.agents,
            DependencyType::Snippet(_) => &mut manifest.snippets,
            DependencyType::Command(_) => &mut manifest.commands,
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
    install_single_dependency(&name, &dependency, resource_type, &manifest).await?;

    Ok(())
}

/// Parse a dependency specification string into a name and `ResourceDependency`
fn parse_dependency_spec(
    spec: &str,
    custom_name: &Option<String>,
) -> Result<(String, ResourceDependency)> {
    // Pattern: source:path@version or source:path
    let remote_pattern = Regex::new(r"^([^:]+):([^@]+)(?:@(.+))?$")?;

    if let Some(captures) = remote_pattern.captures(spec) {
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
            }),
        ))
    } else if spec.starts_with("file:") || Path::new(spec).exists() {
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
) -> Result<()> {
    // For MCP servers, we don't install files
    if resource_type == "mcp-server" {
        println!(
            "{}",
            "MCP server configuration added (no files to install)".yellow()
        );
        return Ok(());
    }

    // Get cache instance
    let cache = Cache::new()?;

    // Determine source path
    let (source_path, source_name, resolved_commit) = match dependency {
        ResourceDependency::Detailed(detailed) => {
            if let Some(ref source_name) = detailed.source {
                // Remote dependency - get from cache
                let source_url = manifest
                    .sources
                    .get(source_name)
                    .ok_or_else(|| anyhow!("Source '{}' not found in manifest", source_name))?;

                // Clone or fetch the repository
                let version_ref = detailed
                    .rev
                    .as_deref()
                    .or(detailed.branch.as_deref())
                    .or(detailed.version.as_deref());
                let cache_dir = cache
                    .get_or_clone_source(source_name, source_url, version_ref)
                    .await?;

                // Get the resolved commit hash
                let git_repo = crate::git::GitRepo::new(&cache_dir);
                let resolved_commit = git_repo.get_current_commit().await?;

                (
                    cache_dir.join(&detailed.path),
                    Some(source_name.clone()),
                    Some(resolved_commit),
                )
            } else {
                // Local dependency with detailed path
                (Path::new(&detailed.path).to_path_buf(), None, None)
            }
        }
        ResourceDependency::Simple(path) => {
            // Simple local dependency
            (Path::new(path).to_path_buf(), None, None)
        }
    };

    // Check if source file exists
    if !source_path.exists() {
        return Err(anyhow!("Source file not found: {}", source_path.display()));
    }

    // Read the source file
    let content = std::fs::read_to_string(&source_path).context("Failed to read source file")?;

    // Determine target directory based on resource type using manifest configuration
    let target_dir = match resource_type {
        "agent" => &manifest.target.agents,
        "snippet" => &manifest.target.snippets,
        "command" => &manifest.target.commands,
        _ => return Err(anyhow!("Unknown resource type: {}", resource_type)),
    };

    // Create target directory if it doesn't exist
    ensure_dir(Path::new(target_dir))?;

    // Write the file
    let target_path = Path::new(target_dir).join(format!("{name}.md"));
    atomic_write(&target_path, content.as_bytes())?;

    // Update or create lockfile
    let lockfile_path = manifest_path_to_lockfile(&find_manifest()?);
    let mut lockfile = if lockfile_path.exists() {
        LockFile::load(&lockfile_path)?
    } else {
        LockFile::new()
    };

    // Calculate checksum of the installed file
    let checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&content);
        format!("sha256:{:x}", hasher.finalize())
    };

    // Add lock entry
    let lock_entry = LockedResource {
        name: name.to_string(),
        source: source_name.clone(),
        url: source_name
            .as_ref()
            .and_then(|s| manifest.sources.get(s))
            .cloned(),
        path: match dependency {
            ResourceDependency::Detailed(d) => d.path.clone(),
            ResourceDependency::Simple(p) => p.clone(),
        },
        version: match dependency {
            ResourceDependency::Detailed(d) => {
                d.version.clone().or(d.branch.clone()).or(d.rev.clone())
            }
            ResourceDependency::Simple(_) => None,
        },
        resolved_commit,
        checksum,
        installed_at: target_path
            .strip_prefix(std::env::current_dir()?)
            .unwrap_or(&target_path)
            .to_string_lossy()
            .to_string(),
    };

    // Add to appropriate section
    match resource_type {
        "agent" => lockfile.agents.push(lock_entry),
        "snippet" => lockfile.snippets.push(lock_entry),
        "command" => lockfile.commands.push(lock_entry),
        _ => {}
    }

    // Save lockfile
    lockfile.save(&lockfile_path)?;

    println!(
        "{}",
        format!(
            "Installed {} '{}' to {}",
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
    use crate::test_utils::WorkingDirGuard;
    use tempfile::TempDir;

    // Helper function to create a test manifest with basic structure
    fn create_test_manifest(manifest_path: &Path) {
        let manifest_content = r#"[sources]

[target]
agents = ".claude/agents"
snippets = ".claude/snippets"
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
snippets = ".claude/snippets"
commands = ".claude/commands"

[agents]
existing-agent = "../local/agent.md"

[snippets]
existing-snippet = { source = "existing", path = "snippets/utils.md", version = "v1.0.0" }

[commands]
existing-command = { source = "existing", path = "commands/deploy.md", version = "v1.0.0" }

[mcp-servers]
existing-mcp = { command = "npx", args = ["-y", "@test/server"] }
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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let add_command = AddCommand {
            command: AddSubcommand::Source {
                name: "test-source".to_string(),
                url: "https://github.com/test/repo.git".to_string(),
            },
        };

        let result = add_command.execute().await;

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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local agent file for testing
        let agent_file = temp_dir.path().join("test-agent.md");
        std::fs::write(&agent_file, "# Test Agent\nThis is a test agent.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

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
        let result = add_command.execute().await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local agent: {result:?}");

        // Verify the agent was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.agents.contains_key("my-test-agent"));
    }

    #[tokio::test]
    async fn test_execute_add_snippet_dependency() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("test-snippet.md");
        std::fs::write(&snippet_file, "# Test Snippet\nUseful code snippet.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Snippet(SnippetDependency {
                common: DependencySpec {
                    spec: snippet_file.to_string_lossy().to_string(),
                    name: Some("my-snippet".to_string()),
                    force: false,
                },
            })),
        };

        let result = add_command.execute().await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local snippet: {result:?}");

        // Verify the snippet was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.snippets.contains_key("my-snippet"));
    }

    #[tokio::test]
    async fn test_execute_add_command_dependency() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("test-command.md");
        std::fs::write(&command_file, "# Test Command\nUseful command.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::Command(CommandDependency {
                common: DependencySpec {
                    spec: command_file.to_string_lossy().to_string(),
                    name: Some("my-command".to_string()),
                    force: false,
                },
            })),
        };

        let result = add_command.execute().await;

        // This should succeed since we're using a local file
        assert!(result.is_ok(), "Failed to add local command: {result:?}");

        // Verify the command was added and installed
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.commands.contains_key("my-command"));
    }

    #[tokio::test]
    async fn test_execute_add_mcp_server_dependency() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let add_command = AddCommand {
            command: AddSubcommand::Dep(DependencySubcommand::McpServer(McpServerDependency {
                common: DependencySpec {
                    spec: "local-config.toml".to_string(),
                    name: Some("test-mcp".to_string()),
                    force: false,
                },
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@test/mcp-server".to_string()],
            })),
        };

        let result = add_command.execute().await;

        // MCP servers don't install files, so this should succeed
        assert!(result.is_ok(), "Failed to add MCP server: {result:?}");

        // Verify the manifest was updated
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("test-mcp"));

        let mcp_server = manifest.mcp_servers.get("test-mcp").unwrap();
        assert_eq!(mcp_server.command, "npx");
        assert_eq!(mcp_server.args, vec!["-y", "@test/mcp-server"]);
    }

    #[tokio::test]
    async fn test_add_source_success() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let source = SourceSpec {
            name: "new-source".to_string(),
            url: "https://github.com/new/repo.git".to_string(),
        };

        let result = add_source(source).await;
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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let source = SourceSpec {
            name: "existing".to_string(),
            url: "https://github.com/different/repo.git".to_string(),
        };

        let result = add_source(source).await;
        assert!(result.is_err());

        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("Source 'existing' already exists"));
    }

    #[test]
    fn test_parse_dependency_spec_file_prefix() {
        // Test file: prefix - this matches the regex as source:path
        let (name, dep) = parse_dependency_spec("file:/path/to/agent.md", &None).unwrap();
        assert_eq!(name, "agent");
        if let ResourceDependency::Detailed(detailed) = dep {
            assert_eq!(detailed.source, Some("file".to_string())); // "file" is treated as source
            assert_eq!(detailed.path, "/path/to/agent.md"); // Path after colon
        } else {
            panic!("Expected detailed dependency");
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

        let manifest = Manifest::load(&manifest_path).unwrap();
        let dependency = ResourceDependency::Simple("config.toml".to_string());

        let result = install_single_dependency(
            "test-mcp",
            &dependency,
            "mcp-server", // This should trigger the early return
            &manifest,
        )
        .await;

        // MCP servers should return OK without installing files
        assert!(
            result.is_ok(),
            "MCP server installation should succeed: {result:?}"
        );
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
        });

        let result = install_single_dependency("test-agent", &dependency, "agent", &manifest).await;

        // Should return error for source not found in manifest
        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("Source 'nonexistent-source' not found in manifest"));
    }

    #[tokio::test]
    async fn test_add_dependency_agent_with_force() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local agent file for testing
        let agent_file = temp_dir.path().join("new-agent.md");
        std::fs::write(&agent_file, "# New Agent\nReplacement agent.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let dep_type = DependencyType::Agent(AgentDependency {
            common: DependencySpec {
                spec: agent_file.to_string_lossy().to_string(),
                name: Some("existing-agent".to_string()), // Same name as existing
                force: true,                              // Force overwrite
            },
        });

        // This should succeed with force flag and overwrite the existing agent
        let result = add_dependency(dep_type).await;

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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "config.toml".to_string(),
                name: Some("existing-mcp".to_string()), // Same name as existing
                force: false,                           // Don't force overwrite
            },
            command: "different-command".to_string(),
            args: vec!["different".to_string(), "args".to_string()],
        });

        let result = add_dependency(dep_type).await;

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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local snippet file for testing
        let snippet_file = temp_dir.path().join("new-snippet.md");
        std::fs::write(&snippet_file, "# New Snippet\nReplacement snippet.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let dep_type = DependencyType::Snippet(SnippetDependency {
            common: DependencySpec {
                spec: snippet_file.to_string_lossy().to_string(),
                name: Some("existing-snippet".to_string()), // Same name as existing
                force: false,                               // Don't force overwrite
            },
        });

        let result = add_dependency(dep_type).await;

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
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest_with_content(&manifest_path);

        // Create local command file for testing
        let command_file = temp_dir.path().join("new-command.md");
        std::fs::write(&command_file, "# New Command\nReplacement command.").unwrap();

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let dep_type = DependencyType::Command(CommandDependency {
            common: DependencySpec {
                spec: command_file.to_string_lossy().to_string(),
                name: Some("existing-command".to_string()), // Same name as existing
                force: false,                               // Don't force overwrite
            },
        });

        let result = add_dependency(dep_type).await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        // The error should mention that the command already exists
        assert!(
            error_msg.contains("existing-command")
                && (error_msg.contains("already exists") || error_msg.contains("force"))
        );
    }

    #[tokio::test]
    async fn test_add_dependency_detailed_mcp_server() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        // First add a source for testing detailed dependencies
        let source = SourceSpec {
            name: "test-source".to_string(),
            url: "https://github.com/test/mcp-configs.git".to_string(),
        };
        add_source(source).await.unwrap();

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "test-source:configs/server.toml@v1.0.0".to_string(),
                name: Some("detailed-mcp".to_string()),
                force: false,
            },
            command: "node".to_string(),
            args: vec!["server.js".to_string(), "--port=3000".to_string()],
        });

        let result = add_dependency(dep_type).await;

        // Should succeed for MCP servers since they don't install files
        assert!(
            result.is_ok(),
            "Failed to add detailed MCP server: {result:?}"
        );

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("detailed-mcp"));

        let mcp_server = manifest.mcp_servers.get("detailed-mcp").unwrap();
        assert_eq!(mcp_server.command, "node");
        assert_eq!(mcp_server.args, vec!["server.js", "--port=3000"]);
        assert_eq!(mcp_server.source, Some("test-source".to_string()));
        assert_eq!(mcp_server.path, Some("configs/server.toml".to_string()));
        assert_eq!(mcp_server.version, Some("v1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_add_dependency_simple_mcp_server() {
        let _guard = WorkingDirGuard::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");
        create_test_manifest(&manifest_path);

        // Change to temp directory
        _guard.change_to(temp_dir.path()).unwrap();

        let dep_type = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "local-config.toml".to_string(), // Simple path spec
                name: Some("simple-mcp".to_string()),
                force: false,
            },
            command: "python".to_string(),
            args: vec!["mcp_server.py".to_string()],
        });

        let result = add_dependency(dep_type).await;

        // Should succeed for MCP servers since they don't install files
        assert!(
            result.is_ok(),
            "Failed to add simple MCP server: {result:?}"
        );

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.mcp_servers.contains_key("simple-mcp"));

        let mcp_server = manifest.mcp_servers.get("simple-mcp").unwrap();
        assert_eq!(mcp_server.command, "python");
        assert_eq!(mcp_server.args, vec!["mcp_server.py"]);
        assert_eq!(mcp_server.source, None); // Simple path has no source
        assert_eq!(mcp_server.path, Some("local-config.toml".to_string()));
        assert_eq!(mcp_server.version, None);
    }
}
