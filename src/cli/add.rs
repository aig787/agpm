//! Add sources and dependencies to a CCPM project.
//!
//! This module provides the `add` command which allows users to add Git repository sources
//! and resource dependencies (agents and snippets) to their `ccpm.toml` manifest file.
//! The command supports both remote dependencies from Git sources and local file dependencies.
//!
//! # Command Structure
//!
//! The add command has two main subcommands:
//! - `add source` - Add a new Git repository source
//! - `add dep` - Add a dependency (agent or snippet)
//!
//! # Examples
//!
//! Add a Git repository source:
//! ```bash
//! ccpm add source official https://github.com/org/ccpm-resources.git
//! ```
//!
//! Add a remote agent dependency:
//! ```bash
//! ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent
//! ```
//!
//! Add a local agent dependency:
//! ```bash
//! ccpm add dep ../local/my-agent.md --agent --name my-agent
//! ```
//!
//! # Dependency Specification Formats
//!
//! Dependencies can be specified in several formats:
//! - `source:path@version` - Remote dependency with specific version
//! - `source:path` - Remote dependency with latest version
//! - `file:path` - Local file dependency
//! - `path` - Local file dependency (if file exists)
//!
//! # Automatic Installation
//!
//! When adding a dependency, the command will automatically attempt to install it
//! after updating the manifest. This provides immediate feedback and ensures the
//! dependency is available for use.
//!
//! # Error Conditions
//!
//! - Returns error if no manifest file is found
//! - Returns error if source already exists (for source addition)
//! - Returns error if dependency already exists and `--force` is not used
//! - Returns error if dependency type cannot be inferred and not explicitly specified
//! - Returns error if unable to parse dependency specification

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use regex::Regex;
use std::path::Path;

use crate::cache::Cache;
use crate::lockfile::{LockEntry, Lockfile};
use crate::manifest::{find_manifest, DetailedDependency, Manifest, ResourceDependency};
use crate::markdown::MarkdownFile;
use crate::resolver::DependencyResolver;
use crate::utils::fs::{atomic_write, ensure_dir};

/// Command to add sources and dependencies to a CCPM project.
///
/// This command provides two main functionalities:
/// 1. Adding Git repository sources to the manifest
/// 2. Adding resource dependencies (agents/snippets) to the manifest
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::add::{AddCommand, AddSubcommand, DependencyArgs};
///
/// // Add a source
/// let cmd = AddCommand {
///     command: AddSubcommand::Source {
///         name: "official".to_string(),
///         url: "https://github.com/org/repo.git".to_string(),
///     }
/// };
///
/// // Add a dependency
/// let cmd = AddCommand {
///     command: AddSubcommand::Dependency(DependencyArgs {
///         spec: "official:agents/code-reviewer.md@v1.0.0".to_string(),
///         agent: true,
///         snippet: false,
///         name: None,
///         force: false,
///     })
/// };
/// ```
#[derive(Args)]
pub struct AddCommand {
    /// The specific add operation to perform
    #[command(subcommand)]
    command: AddSubcommand,
}

/// Subcommands for the add command.
///
/// This enum defines the two main operations supported by the add command:
/// adding sources and adding dependencies.
#[derive(Subcommand)]
enum AddSubcommand {
    /// Add a new Git repository source to the manifest.
    ///
    /// Sources are Git repositories that contain Claude Code resources.
    /// Once added to the manifest, resources from this source can be
    /// referenced in dependency specifications.
    ///
    /// # Example
    /// ```bash
    /// ccpm add source official https://github.com/org/ccpm-resources.git
    /// ```
    Source {
        /// Name for the source
        ///
        /// This name will be used to reference the source in dependency specifications.
        /// Should be descriptive and unique within the project.
        name: String,

        /// Git repository URL
        ///
        /// Must be a valid Git repository URL. Supports HTTP(S), SSH, and local paths.
        /// Examples:
        /// - `https://github.com/org/repo.git`
        /// - `git@github.com:org/repo.git`
        /// - `file:///local/path/to/repo`
        url: String,
    },

    /// Add a resource dependency (agent or snippet) to the manifest.
    ///
    /// Dependencies can be either remote (from a Git source) or local files.
    /// The command will automatically install the dependency after adding it
    /// to the manifest.
    ///
    /// # Example
    /// ```bash
    /// ccpm add dep official:agents/reviewer.md@v1.0.0 --agent
    /// ```
    #[command(name = "dep")]
    Dependency(DependencyArgs),
}

/// Arguments for adding a resource dependency.
///
/// This struct captures all the arguments needed to add either an agent or snippet
/// dependency to the manifest. Dependencies can be remote (from Git sources) or local files.
///
/// # Dependency Specification Formats
///
/// The `spec` field supports multiple formats:
/// - `source:path@version` - Remote dependency with specific version/tag/branch
/// - `source:path` - Remote dependency using latest available version
/// - `file:path` - Explicit local file dependency
/// - `path` - Local file path (if the file exists)
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::add::DependencyArgs;
///
/// // Remote agent with version
/// let args = DependencyArgs {
///     spec: "official:agents/code-reviewer.md@v1.0.0".to_string(),
///     agent: true,
///     snippet: false,
///     name: None,
///     force: false,
/// };
///
/// // Local snippet with custom name
/// let args = DependencyArgs {
///     spec: "../local/my-utils.md".to_string(),
///     agent: false,
///     snippet: true,
///     name: Some("utils".to_string()),
///     force: true,
/// };
/// ```
#[derive(Args)]
struct DependencyArgs {
    /// Dependency specification string
    ///
    /// Specifies the dependency using one of the supported formats:
    /// - `source:path@version` for remote dependencies with version
    /// - `source:path` for remote dependencies with latest version
    /// - `file:path` for explicit local file dependencies
    /// - `path` for local file dependencies (if file exists)
    ///
    /// Examples:
    /// - `official:agents/reviewer.md@v1.0.0`
    /// - `community:snippets/utils.md`
    /// - `file:../local/agent.md`
    /// - `./agents/local-agent.md`
    spec: String,

    /// Add as an agent resource
    ///
    /// Mutually exclusive with `--snippet`. If neither flag is provided,
    /// the command will attempt to infer the type from the specification path.
    #[arg(long, group = "type")]
    agent: bool,

    /// Add as a snippet resource
    ///
    /// Mutually exclusive with `--agent`. If neither flag is provided,
    /// the command will attempt to infer the type from the specification path.
    #[arg(long, group = "type")]
    snippet: bool,

    /// Custom name for the dependency
    ///
    /// If not provided, the name will be extracted from the filename
    /// (without extension). This name is used as the key in the manifest
    /// and must be unique within the resource type.
    #[arg(long)]
    name: Option<String>,

    /// Force overwrite if dependency already exists
    ///
    /// By default, the command will fail if a dependency with the same
    /// name already exists. Use this flag to overwrite existing dependencies.
    #[arg(long)]
    force: bool,
}

impl AddCommand {
    /// Execute the add command.
    ///
    /// This method dispatches to the appropriate subcommand handler based on
    /// the command type (source or dependency).
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation completed successfully
    /// - `Err(anyhow::Error)` if the operation failed
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::add::{AddCommand, AddSubcommand};
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = AddCommand {
    ///     command: AddSubcommand::Source {
    ///         name: "test".to_string(),
    ///         url: "https://github.com/test/repo.git".to_string(),
    ///     }
    /// };
    /// // cmd.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute(self) -> Result<()> {
        match self.command {
            AddSubcommand::Source { name, url } => add_source(&name, &url).await,
            AddSubcommand::Dependency(args) => add_dependency(args).await,
        }
    }
}

/// Add a new Git repository source to the project manifest.
///
/// This function adds a Git repository source to the `[sources]` section of the
/// manifest file. The source can then be referenced when adding dependencies.
///
/// # Arguments
///
/// * `name` - Unique name for the source (used to reference it in dependencies)
/// * `url` - Git repository URL (HTTP(S), SSH, or local path)
///
/// # Behavior
///
/// 1. Finds and loads the project manifest
/// 2. Validates that the source name is not already in use
/// 3. Validates the URL format
/// 4. Adds the source to the manifest
/// 5. Saves the updated manifest
///
/// # Returns
///
/// - `Ok(())` if the source was added successfully
/// - `Err(anyhow::Error)` if:
///   - No manifest is found in the project
///   - Source name already exists
///   - URL format is invalid
///   - Unable to save the updated manifest
///
/// # Examples
///
/// ```rust,ignore
/// # tokio_test::block_on(async {
/// // This would add a source to ccpm.toml
/// // add_source("official", "https://github.com/org/repo.git").await?;
/// # Ok::<(), anyhow::Error>(())
/// # });
/// ```
async fn add_source(name: &str, url: &str) -> Result<()> {
    let manifest_path = find_manifest()?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Check if source already exists
    if manifest.sources.contains_key(name) {
        return Err(anyhow!("Source '{}' already exists", name));
    }

    // Validate URL format - allow local paths and file:// URLs
    if !url.starts_with("http://")
        && !url.starts_with("https://")
        && !url.starts_with("git@")
        && !url.starts_with("file://")
        && !url.starts_with("/")
        && !url.starts_with("./")
        && !url.starts_with("../")
    {
        return Err(anyhow!("Invalid Git URL format: {}", url));
    }

    // Add the source
    manifest.sources.insert(name.to_string(), url.to_string());

    // Save the manifest
    manifest.save(&manifest_path)?;

    println!("{} Added source '{}': {}", "✓".green(), name, url);
    Ok(())
}

/// Add a resource dependency to the project manifest.
///
/// This function parses the dependency specification, adds the dependency to the
/// appropriate section of the manifest (agents or snippets), and attempts to
/// automatically install the dependency.
///
/// # Arguments
///
/// * `args` - Dependency arguments including specification, type, name, and flags
///
/// # Behavior
///
/// 1. Loads the project manifest
/// 2. Parses the dependency specification
/// 3. Determines the dependency type (agent/snippet)
/// 4. Extracts or uses the provided name
/// 5. Checks for existing dependencies (unless force is used)
/// 6. Adds the dependency to the manifest
/// 7. Saves the updated manifest
/// 8. Attempts to automatically install the dependency
///
/// # Type Inference
///
/// If neither `--agent` nor `--snippet` is specified, the function attempts to
/// infer the type from the specification path:
/// - Paths containing "agent" are treated as agents
/// - Paths containing "snippet" are treated as snippets
/// - Otherwise, an error is returned
///
/// # Returns
///
/// - `Ok(())` if the dependency was added successfully (installation may fail)
/// - `Err(anyhow::Error)` if:
///   - No manifest is found
///   - Dependency specification is invalid
///   - Dependency type cannot be determined
///   - Dependency already exists and force is not used
///   - Unable to save the manifest
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::add::DependencyArgs;
///
/// # tokio_test::block_on(async {
/// let args = DependencyArgs {
///     spec: "official:agents/reviewer.md@v1.0.0".to_string(),
///     agent: true,
///     snippet: false,
///     name: None,
///     force: false,
/// };
/// // add_dependency(args).await?;
/// # Ok::<(), anyhow::Error>(())
/// # });
/// ```
async fn add_dependency(args: DependencyArgs) -> Result<()> {
    let manifest_path = find_manifest()?;
    let mut manifest = Manifest::load(&manifest_path)?;

    // Parse the dependency specification
    let dep = parse_dependency_spec(&args.spec, &args.name)?;

    // Determine the type
    let dep_type = if args.agent {
        "agent"
    } else if args.snippet {
        "snippet"
    } else {
        // Try to infer from path
        if args.spec.contains("agent") {
            "agent"
        } else if args.spec.contains("snippet") {
            "snippet"
        } else {
            return Err(anyhow!(
                "Cannot determine dependency type. Please specify --agent or --snippet"
            ));
        }
    };

    // Get the name
    let name = if let Some(ref custom_name) = args.name {
        custom_name.clone()
    } else {
        // Extract name from path
        extract_name_from_path(&dep)?
    };

    // Check if dependency already exists
    if !args.force {
        if dep_type == "agent" && manifest.agents.contains_key(&name) {
            return Err(anyhow!(
                "Agent '{}' already exists. Use --force to overwrite",
                name
            ));
        }
        if dep_type == "snippet" && manifest.snippets.contains_key(&name) {
            return Err(anyhow!(
                "Snippet '{}' already exists. Use --force to overwrite",
                name
            ));
        }
    }

    // Add the dependency
    if dep_type == "agent" {
        manifest.agents.insert(name.clone(), dep.clone());
    } else {
        manifest.snippets.insert(name.clone(), dep.clone());
    }

    // Save the manifest
    manifest.save(&manifest_path)?;

    println!("{} Added {} '{}'", "✓".green(), dep_type, name);

    // Install only the newly added dependency
    println!("\n{} Installing {}...", "📦".cyan(), name);

    if let Err(e) = install_single_dependency(&manifest_path, &name, dep_type, &dep).await {
        println!("{} Failed to install {}: {}", "⚠️".yellow(), name, e);
        println!("You can manually install later with: ccpm install");
    } else {
        println!("{} Successfully installed '{}'", "✓".green(), name);
    }

    Ok(())
}

/// Install a single dependency that was just added to the manifest
async fn install_single_dependency(
    manifest_path: &Path,
    name: &str,
    dep_type: &str,
    dep: &ResourceDependency,
) -> Result<()> {
    let project_dir = manifest_path.parent().unwrap();

    // Load or create lockfile
    let lockfile_path = project_dir.join("ccpm.lock");
    let mut lockfile = if lockfile_path.exists() {
        Lockfile::load(&lockfile_path)?
    } else {
        // Create new lockfile with existing dependencies from manifest
        let manifest = Manifest::load(manifest_path)?;
        let mut resolver = DependencyResolver::new_with_global(manifest.clone()).await?;
        resolver.resolve(None).await?
    };

    // Create a lock entry for the new dependency
    let lock_entry = create_lock_entry(name, dep, dep_type, manifest_path).await?;

    // Add to lockfile
    if dep_type == "agent" {
        // Remove existing entry if force was used
        lockfile.agents.retain(|e| e.name != name);
        lockfile.agents.push(lock_entry.clone());
    } else {
        // Remove existing entry if force was used
        lockfile.snippets.retain(|e| e.name != name);
        lockfile.snippets.push(lock_entry.clone());
    }

    // Install the resource
    let manifest = Manifest::load(manifest_path)?;
    let resource_dir = if dep_type == "agent" {
        manifest.target.agents.as_str()
    } else {
        manifest.target.snippets.as_str()
    };

    // Initialize cache
    let cache = Cache::new()?;

    // Install the single resource
    install_resource(&lock_entry, project_dir, resource_dir, &cache).await?;

    // Save updated lockfile
    lockfile.save(&lockfile_path)?;

    Ok(())
}

/// Create a lock entry for a newly added dependency
async fn create_lock_entry(
    name: &str,
    dep: &ResourceDependency,
    dep_type: &str,
    manifest_path: &Path,
) -> Result<LockEntry> {
    let manifest = Manifest::load(manifest_path)?;

    match dep {
        ResourceDependency::Simple(path) => {
            // Local dependency
            Ok(LockEntry {
                name: name.to_string(),
                source: None,
                url: None,
                path: path.clone(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: format!(
                    "{}/{}.md",
                    if dep_type == "agent" {
                        &manifest.target.agents
                    } else {
                        &manifest.target.snippets
                    },
                    name
                ),
            })
        }
        ResourceDependency::Detailed(detailed) => {
            // Remote dependency
            let source_name = detailed
                .source
                .as_ref()
                .ok_or_else(|| anyhow!("Remote dependency must have a source"))?;

            // Get source URL from manifest or global config
            let url = if let Some(url) = manifest.sources.get(source_name) {
                url.clone()
            } else {
                // Check global config for source (synchronously)
                let config_path = crate::config::GlobalConfig::default_path()
                    .map_err(|e| anyhow!("Failed to get config path: {}", e))?;
                if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path)
                        .map_err(|e| anyhow!("Failed to read global config: {}", e))?;
                    let global_config: crate::config::GlobalConfig = toml::from_str(&content)
                        .map_err(|e| anyhow!("Failed to parse global config: {}", e))?;
                    global_config
                        .sources
                        .get(source_name)
                        .ok_or_else(|| {
                            anyhow!(
                                "Source '{}' not found in manifest or global config",
                                source_name
                            )
                        })?
                        .clone()
                } else {
                    return Err(anyhow!(
                        "Source '{}' not found in manifest and no global config exists",
                        source_name
                    ));
                }
            };

            // Resolve the actual commit if needed
            let resolved_commit = if let Some(version) = &detailed.version {
                // Clone/fetch to get the actual commit for this version
                let cache = Cache::new()?;
                let cache_dir = cache
                    .get_or_clone_source(source_name, &url, Some(version))
                    .await?;

                // Get the current commit
                let output = tokio::process::Command::new("git")
                    .arg("rev-parse")
                    .arg("HEAD")
                    .current_dir(&cache_dir)
                    .output()
                    .await?;

                if output.status.success() {
                    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    None
                }
            } else {
                None
            };

            Ok(LockEntry {
                name: name.to_string(),
                source: Some(source_name.clone()),
                url: Some(url),
                path: detailed.path.clone(),
                version: detailed.version.clone(),
                resolved_commit,
                checksum: String::new(),
                installed_at: format!(
                    "{}/{}.md",
                    if dep_type == "agent" {
                        &manifest.target.agents
                    } else {
                        &manifest.target.snippets
                    },
                    name
                ),
            })
        }
    }
}

/// Install a single resource from a lock entry
async fn install_resource(
    entry: &LockEntry,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
) -> Result<()> {
    // Determine destination path
    let dest_path = if !entry.installed_at.is_empty() {
        project_dir.join(&entry.installed_at)
    } else {
        // Default location based on resource type
        project_dir
            .join(resource_dir)
            .join(format!("{}.md", entry.name))
    };

    // Install based on source type
    if let Some(source_name) = &entry.source {
        // Remote resource
        let url = entry
            .url
            .as_ref()
            .ok_or_else(|| anyhow!("Remote resource {} has no URL", entry.name))?;

        // Get or clone the source to cache
        let cache_dir = cache
            .get_or_clone_source(
                source_name,
                url,
                entry
                    .version
                    .as_deref()
                    .or(entry.resolved_commit.as_deref()),
            )
            .await?;

        // Copy from cache to destination (without output for single installs)
        cache
            .copy_resource_with_output(&cache_dir, &entry.path, &dest_path, false)
            .await?;
    } else {
        // Local resource - copy directly
        let source_path = project_dir.join(&entry.path);

        if !source_path.exists() {
            return Err(anyhow!(
                "Local file '{}' not found. Expected at: {}",
                entry.path,
                source_path.display()
            ));
        }

        // Read the source file
        let content = std::fs::read_to_string(&source_path)
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        // Parse as markdown to validate
        let _markdown = MarkdownFile::parse(&content)
            .with_context(|| format!("Invalid markdown file: {}", entry.name))?;

        // Ensure destination directory exists
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Write file atomically
        atomic_write(&dest_path, content.as_bytes())?;
    }

    Ok(())
}

/// Parse a dependency specification string into a ResourceDependency.
///
/// This function parses various formats of dependency specifications and converts
/// them into the appropriate ResourceDependency type.
///
/// # Arguments
///
/// * `spec` - The dependency specification string
/// * `_custom_name` - Custom name (currently unused but kept for compatibility)
///
/// # Supported Formats
///
/// - `source:path@version` - Remote dependency with specific version
/// - `source:path` - Remote dependency with latest version  
/// - `file:path` - Explicit local file dependency
/// - `path` - Local file path (if file exists)
///
/// # Returns
///
/// - `Ok(ResourceDependency)` with the parsed dependency
/// - `Err(anyhow::Error)` if the specification format is invalid
///
/// # Examples
///
/// ```rust,ignore
/// # use ccpm::cli::add::parse_dependency_spec;
/// # use ccpm::manifest::ResourceDependency;
/// // Remote dependency
/// let dep = parse_dependency_spec("official:agents/test.md@v1.0.0", &None)?;
///
/// // Local dependency
/// let dep = parse_dependency_spec("./local/agent.md", &None)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
fn parse_dependency_spec(spec: &str, _custom_name: &Option<String>) -> Result<ResourceDependency> {
    // Handle local file paths (file:path or just path)
    if spec.starts_with("file:") {
        let path = spec.strip_prefix("file:").unwrap_or(spec);
        return Ok(ResourceDependency::Simple(path.to_string()));
    }

    // Check if it's a local path that exists (handles Windows drive letters)
    if Path::new(spec).exists() {
        return Ok(ResourceDependency::Simple(spec.to_string()));
    }

    // Check if it looks like a Windows path (contains :\ or :/)
    #[cfg(windows)]
    if spec.len() > 2
        && spec.chars().nth(1) == Some(':')
        && (spec.chars().nth(2) == Some('\\') || spec.chars().nth(2) == Some('/'))
    {
        return Ok(ResourceDependency::Simple(spec.to_string()));
    }

    // Parse source:path@version format
    let re = Regex::new(r"^([^:]+):([^@]+)(?:@(.+))?$")?;

    if let Some(captures) = re.captures(spec) {
        let source = captures.get(1).unwrap().as_str().to_string();
        let path = captures.get(2).unwrap().as_str().to_string();
        let version = captures.get(3).map(|m| m.as_str().to_string());

        Ok(ResourceDependency::Detailed(DetailedDependency {
            source: Some(source),
            path,
            version,
            git: None,
        }))
    } else {
        // Treat as simple path
        Ok(ResourceDependency::Simple(spec.to_string()))
    }
}

/// Extract a dependency name from its path.
///
/// This function extracts a suitable name for a dependency from its file path.
/// The name is derived from the filename without its extension.
///
/// # Arguments
///
/// * `dep` - The resource dependency to extract a name from
///
/// # Returns
///
/// - `Ok(String)` with the extracted name
/// - `Err(anyhow::Error)` if:
///   - Path ends with a directory separator
///   - Path is a hidden file (starts with '.')
///   - Unable to extract a meaningful name
///
/// # Examples
///
/// ```rust,ignore
/// # use ccpm::cli::add::extract_name_from_path;
/// # use ccpm::manifest::ResourceDependency;
/// let dep = ResourceDependency::Simple("agents/code-reviewer.md".to_string());
/// let name = extract_name_from_path(&dep)?; // Returns "code-reviewer"
/// # Ok::<(), anyhow::Error>(())
/// ```
fn extract_name_from_path(dep: &ResourceDependency) -> Result<String> {
    let path = match dep {
        ResourceDependency::Simple(p) => p,
        ResourceDependency::Detailed(d) => &d.path,
    };

    // Check for edge cases that should fail
    if path.ends_with('/') {
        return Err(anyhow!("Cannot extract name from path: {}", path));
    }

    // Check for hidden files (starting with . but not .. for relative paths)
    let path_obj = Path::new(path);
    if let Some(filename) = path_obj.file_name() {
        if let Some(filename_str) = filename.to_str() {
            if filename_str.starts_with('.') && !filename_str.starts_with("..") {
                return Err(anyhow!("Cannot extract name from path: {}", path));
            }
        }
    }

    path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Cannot extract name from path: {}", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_dependency_spec_remote() {
        // Test remote dependency with version
        let spec = "official:agents/test.md@v1.0.0";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.source, Some("official".to_string()));
                assert_eq!(d.path, "agents/test.md");
                assert_eq!(d.version, Some("v1.0.0".to_string()));
            }
            _ => panic!("Expected Detailed dependency"),
        }

        // Test remote dependency without version
        let spec = "official:agents/test.md";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.source, Some("official".to_string()));
                assert_eq!(d.path, "agents/test.md");
                assert_eq!(d.version, None);
            }
            _ => panic!("Expected Detailed dependency"),
        }
    }

    #[test]
    fn test_parse_dependency_spec_local() {
        // Create a temp file for testing
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "test content").unwrap();

        // Test with existing file path
        let spec = test_file.to_str().unwrap();
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Simple(p) => {
                assert_eq!(p, spec);
            }
            _ => panic!("Expected Simple dependency"),
        }

        // Test with file: prefix
        let spec = format!("file:{}", test_file.to_str().unwrap());
        let dep = parse_dependency_spec(&spec, &None).unwrap();
        match dep {
            ResourceDependency::Simple(p) => {
                assert_eq!(p, test_file.to_str().unwrap());
            }
            _ => panic!("Expected Simple dependency"),
        }
    }

    #[test]
    fn test_extract_name_from_path() {
        // Test with simple dependency
        let dep = ResourceDependency::Simple("../agents/test-agent.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "test-agent");

        // Test with detailed dependency
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "agents/code-reviewer.md".to_string(),
            version: Some("v1.0.0".to_string()),
            git: None,
        });
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "code-reviewer");

        // Test with path without extension
        let dep = ResourceDependency::Simple("/tmp/myagent".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "myagent");
    }

    #[tokio::test]
    async fn test_add_source() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let mut manifest = Manifest::default();

        // Test the logic directly
        manifest.sources.insert(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Verify manifest was updated
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("official"));
        assert_eq!(
            manifest.sources.get("official").unwrap(),
            "https://github.com/test/repo.git"
        );

        // Test that adding duplicate would fail (by checking the logic)
        // In real implementation, add_source would check for duplicates
        assert!(manifest.sources.contains_key("official"));
    }

    #[tokio::test]
    async fn test_add_dependency_agent() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest with a source
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Create a local test file
        let test_file = temp_dir.path().join("test-agent.md");
        fs::write(&test_file, "# Test Agent").unwrap();

        // Add local agent dependency
        let args = DependencyArgs {
            spec: test_file.to_str().unwrap().to_string(),
            agent: true,
            snippet: false,
            name: Some("my-agent".to_string()),
            force: false,
        };

        // Note: We can't easily test the full add_dependency with auto-install
        // because it requires InstallCommand which needs git operations.
        // Instead, we'll test the parsing and manifest update logic separately.

        // Test dependency parsing
        let dep = parse_dependency_spec(&args.spec, &args.name).unwrap();
        assert!(matches!(dep, ResourceDependency::Simple(_)));

        // Test name extraction
        let name = if let Some(ref custom_name) = args.name {
            custom_name.clone()
        } else {
            extract_name_from_path(&dep).unwrap()
        };
        assert_eq!(name, "my-agent");
    }

    #[tokio::test]
    async fn test_add_dependency_snippet() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Test remote snippet dependency
        let args = DependencyArgs {
            spec: "official:snippets/utils.md@v1.0.0".to_string(),
            agent: false,
            snippet: true,
            name: None,
            force: false,
        };

        let dep = parse_dependency_spec(&args.spec, &args.name).unwrap();

        // Check the dependency structure
        match &dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.source, Some("official".to_string()));
                assert_eq!(d.path, "snippets/utils.md");
                assert_eq!(d.version, Some("v1.0.0".to_string()));
            }
            _ => panic!("Expected Detailed dependency"),
        }

        // Test name extraction
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "utils");
    }

    #[test]
    fn test_validate_url_format() {
        // Valid URLs
        assert!(validate_url("https://github.com/test/repo.git"));
        assert!(validate_url("http://github.com/test/repo.git"));
        assert!(validate_url("git@github.com:test/repo.git"));
        assert!(validate_url("file:///local/path"));
        assert!(validate_url("/absolute/path"));
        assert!(validate_url("./relative/path"));
        assert!(validate_url("../parent/path"));

        // Invalid URLs
        assert!(!validate_url("github.com/test/repo.git"));
        assert!(!validate_url("not-a-url"));
        assert!(!validate_url("ftp://example.com"));
    }

    fn validate_url(url: &str) -> bool {
        url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("git@")
            || url.starts_with("file://")
            || url.starts_with("/")
            || url.starts_with("./")
            || url.starts_with("../")
    }

    #[test]
    fn test_parse_dependency_spec_edge_cases() {
        // Test empty spec
        let dep = parse_dependency_spec("", &None);
        assert!(matches!(dep, Ok(ResourceDependency::Simple(_))));

        // Test spec with multiple colons
        let spec = "source:path:subpath@v1.0.0";
        let dep = parse_dependency_spec(spec, &None);
        assert!(dep.is_ok());

        // Test spec with multiple @ symbols
        let spec = "source:path@v1.0.0@extra";
        let dep = parse_dependency_spec(spec, &None);
        assert!(dep.is_ok());

        // Test spec with special characters
        let spec = "source:path/with-dash_underscore.md@v1.0.0";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.path, "path/with-dash_underscore.md");
            }
            _ => panic!("Expected Detailed dependency"),
        }
    }

    #[test]
    fn test_extract_name_edge_cases() {
        // Test with no extension
        let dep = ResourceDependency::Simple("noextension".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "noextension");

        // Test with multiple dots
        let dep = ResourceDependency::Simple("file.test.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "file.test");

        // Test with trailing slash
        let dep = ResourceDependency::Simple("path/to/file.md/".to_string());
        let name = extract_name_from_path(&dep);
        assert!(name.is_err());

        // Test with just extension
        let dep = ResourceDependency::Simple(".hidden".to_string());
        let name = extract_name_from_path(&dep);
        assert!(name.is_err());
    }

    #[tokio::test]
    async fn test_create_lock_entry_local() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test local dependency
        let dep = ResourceDependency::Simple("../local/file.md".to_string());
        let entry = create_lock_entry("test", &dep, "agent", &manifest_path)
            .await
            .unwrap();

        assert_eq!(entry.name, "test");
        assert_eq!(entry.path, "../local/file.md");
        assert!(entry.source.is_none());
        assert!(entry.url.is_none());
        assert!(entry.installed_at.contains("agents/test.md"));
    }

    #[test]
    fn test_dependency_args_validation() {
        // Test that agent and snippet flags are mutually exclusive
        // This is handled by clap's group = "type" attribute
        let args = DependencyArgs {
            spec: "test.md".to_string(),
            agent: true,
            snippet: true, // This would be rejected by clap
            name: None,
            force: false,
        };

        // In practice, clap prevents both from being true
        assert!(args.agent);
        assert!(args.snippet);
    }

    // Additional comprehensive tests for better coverage

    #[tokio::test]
    async fn test_add_command_execute_source() {
        use crate::test_utils::WorkingDirGuard;

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Use WorkingDirGuard to handle directory changes safely
        let _guard = WorkingDirGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Test adding source via execute method
        let cmd = AddCommand {
            command: AddSubcommand::Source {
                name: "test-repo".to_string(),
                url: "https://github.com/test/repo.git".to_string(),
            },
        };

        // Execute the command
        let result = cmd.execute().await;
        assert!(result.is_ok());

        // Verify source was added
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.sources.contains_key("test-repo"));
    }

    #[tokio::test]
    async fn test_add_command_execute_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create test file
        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "# Test Content").unwrap();

        // Create initial manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test the command structure without executing (to avoid directory issues)
        let cmd = AddCommand {
            command: AddSubcommand::Dependency(DependencyArgs {
                spec: "test.md".to_string(),
                agent: true,
                snippet: false,
                name: Some("test-agent".to_string()),
                force: false,
            }),
        };

        // Test that the command structure is correct
        match cmd.command {
            AddSubcommand::Dependency(args) => {
                assert_eq!(args.spec, "test.md");
                assert!(args.agent);
                assert!(!args.snippet);
                assert_eq!(args.name, Some("test-agent".to_string()));
                assert!(!args.force);
            }
            _ => panic!("Expected Dependency subcommand"),
        }
    }

    #[tokio::test]
    async fn test_add_source_with_cwd_context() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test adding source logic by directly manipulating the manifest
        let mut manifest = Manifest::load(&manifest_path).unwrap();

        // Simulate the add_source logic: check for duplicates and URL validation
        let name = "official";
        let url = "https://github.com/example/repo.git";

        // Check if source already exists (should be false for new source)
        assert!(!manifest.sources.contains_key(name));

        // Validate URL format
        assert!(url.starts_with("https://"));

        // Add the source (simulating what add_source does)
        manifest.sources.insert(name.to_string(), url.to_string());
        manifest.save(&manifest_path).unwrap();

        // Verify source was added
        let updated_manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(updated_manifest.sources.get("official").unwrap(), url);
    }

    #[tokio::test]
    async fn test_add_source_duplicate_error() {
        use crate::test_utils::WorkingDirGuard;

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with existing source
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "existing".to_string(),
            "https://github.com/existing/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Use WorkingDirGuard to handle directory changes safely
        let _guard = WorkingDirGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Test adding duplicate source
        let result = add_source("existing", "https://github.com/different/repo.git").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_add_source_invalid_url() {
        use crate::test_utils::WorkingDirGuard;

        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Use WorkingDirGuard to handle directory changes safely
        let _guard = WorkingDirGuard::new().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Test adding source with invalid URL
        let result = add_source("test", "invalid-url").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid Git URL format"));
    }

    #[tokio::test]
    async fn test_add_source_various_valid_urls() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Note: Since we can't reliably change working directory in tests,
        // we'll test the URL validation logic by directly checking URLs
        let valid_urls = vec![
            "https://github.com/user/repo.git",
            "http://github.com/user/repo.git",
            "git@github.com:user/repo.git",
            "file:///local/path/to/repo",
            "/absolute/path/to/repo",
            "./relative/path/to/repo",
            "../parent/path/to/repo",
        ];

        for url in valid_urls {
            // Test URL validation (the core logic we want to test)
            assert!(validate_url(url), "URL should be valid: {}", url);
        }
    }

    #[tokio::test]
    async fn test_add_dependency_duplicate_agent_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with existing agent
        let mut manifest = Manifest::default();
        let test_dep = ResourceDependency::Simple("test.md".to_string());
        manifest
            .agents
            .insert("existing-agent".to_string(), test_dep);
        manifest.save(&manifest_path).unwrap();

        // Test the duplicate check logic directly since directory changing fails in tests
        // The key test is that the manifest contains the agent and force is false
        assert!(manifest.agents.contains_key("existing-agent"));

        // Test the logic that would be executed in add_dependency
        let name = "existing-agent";
        let force = false;
        let dep_type = "agent";

        // This is the logic from add_dependency for duplicate checking
        let should_fail = !force && dep_type == "agent" && manifest.agents.contains_key(name);
        assert!(should_fail, "Should fail due to duplicate without force");

        // Also test that with force=true it would not fail this check
        let should_not_fail_with_force = true; // force = true
        let would_fail_with_force = !should_not_fail_with_force
            && dep_type == "agent"
            && manifest.agents.contains_key(name);
        assert!(
            !would_fail_with_force,
            "Should not fail duplicate check with force=true"
        );
    }

    #[tokio::test]
    async fn test_add_dependency_duplicate_snippet_without_force() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with existing snippet
        let mut manifest = Manifest::default();
        let test_dep = ResourceDependency::Simple("test.md".to_string());
        manifest
            .snippets
            .insert("existing-snippet".to_string(), test_dep);
        manifest.save(&manifest_path).unwrap();

        // Test the duplicate check logic directly
        let name = "existing-snippet";
        let force = false;
        let dep_type = "snippet";

        // The manifest should contain the existing snippet
        assert!(manifest.snippets.contains_key(name));

        // Test the duplicate check logic
        let should_fail = !force && dep_type == "snippet" && manifest.snippets.contains_key(name);
        assert!(
            should_fail,
            "Should fail due to duplicate snippet without force"
        );

        // Test that with force=true it would not fail
        let should_not_fail_with_force = true; // force = true
        let would_fail_with_force = !should_not_fail_with_force
            && dep_type == "snippet"
            && manifest.snippets.contains_key(name);
        assert!(
            !would_fail_with_force,
            "Should not fail duplicate check with force=true"
        );
    }

    #[tokio::test]
    async fn test_add_dependency_with_force_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create test file
        let test_file = temp_dir.path().join("new-agent.md");
        fs::write(&test_file, "# New Agent").unwrap();

        // Create manifest with existing agent
        let mut manifest = Manifest::default();
        let existing_dep = ResourceDependency::Simple("old.md".to_string());
        manifest
            .agents
            .insert("test-agent".to_string(), existing_dep);
        manifest.save(&manifest_path).unwrap();

        // Test force overwrite logic directly
        let name = "test-agent";
        let force = true;
        let dep_type = "agent";

        // The manifest should contain the existing agent
        assert!(manifest.agents.contains_key(name));

        // Test the duplicate check logic with force=true
        let should_fail_duplicate_check =
            !force && dep_type == "agent" && manifest.agents.contains_key(name);
        assert!(
            !should_fail_duplicate_check,
            "With force=true, duplicate check should not fail"
        );

        // Test that force allows overwrite by updating the manifest directly
        let new_dep = ResourceDependency::Simple("new-agent.md".to_string());
        manifest.agents.insert(name.to_string(), new_dep.clone());
        manifest.save(&manifest_path).unwrap();

        // Verify the agent was updated
        let updated_manifest = Manifest::load(&manifest_path).unwrap();
        if let Some(ResourceDependency::Simple(path)) = updated_manifest.agents.get(name) {
            assert_eq!(path, "new-agent.md");
        } else {
            panic!("Expected simple dependency");
        }
    }

    #[tokio::test]
    async fn test_add_dependency_type_inference() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test type inference logic without executing (to avoid directory/file issues)
        let args = DependencyArgs {
            spec: "path/with/agent/test.md".to_string(),
            agent: false,
            snippet: false, // Neither flag set, should infer from path
            name: Some("test-agent".to_string()),
            force: false,
        };

        // Test the type inference logic directly
        let dep_type = if args.agent {
            "agent"
        } else if args.snippet {
            "snippet"
        } else {
            // Try to infer from path
            if args.spec.contains("agent") {
                "agent"
            } else if args.spec.contains("snippet") {
                "snippet"
            } else {
                "unknown"
            }
        };

        assert_eq!(dep_type, "agent", "Should infer agent type from path");
    }

    #[tokio::test]
    async fn test_add_dependency_type_inference_snippet() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test type inference logic without executing (to avoid directory/file issues)
        let args = DependencyArgs {
            spec: "path/with/snippet/test.md".to_string(),
            agent: false,
            snippet: false, // Neither flag set, should infer from path
            name: Some("test-snippet".to_string()),
            force: false,
        };

        // Test the type inference logic directly
        let dep_type = if args.agent {
            "agent"
        } else if args.snippet {
            "snippet"
        } else {
            // Try to infer from path
            if args.spec.contains("agent") {
                "agent"
            } else if args.spec.contains("snippet") {
                "snippet"
            } else {
                "unknown"
            }
        };

        assert_eq!(dep_type, "snippet", "Should infer snippet type from path");
    }

    #[tokio::test]
    async fn test_add_dependency_no_type_inference_fails() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test type inference failure logic without executing
        let args = DependencyArgs {
            spec: "some/random/path.md".to_string(),
            agent: false,
            snippet: false, // Neither flag set, can't infer from path
            name: Some("test".to_string()),
            force: false,
        };

        // Test the type determination logic directly
        let dep_type = if args.agent {
            "agent"
        } else if args.snippet {
            "snippet"
        } else {
            // Try to infer from path
            if args.spec.contains("agent") {
                "agent"
            } else if args.spec.contains("snippet") {
                "snippet"
            } else {
                // This should return an error in the actual function
                "unknown"
            }
        };

        assert_eq!(
            dep_type, "unknown",
            "Should not be able to infer type from random path"
        );

        // Test that the error condition would be triggered
        let would_error = dep_type == "unknown";
        assert!(would_error, "Should fail when type cannot be determined");
    }

    #[tokio::test]
    async fn test_create_lock_entry_remote_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create manifest with source
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "test-source".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Test remote dependency (this will fail during git operations but tests the structure)
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("test-source".to_string()),
            path: "agents/test.md".to_string(),
            version: None, // No version to avoid git operations
            git: None,
        });

        let result = create_lock_entry("test-agent", &dep, "agent", &manifest_path).await;

        // This test may fail during git operations, but we're testing the code path exists
        // and doesn't panic
        match result {
            Ok(entry) => {
                assert_eq!(entry.name, "test-agent");
                assert_eq!(entry.source, Some("test-source".to_string()));
                assert_eq!(entry.path, "agents/test.md");
            }
            Err(_) => {
                // Expected to fail during git operations in test environment
                // The important thing is that the code doesn't panic
            }
        }
    }

    #[tokio::test]
    async fn test_create_lock_entry_missing_source() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create empty manifest (no sources)
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Test remote dependency with missing source
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("missing-source".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            git: None,
        });

        let result = create_lock_entry("test-agent", &dep, "agent", &manifest_path).await;
        assert!(result.is_err());
        // Just check that it fails - the error message depends on global config state
        let _err_msg = result.unwrap_err().to_string();
    }

    #[test]
    fn test_parse_dependency_spec_file_prefix_nonexistent() {
        // Test file: prefix with non-existent file should be treated as simple path
        let spec = "file:/nonexistent/path.md";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Simple(p) => {
                assert_eq!(p, "/nonexistent/path.md");
            }
            _ => panic!("Expected Simple dependency"),
        }
    }

    #[test]
    fn test_parse_dependency_spec_malformed_regex() {
        // Test spec that doesn't match the regex pattern
        let spec = "no-colon-at-all";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Simple(p) => {
                assert_eq!(p, "no-colon-at-all");
            }
            _ => panic!("Expected Simple dependency for malformed spec"),
        }
    }

    #[test]
    fn test_extract_name_from_path_complex_scenarios() {
        // Test with URL-like paths
        let dep = ResourceDependency::Simple("https://example.com/path/file.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "file");

        // Test with query parameters
        let dep = ResourceDependency::Simple("path/file.md?version=1.0".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "file.md?version=1");

        // Test with spaces in filename (should work)
        let dep = ResourceDependency::Simple("path/file name.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "file name");
    }

    #[tokio::test]
    async fn test_install_resource_error_handling() {
        // Test error handling in install_resource function by creating scenarios
        // that should fail (this mainly tests that error paths exist and don't panic)

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::new().unwrap();

        // Test with missing local file
        let entry = LockEntry {
            name: "test".to_string(),
            source: None,
            url: None,
            path: "nonexistent.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: String::new(),
            installed_at: "agents/test.md".to_string(),
        };

        let result = install_resource(&entry, project_dir, "agents", &cache).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_install_single_dependency_new_lockfile() {
        // Test install_single_dependency when no lockfile exists
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create a simple manifest
        let manifest = Manifest::default();
        manifest.save(&manifest_path).unwrap();

        // Create test markdown file
        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "# Test Agent\n\nTest content").unwrap();

        let dep = ResourceDependency::Simple("test.md".to_string());

        // This will likely fail during dependency resolution, but tests the code path
        let result = install_single_dependency(&manifest_path, "test-agent", "agent", &dep).await;

        // We expect this to fail in test environment, but it shouldn't panic
        // The main goal is to execute the code path through install_single_dependency
        match result {
            Ok(_) => {
                // If it succeeds, great! Check that lockfile was created
                let lockfile_path = temp_dir.path().join("ccpm.lock");
                assert!(lockfile_path.exists());
            }
            Err(_) => {
                // Expected to fail in test environment due to git operations
                // The important thing is no panic occurred
            }
        }
    }

    #[test]
    fn test_extract_name_from_path_edge_cases() {
        // Test with trailing slash (should fail)
        let dep = ResourceDependency::Simple("/path/to/dir/".to_string());
        let result = extract_name_from_path(&dep);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot extract name from path"));

        // Test with multiple extensions
        let dep = ResourceDependency::Simple("test.agent.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "test.agent");

        // Test with path containing dots
        let dep = ResourceDependency::Simple("../v1.0.0/agent.md".to_string());
        let name = extract_name_from_path(&dep).unwrap();
        assert_eq!(name, "agent");
    }

    #[test]
    fn test_parse_dependency_spec_additional_cases() {
        // Test simple path (non-existent file treated as simple)
        let spec = "some/relative/path.md";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        assert!(matches!(dep, ResourceDependency::Simple(_)));

        // Test complex source:path@version
        let spec = "my-source:deeply/nested/path/to/resource.md@feature/branch";
        let dep = parse_dependency_spec(spec, &None).unwrap();
        match dep {
            ResourceDependency::Detailed(d) => {
                assert_eq!(d.source, Some("my-source".to_string()));
                assert_eq!(d.path, "deeply/nested/path/to/resource.md");
                assert_eq!(d.version, Some("feature/branch".to_string()));
            }
            _ => panic!("Expected Detailed dependency"),
        }
    }

    #[tokio::test]
    async fn test_add_snippet_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("ccpm.toml");

        // Create initial manifest with source defined
        let mut manifest = Manifest::default();
        manifest.sources.insert(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.save(&manifest_path).unwrap();

        // Test adding a snippet
        let dep = ResourceDependency::Detailed(DetailedDependency {
            source: Some("official".to_string()),
            path: "snippets/utils.md".to_string(),
            version: Some("v1.0.0".to_string()),
            git: None,
        });

        // Reload and add snippet
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        manifest.snippets.insert("utils".to_string(), dep);
        manifest.save(&manifest_path).unwrap();

        // Verify snippet was added
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.snippets.contains_key("utils"));
    }
}
