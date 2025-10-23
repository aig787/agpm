//! Lockfile management for reproducible installations across environments.
//!
//! This module provides comprehensive lockfile functionality for AGPM, similar to Cargo's
//! `Cargo.lock` but designed specifically for managing Claude Code resources (agents,
//! snippets, and commands) from Git repositories. The lockfile ensures that all team members and CI/CD
//! systems install identical versions of dependencies.
//!
//! # Overview
//!
//! The lockfile (`agpm.lock`) is automatically generated from the manifest (`agpm.toml`)
//! during installation and contains exact resolved versions of all dependencies. Unlike
//! the manifest which specifies version constraints, the lockfile pins exact commit hashes
//! and file checksums for reproducibility.
//!
//! ## Key Concepts
//!
//! - **Version Resolution**: Converts version constraints to exact commits
//! - **Dependency Pinning**: Locks all transitive dependencies at specific versions
//! - **Reproducibility**: Guarantees identical installations across environments
//! - **Integrity Verification**: Uses SHA-256 checksums to detect file corruption
//! - **Atomic Operations**: All lockfile updates are atomic to prevent corruption
//!
//! # Lockfile Format Specification
//!
//! The lockfile uses TOML format with the following structure:
//!
//! ```toml
//! # Auto-generated lockfile - DO NOT EDIT
//! version = 1
//!
//! # Source repositories with resolved commits
//! [[sources]]
//! name = "community"                              # Source name from manifest
//! url = "https://github.com/example/repo.git"     # Repository URL
//! commit = "a1b2c3d4e5f6..."                      # Resolved commit hash (40 chars)
//! fetched_at = "2024-01-01T00:00:00Z"             # Last fetch timestamp (RFC 3339)
//!
//! # Agent resources
//! [[agents]]
//! name = "example-agent"                          # Resource name
//! source = "community"                            # Source name (optional for local)
//! url = "https://github.com/example/repo.git"     # Source URL (optional for local)
//! path = "agents/example.md"                      # Path in source repository
//! version = "v1.0.0"                              # Requested version constraint
//! resolved_commit = "a1b2c3d4e5f6..."             # Resolved commit for this resource
//! checksum = "sha256:abcdef123456..."             # SHA-256 checksum of installed file
//! installed_at = "agents/example-agent.md"        # Installation path (relative to project)
//!
//! # Snippet resources (same structure as agents)
//! [[snippets]]
//! name = "example-snippet"
//! source = "community"
//! path = "snippets/example.md"
//! version = "^1.0"
//! resolved_commit = "a1b2c3d4e5f6..."
//! checksum = "sha256:fedcba654321..."
//! installed_at = "snippets/example-snippet.md"
//!
//! # Command resources (same structure as agents)
//! [[commands]]
//! name = "build-command"
//! source = "community"
//! path = "commands/build.md"
//! version = "v1.0.0"
//! resolved_commit = "a1b2c3d4e5f6..."
//! checksum = "sha256:123456abcdef..."
//! installed_at = ".claude/commands/build-command.md"
//! ```
//!
//! ## Field Details
//!
//! ### Version Field
//! - **Type**: Integer
//! - **Purpose**: Lockfile format version for future compatibility
//! - **Current**: 1
//!
//! ### Sources Array
//! - **name**: Unique identifier for the source repository
//! - **url**: Full Git repository URL (HTTP/HTTPS/SSH)
//! - **commit**: 40-character SHA-1 commit hash at time of resolution
//! - **`fetched_at`**: ISO 8601 timestamp of last successful fetch
//!
//! ### Resources Arrays (agents/snippets/commands)
//! - **name**: Unique resource identifier within its type
//! - **source**: Source name (omitted for local resources)
//! - **url**: Repository URL (omitted for local resources)  
//! - **path**: Relative path within source repository or filesystem
//! - **version**: Original version constraint from manifest (omitted for local)
//! - **`resolved_commit`**: Exact commit containing this resource (omitted for local)
//! - **checksum**: SHA-256 hash prefixed with "sha256:" for integrity verification
//! - **`installed_at`**: Relative path where resource is installed in project
//!
//! # Relationship to Manifest
//!
//! The lockfile is generated from the manifest (`agpm.toml`) through dependency resolution:
//!
//! ```toml
//! # agpm.toml (manifest)
//! [sources]
//! community = "https://github.com/example/repo.git"
//!
//! [agents]
//! example-agent = { source = "community", path = "agents/example.md", version = "^1.0" }
//! local-agent = { path = "../local/helper.md" }
//! ```
//!
//! During `agpm install`, this becomes:
//!
//! ```toml
//! # agpm.lock (lockfile)
//! version = 1
//!
//! [[sources]]
//! name = "community"
//! url = "https://github.com/example/repo.git"
//! commit = "a1b2c3d4e5f6..."
//! fetched_at = "2024-01-01T00:00:00Z"
//!
//! [[agents]]
//! name = "example-agent"
//! source = "community"
//! url = "https://github.com/example/repo.git"
//! path = "agents/example.md"
//! version = "^1.0"
//! resolved_commit = "a1b2c3d4e5f6..."
//! checksum = "sha256:abcdef..."
//! installed_at = "agents/example-agent.md"
//!
//! [[agents]]
//! name = "local-agent"
//! path = "../local/helper.md"
//! checksum = "sha256:123abc..."
//! installed_at = "agents/local-agent.md"
//! ```
//!
//! # Version Resolution and Pinning
//!
//! AGPM resolves version constraints to exact commits using Git tags and branches:
//!
//! ## Version Constraint Resolution
//!
//! 1. **Exact versions** (`"v1.2.3"`): Match exact Git tag
//! 2. **Semantic ranges** (`"^1.0"`, `"~1.2"`): Find latest compatible tag
//! 3. **Branch names** (`"main"`, `"develop"`): Use latest commit on branch
//! 4. **Commit hashes** (`"a1b2c3d"`): Use exact commit (must be full 40-char hash)
//!
//! ## Resolution Process
//!
//! 1. **Fetch Repository**: Clone or update source repository cache
//! 2. **Enumerate Tags**: List all Git tags matching semantic version pattern
//! 3. **Apply Constraints**: Filter tags that satisfy version constraint
//! 4. **Select Latest**: Choose highest version within constraint
//! 5. **Resolve Commit**: Map tag to commit hash
//! 6. **Verify Resource**: Ensure resource exists at that commit
//! 7. **Calculate Checksum**: Generate SHA-256 hash of resource content
//! 8. **Record Entry**: Add resolved information to lockfile
//!
//! # Install vs Update Semantics
//!
//! ## Install Behavior
//! - Uses existing lockfile if present (respects pinned versions)
//! - Only resolves dependencies not in lockfile
//! - Preserves existing pins even if newer versions available
//! - Ensures reproducible installations
//!
//! ## Update Behavior  
//! - Ignores existing lockfile constraints
//! - Re-resolves all dependencies against current manifest constraints
//! - Updates to latest compatible versions within constraints
//! - Regenerates entire lockfile
//!
//! ```bash
//! # Install exact versions from lockfile (if available)
//! agpm install
//!
//! # Update to latest within manifest constraints
//! agpm update
//!
//! # Update specific resource
//! agpm update example-agent
//! ```
//!
//! # Checksum Verification
//!
//! AGPM uses SHA-256 checksums to ensure file integrity:
//!
//! ## Checksum Format
//! - **Algorithm**: SHA-256
//! - **Encoding**: Hexadecimal
//! - **Prefix**: "sha256:"
//! - **Example**: "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
//!
//! ## Verification Process
//! 1. **During Installation**: Calculate checksum of installed file
//! 2. **During Validation**: Compare stored checksum with file content
//! 3. **On Mismatch**: Report corruption and suggest re-installation
//!
//! # Best Practices
//!
//! ## Commit Lockfile to Version Control
//! The lockfile should always be committed to version control:
//!
//! ```bash
//! # Commit both manifest and lockfile together
//! git add agpm.toml agpm.lock
//! git commit -m "Add new agent dependency"
//! ```
//!
//! This ensures all team members get identical dependency versions.
//!
//! ## Don't Edit Lockfile Manually
//! The lockfile is auto-generated and should not be edited manually:
//! - Use `agpm install` to update lockfile from manifest changes
//! - Use `agpm update` to update dependency versions
//! - Delete lockfile and run `agpm install` to regenerate from scratch
//!
//! ## Lockfile Conflicts
//! During Git merges, lockfile conflicts may occur:
//!
//! ```bash
//! # Resolve by regenerating lockfile
//! rm agpm.lock
//! agpm install
//! git add agpm.lock
//! git commit -m "Resolve lockfile conflict"
//! ```
//!
//! # Migration and Upgrades
//!
//! ## Format Version Compatibility
//! AGPM checks lockfile format version and provides clear error messages:
//!
//! ```text
//! Error: Lockfile version 2 is newer than supported version 1.
//! This lockfile was created by a newer version of agpm.
//! Please update agpm to the latest version to use this lockfile.
//! ```
//!
//! ## Upgrading Lockfiles
//! Future format versions will include automatic migration:
//!
//! ```bash
//! # Future: Migrate lockfile to newer format
//! agpm install --migrate-lockfile
//! ```
//!
//! # Comparison with Cargo.lock
//!
//! AGPM's lockfile design is inspired by Cargo but adapted for Git-based resources:
//!
//! | Feature | Cargo.lock | agpm.lock |
//! |---------|------------|-----------|
//! | Format | TOML | TOML |
//! | Versioning | Semantic | Git tags/branches/commits |
//! | Integrity | Checksums | SHA-256 checksums |
//! | Sources | crates.io + git | Git repositories only |
//! | Resources | Crates | Agents + Snippets |
//! | Resolution | Dependency graph | Flat dependency list |
//!
//! # Error Handling
//!
//! The lockfile module provides detailed error messages with actionable suggestions:
//!
//! - **Parse Errors**: TOML syntax issues with fix suggestions
//! - **Version Errors**: Incompatible format versions with upgrade instructions  
//! - **IO Errors**: File system issues with permission/space guidance
//! - **Corruption**: Checksum mismatches with re-installation steps
//!
//! # Cross-Platform Considerations
//!
//! Lockfiles are fully cross-platform compatible:
//! - **Path Separators**: Always use forward slashes in lockfile paths
//! - **Line Endings**: Normalize to LF for consistent checksums
//! - **File Permissions**: Not stored in lockfile (Git handles this)
//! - **Case Sensitivity**: Preserve case from source repositories
//!
//! # Performance Characteristics
//!
//! - **Parsing**: O(n) where n is number of locked resources
//! - **Checksum Calculation**: O(m) where m is total file size
//! - **Lookups**: O(n) linear search (suitable for typical dependency counts)
//! - **Atomic Writes**: Single fsync per lockfile update
//!
//! # Thread Safety
//!
//! The [`LockFile`] struct is not thread-safe by itself, but the module provides
//! atomic operations for concurrent access:
//! - **File Locking**: Uses OS file locking during atomic writes
//! - **Process Safety**: Multiple agpm instances coordinate via lockfile
//! - **Concurrent Reads**: Safe to read lockfile from multiple threads

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Reasons why a lockfile might be considered stale.
///
/// This enum describes various conditions that indicate a lockfile is
/// out-of-sync with the manifest and needs to be regenerated to prevent
/// installation errors or inconsistencies.
///
/// # Display Format
///
/// Each variant implements `Display` to provide user-friendly error messages
/// that explain the problem and suggest solutions.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::lockfile::StalenessReason;
/// use agpm_cli::core::ResourceType;
///
/// let reason = StalenessReason::MissingDependency {
///     name: "my-agent".to_string(),
///     resource_type: ResourceType::Agent,
/// };
///
/// println!("{}", reason);
/// // Output: "Dependency 'my-agent' (agent) is in manifest but missing from lockfile"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessReason {
    /// A dependency is in the manifest but not in the lockfile.
    /// This indicates the lockfile is incomplete and needs regeneration.
    MissingDependency {
        /// Name of the missing dependency
        name: String,
        /// Type of resource (agent, snippet, etc.)
        resource_type: crate::core::ResourceType,
    },

    /// A dependency's version constraint has changed in the manifest.
    VersionChanged {
        /// Name of the dependency
        name: String,
        /// Type of resource (agent, snippet, etc.)
        resource_type: crate::core::ResourceType,
        /// Previous version from lockfile
        old_version: String,
        /// New version from manifest
        new_version: String,
    },

    /// A dependency's path has changed in the manifest.
    PathChanged {
        /// Name of the dependency
        name: String,
        /// Type of resource (agent, snippet, etc.)
        resource_type: crate::core::ResourceType,
        /// Previous path from lockfile
        old_path: String,
        /// New path from manifest
        new_path: String,
    },

    /// A source repository has a different URL in the manifest.
    /// This is a security concern as it could point to a different repository.
    SourceUrlChanged {
        /// Name of the source repository
        name: String,
        /// Previous URL from lockfile
        old_url: String,
        /// New URL from manifest
        new_url: String,
    },

    /// Multiple entries exist for the same dependency (lockfile corruption).
    DuplicateEntries {
        /// Name of the duplicated dependency
        name: String,
        /// Type of resource (agent, snippet, etc.)
        resource_type: crate::core::ResourceType,
        /// Number of duplicate entries found
        count: usize,
    },

    /// A dependency's tool field has changed in the manifest.
    ToolChanged {
        /// Name of the dependency
        name: String,
        /// Type of resource (agent, snippet, etc.)
        resource_type: crate::core::ResourceType,
        /// Previous tool from lockfile
        old_tool: String,
        /// New tool from manifest (with defaults applied)
        new_tool: String,
    },
}

impl std::fmt::Display for StalenessReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDependency {
                name,
                resource_type,
            } => {
                write!(
                    f,
                    "Dependency '{name}' ({resource_type}) is in manifest but missing from lockfile"
                )
            }
            Self::VersionChanged {
                name,
                resource_type,
                old_version,
                new_version,
            } => {
                write!(
                    f,
                    "Dependency '{name}' ({resource_type}) version changed from '{old_version}' to '{new_version}'"
                )
            }
            Self::PathChanged {
                name,
                resource_type,
                old_path,
                new_path,
            } => {
                write!(
                    f,
                    "Dependency '{name}' ({resource_type}) path changed from '{old_path}' to '{new_path}'"
                )
            }
            Self::SourceUrlChanged {
                name,
                old_url,
                new_url,
            } => {
                write!(f, "Source repository '{name}' URL changed from '{old_url}' to '{new_url}'")
            }
            Self::DuplicateEntries {
                name,
                resource_type,
                count,
            } => {
                write!(
                    f,
                    "Found {count} duplicate entries for dependency '{name}' ({resource_type})"
                )
            }
            Self::ToolChanged {
                name,
                resource_type,
                old_tool,
                new_tool,
            } => {
                write!(
                    f,
                    "Dependency '{name}' ({resource_type}) tool changed from '{old_tool}' to '{new_tool}'"
                )
            }
        }
    }
}

impl std::error::Error for StalenessReason {}

/// Unique identifier for a resource in the lockfile.
///
/// This struct ensures type-safe identification of lockfile entries by combining
/// the resource name, source, and tool. Resources are considered unique when they
/// have distinct combinations of these fields:
///
/// - Same name, different sources: Different repositories providing same-named resources
/// - Same name, different tools: Resources used by different tools (e.g., Claude Code vs OpenCode)
/// - Same name and source, different tools: Transitive dependencies inherited from different parent tools
///
/// # Examples
///
/// ```rust
/// use agpm_cli::lockfile::ResourceId;
/// use agpm_cli::core::ResourceType;
/// use serde_json::json;
///
/// // Local resource (no source)
/// let local = ResourceId::new("my-agent", None::<String>, Some("claude-code"), ResourceType::Agent, json!({}));
///
/// // Git resource from a source
/// let git = ResourceId::new("shared-agent", Some("community"), Some("claude-code"), ResourceType::Agent, json!({}));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId {
    /// The name of the resource
    name: String,
    /// The source repository name (None for local resources)
    source: Option<String>,
    /// The tool identifier (e.g., "claude-code", "opencode", "agpm")
    tool: Option<String>,
    /// The resource type (Agent, Snippet, Command, etc.)
    resource_type: crate::core::ResourceType,
    /// The complete merged template variable context (serialized as JSON string)
    ///
    /// This contains the full template context that was used during dependency
    /// resolution, including both global project config and resource-specific overrides.
    /// Stored as a JSON string to ensure consistent comparison with LockedResource.
    /// Two resources with different template_vars are considered distinct, even if
    /// they have the same name, source, and tool.
    template_vars: String,
}

impl ResourceId {
    /// Create a new ResourceId from template_vars JSON value
    pub fn new(
        name: impl Into<String>,
        source: Option<impl Into<String>>,
        tool: Option<impl Into<String>>,
        resource_type: crate::core::ResourceType,
        template_vars: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            source: source.map(|s| s.into()),
            tool: tool.map(|t| t.into()),
            resource_type,
            template_vars: serde_json::to_string(&template_vars).unwrap_or_default(),
        }
    }

    /// Create a new ResourceId from pre-serialized template_vars string
    pub fn from_serialized(
        name: impl Into<String>,
        source: Option<impl Into<String>>,
        tool: Option<impl Into<String>>,
        resource_type: crate::core::ResourceType,
        template_vars: String,
    ) -> Self {
        Self {
            name: name.into(),
            source: source.map(|s| s.into()),
            tool: tool.map(|t| t.into()),
            resource_type,
            template_vars,
        }
    }

    /// Create a ResourceId from a LockedResource
    pub fn from_resource(resource: &LockedResource) -> Self {
        Self {
            name: resource.name.clone(),
            source: resource.source.clone(),
            tool: resource.tool.clone(),
            resource_type: resource.resource_type,
            template_vars: resource.template_vars.clone(),
        }
    }

    /// Resource name accessor.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source repository name accessor.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Tool identifier accessor.
    #[must_use]
    pub fn tool(&self) -> Option<&str> {
        self.tool.as_deref()
    }

    /// Resource type accessor.
    #[must_use]
    pub fn resource_type(&self) -> crate::core::ResourceType {
        self.resource_type
    }

    /// Template variables accessor (returns parsed JSON value).
    /// Returns None if template_vars is an empty JSON object "{}".
    #[must_use]
    pub fn template_vars(&self) -> Option<serde_json::Value> {
        let parsed =
            serde_json::from_str::<serde_json::Value>(&self.template_vars).unwrap_or_default();
        if parsed == serde_json::Value::Object(serde_json::Map::new()) {
            None
        } else {
            Some(parsed)
        }
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref source) = self.source {
            write!(f, " (source: {})", source)?;
        }
        if let Some(ref tool) = self.tool {
            write!(f, " [{}]", tool)?;
        }
        // Show compact representation of template_vars
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(&self.template_vars)
        {
            if !map.is_empty() {
                write!(f, " <vars: {} keys>", map.len())?;
            }
        }
        Ok(())
    }
}

/// The main lockfile structure representing a complete `agpm.lock` file.
///
/// This structure contains all resolved dependencies, source repositories, and their
/// exact versions/commits for reproducible installations. The lockfile is automatically
/// generated from the [`crate::manifest::Manifest`] during installation and should not
/// be edited manually.
///
/// # Format Version
///
/// The lockfile includes a format version to enable future migrations and compatibility
/// checking. The current version is 1.
///
/// # Serialization
///
/// The lockfile serializes to TOML format with arrays of sources, agents, and snippets.
/// Empty arrays are omitted from serialization to keep the lockfile clean.
///
/// # Examples
///
/// Creating a new lockfile:
///
/// ```rust,no_run
/// use agpm_cli::lockfile::LockFile;
///
/// let lockfile = LockFile::new();
/// assert_eq!(lockfile.version, 1);
/// assert!(lockfile.sources.is_empty());
/// ```
///
/// Loading an existing lockfile:
///
/// ```rust,no_run
/// # use std::path::Path;
/// # use agpm_cli::lockfile::LockFile;
/// # fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// println!("Loaded {} sources, {} agents",
///          lockfile.sources.len(), lockfile.agents.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    /// Version of the lockfile format.
    ///
    /// This field enables forward and backward compatibility checking. AGPM will
    /// refuse to load lockfiles with versions newer than it supports, and may
    /// provide migration paths for older versions in the future.
    pub version: u32,

    /// Locked source repositories with their resolved commit hashes.
    ///
    /// Each entry represents a Git repository that has been fetched and resolved
    /// to an exact commit. The commit hash ensures all team members get identical
    /// source content even as the upstream repository evolves.
    ///
    /// This field is omitted from TOML serialization if empty to keep the lockfile clean.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<LockedSource>,

    /// Locked agent resources with their exact versions and checksums.
    ///
    /// Contains all resolved agent dependencies from the manifest, with exact
    /// commit hashes, installation paths, and SHA-256 checksums for integrity
    /// verification.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<LockedResource>,

    /// Locked snippet resources with their exact versions and checksums.
    ///
    /// Contains all resolved snippet dependencies from the manifest, with exact
    /// commit hashes, installation paths, and SHA-256 checksums for integrity
    /// verification.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<LockedResource>,

    /// Locked command resources with their exact versions and checksums.
    ///
    /// Contains all resolved command dependencies from the manifest, with exact
    /// commit hashes, installation paths, and SHA-256 checksums for integrity
    /// verification.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<LockedResource>,

    /// Locked MCP server resources with their exact versions and checksums.
    ///
    /// Contains all resolved MCP server dependencies from the manifest, with exact
    /// commit hashes, installation paths, and SHA-256 checksums for integrity
    /// verification. MCP servers are installed as JSON files and also configured
    /// in `.claude/settings.local.json`.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "mcp-servers")]
    pub mcp_servers: Vec<LockedResource>,

    /// Locked script resources with their exact versions and checksums.
    ///
    /// Contains all resolved script dependencies from the manifest, with exact
    /// commit hashes, installation paths, and SHA-256 checksums for integrity
    /// verification. Scripts are executable files that can be referenced by hooks.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<LockedResource>,

    /// Locked hook configurations with their exact versions and checksums.
    ///
    /// Contains all resolved hook dependencies from the manifest. Hooks are
    /// JSON configuration files that define event-based automation in Claude Code.
    ///
    /// This field is omitted from TOML serialization if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<LockedResource>,
}

/// A locked source repository with resolved commit information.
///
/// Represents a Git repository that has been fetched and resolved to an exact
/// commit hash. This ensures reproducible access to source repositories across
/// different environments and times.
///
/// # Fields
///
/// - **name**: Unique identifier used in the manifest to reference this source
/// - **url**: Full Git repository URL (HTTP/HTTPS/SSH)
/// - **commit**: 40-character SHA-1 commit hash resolved at time of lock
/// - **`fetched_at`**: RFC 3339 timestamp of when the repository was last fetched
///
/// # Examples
///
/// A typical locked source in TOML format:
///
/// ```toml
/// [[sources]]
/// name = "community"
/// url = "https://github.com/example/agpm-community.git"
/// commit = "a1b2c3d4e5f6789abcdef0123456789abcdef012"
/// fetched_at = "2024-01-15T10:30:00Z"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSource {
    /// Unique source name from the manifest.
    ///
    /// This corresponds to keys in the `[sources]` section of `agpm.toml`
    /// and is used to reference the source in resource definitions.
    pub name: String,

    /// Full Git repository URL.
    ///
    /// Supports HTTP, HTTPS, and SSH URLs. This is the exact URL used
    /// for cloning and fetching the repository.
    pub url: String,

    /// Timestamp of last successful fetch in RFC 3339 format.
    ///
    /// Records when the repository was last fetched from the remote.
    /// This helps track staleness and debugging fetch issues.
    pub fetched_at: String,
}

/// A locked resource (agent or snippet) with resolved version and integrity information.
///
/// Represents a specific resource file that has been resolved from either a source
/// repository or local filesystem. Contains all information needed to verify the
/// exact version and integrity of the installed resource.
///
/// # Local vs Remote Resources
///
/// Remote resources (from Git repositories) include:
/// - `source`: Source repository name
/// - `url`: Repository URL  
/// - `version`: Original version constraint
/// - `resolved_commit`: Exact commit containing the resource
///
/// Local resources (from filesystem) omit these fields since they don't
/// involve Git repositories.
///
/// # Integrity Verification
///
/// All resources include a SHA-256 checksum for integrity verification.
/// The checksum is calculated from the file content after installation
/// and can be used to detect corruption or tampering.
///
/// # Examples
///
/// Remote resource in TOML format:
///
/// ```toml
/// [[agents]]
/// name = "example-agent"
/// source = "community"
/// url = "https://github.com/example/repo.git"
/// path = "agents/example.md"
/// version = "^1.0"
/// resolved_commit = "a1b2c3d4e5f6..."
/// checksum = "sha256:abcdef123456..."
/// installed_at = "agents/example-agent.md"
/// ```
///
/// Local resource in TOML format:
///
/// ```toml
/// [[agents]]
/// name = "local-helper"
/// path = "../local/helper.md"
/// checksum = "sha256:fedcba654321..."
/// installed_at = "agents/local-helper.md"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedResource {
    /// Resource name from the manifest.
    ///
    /// This corresponds to keys in the `[agents]` or `[snippets]` sections
    /// of the manifest. Resources are uniquely identified by the combination
    /// of (name, source), allowing multiple sources to provide resources with
    /// the same name.
    pub name: String,

    /// Source repository name for remote resources.
    ///
    /// References a source defined in the `[sources]` section of the manifest.
    /// This field is `None` for local resources that don't come from Git repositories.
    ///
    /// Omitted from TOML serialization when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Source repository URL for remote resources.
    ///
    /// The full Git repository URL where this resource originates.
    /// This field is `None` for local resources.
    ///
    /// Omitted from TOML serialization when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Path to the resource file.
    ///
    /// For remote resources, this is the relative path within the source repository.
    /// For local resources, this is the filesystem path (may be relative or absolute).
    pub path: String,

    /// Resolved version for the resource.
    ///
    /// This stores the resolved version tag (e.g., "v1.0.0", "main") that was matched
    /// by the version constraint in `agpm.toml`. Like Cargo.lock, this provides
    /// human-readable context while `resolved_commit` ensures reproducibility.
    /// For local resources or resources without versions, this field is `None`.
    ///
    /// Omitted from TOML serialization when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Resolved Git commit hash for remote resources.
    ///
    /// The exact 40-character SHA-1 commit hash where this resource was found.
    /// This ensures reproducible installations even if the version constraint
    /// could match multiple commits. For local resources, this field is `None`.
    ///
    /// Omitted from TOML serialization when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,

    /// SHA-256 checksum of the installed file content.
    ///
    /// Used for integrity verification to detect file corruption or tampering.
    /// The format is "sha256:" followed by the hexadecimal hash.
    ///
    /// Example: "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
    pub checksum: String,

    /// Installation path relative to the project root.
    ///
    /// Where the resource file is installed within the project directory.
    /// This path is always relative to the project root and uses forward
    /// slashes as separators for cross-platform compatibility.
    ///
    /// Examples: "agents/example-agent.md", "snippets/util-snippet.md"
    pub installed_at: String,

    /// Dependencies of this resource.
    ///
    /// Lists the direct dependencies that this resource requires, including
    /// both manifest dependencies and transitive dependencies discovered from
    /// the resource file itself. Each dependency is identified by its resource
    /// type and name (e.g., "agents/helper-agent", "snippets/utils").
    ///
    /// This field enables dependency graph analysis and ensures all required
    /// resources are installed. It follows the same model as Cargo.lock where
    /// each package lists its dependencies.
    ///
    /// Always included in TOML serialization, even when empty, to match Cargo.lock format.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Resource type (agent, snippet, command, etc.)
    ///
    /// This field is populated during deserialization based on which TOML section
    /// the resource came from (`[[agents]]`, `[[snippets]]`, etc.) and is used internally
    /// for determining the correct lockfile section when adding/updating entries.
    ///
    /// It is never serialized to the lockfile - the section header provides this information.
    #[serde(skip)]
    pub resource_type: crate::core::ResourceType,

    /// Tool type for multi-tool support (claude-code, opencode, agpm, custom).
    ///
    /// Specifies which target AI coding assistant tool this resource is for. This determines
    /// where the resource is installed and how it's configured.
    ///
    /// When None during deserialization, will be set based on resource type's default
    /// (e.g., snippets default to "agpm", others to "claude-code").
    ///
    /// Always serialized for clarity and to avoid ambiguity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,

    /// Original manifest alias for pattern-expanded dependencies.
    ///
    /// When a pattern dependency (e.g., `agents/helpers/*.md` with alias "all-helpers")
    /// expands to multiple files, each file gets its own lockfile entry with a unique `name`
    /// (e.g., "helper-alpha", "helper-beta"). The `manifest_alias` field preserves the
    /// original pattern alias so patches defined under that alias can be correctly applied
    /// to all matched files.
    ///
    /// For non-pattern dependencies, this field is `None` since `name` already represents
    /// the manifest alias.
    ///
    /// Example lockfile entry for pattern-expanded resource:
    /// ```toml
    /// [[agents]]
    /// name = "helper-alpha"                    # Individual file name
    /// manifest_alias = "all-helpers"           # Original pattern alias
    /// path = "agents/helpers/helper-alpha.md"
    /// ...
    /// ```
    ///
    /// This enables pattern patching: all files matched by "all-helpers" pattern can
    /// have patches applied via `[patch.agents.all-helpers]` in the manifest.
    ///
    /// Omitted from TOML serialization when `None` (for non-pattern dependencies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_alias: Option<String>,

    /// Applied patches from manifest configuration.
    ///
    /// Contains the key-value pairs that were applied to this resource's metadata
    /// via `[patch.<resource-type>.<alias>]` sections in agpm.toml or agpm.private.toml.
    ///
    /// This enables reproducible installations and provides visibility into which
    /// resources have been patched.
    ///
    /// Omitted from TOML serialization when empty.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub applied_patches: HashMap<String, toml::Value>,

    /// Whether this dependency should be installed to disk.
    ///
    /// When `false`, the dependency is resolved, fetched, and tracked in the lockfile,
    /// but the file is not written to the project directory. Instead, its content is
    /// made available in template context via `agpm.deps.<type>.<name>.content`.
    ///
    /// This is useful for snippet embedding use cases where you want to include
    /// content inline rather than as a separate file.
    ///
    /// Defaults to `true` (install the file) for backwards compatibility.
    ///
    /// Omitted from TOML serialization when `None` or `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<bool>,

    /// Template variable overrides applied to this dependency.
    ///
    /// Stores the template variable overrides that were specified in the manifest
    /// for this dependency. These overrides are applied when rendering templates
    /// to allow customization of generic templates for specific use cases.
    ///
    /// The structure matches the template namespace hierarchy
    /// (e.g., `{ "project": { "language": "python" } }`).
    ///
    /// Always serialized, even when empty, for consistent lockfile format.
    /// Stored as JSON string to ensure inline serialization and consistent comparison.
    #[serde(default = "default_template_vars_string")]
    pub template_vars: String,
}

/// Builder for creating LockedResource instances.
///
/// This builder helps address clippy warnings about functions with too many arguments
/// by providing a fluent interface for constructing LockedResource instances.
pub struct LockedResourceBuilder {
    name: String,
    source: Option<String>,
    url: Option<String>,
    path: String,
    version: Option<String>,
    resolved_commit: Option<String>,
    checksum: String,
    installed_at: String,
    dependencies: Vec<String>,
    resource_type: crate::core::ResourceType,
    tool: Option<String>,
    manifest_alias: Option<String>,
    applied_patches: HashMap<String, toml::Value>,
    install: Option<bool>,
    template_vars: serde_json::Value,
}

impl LockedResourceBuilder {
    /// Create a new builder with the required fields.
    pub fn new(
        name: String,
        path: String,
        checksum: String,
        installed_at: String,
        resource_type: crate::core::ResourceType,
    ) -> Self {
        Self {
            name,
            source: None,
            url: None,
            path,
            version: None,
            resolved_commit: None,
            checksum,
            installed_at,
            dependencies: Vec::new(),
            resource_type,
            tool: None,
            manifest_alias: None,
            applied_patches: HashMap::new(),
            install: None,
            template_vars: serde_json::Value::Object(Default::default()),
        }
    }

    /// Set the source repository name.
    pub fn source(mut self, source: Option<String>) -> Self {
        self.source = source;
        self
    }

    /// Set the source repository URL.
    pub fn url(mut self, url: Option<String>) -> Self {
        self.url = url;
        self
    }

    /// Set the version.
    pub fn version(mut self, version: Option<String>) -> Self {
        self.version = version;
        self
    }

    /// Set the resolved commit.
    pub fn resolved_commit(mut self, resolved_commit: Option<String>) -> Self {
        self.resolved_commit = resolved_commit;
        self
    }

    /// Set the dependencies.
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    /// Set the tool.
    pub fn tool(mut self, tool: Option<String>) -> Self {
        self.tool = tool;
        self
    }

    /// Set the manifest alias.
    pub fn manifest_alias(mut self, manifest_alias: Option<String>) -> Self {
        self.manifest_alias = manifest_alias;
        self
    }

    /// Set the applied patches.
    pub fn applied_patches(mut self, applied_patches: HashMap<String, toml::Value>) -> Self {
        self.applied_patches = applied_patches;
        self
    }

    /// Set the install flag.
    pub fn install(mut self, install: Option<bool>) -> Self {
        self.install = install;
        self
    }

    /// Set the template variables.
    pub fn template_vars(mut self, template_vars: serde_json::Value) -> Self {
        self.template_vars = template_vars;
        self
    }

    /// Build the LockedResource, serializing template_vars to string format.
    pub fn build(self) -> LockedResource {
        LockedResource {
            name: self.name,
            source: self.source,
            url: self.url,
            path: self.path,
            version: self.version,
            resolved_commit: self.resolved_commit,
            checksum: self.checksum,
            installed_at: self.installed_at,
            dependencies: self.dependencies,
            resource_type: self.resource_type,
            tool: self.tool,
            manifest_alias: self.manifest_alias,
            applied_patches: self.applied_patches,
            install: self.install,
            template_vars: serde_json::to_string(&self.template_vars).unwrap_or_default(),
        }
    }

    /// Build the LockedResource from pre-serialized template_vars string.
    ///
    /// This method is useful when building from lockfile data where template_vars
    /// has already been serialized to string format.
    pub fn build_from_serialized(self, template_vars: String) -> LockedResource {
        LockedResource {
            name: self.name,
            source: self.source,
            url: self.url,
            path: self.path,
            version: self.version,
            resolved_commit: self.resolved_commit,
            checksum: self.checksum,
            installed_at: self.installed_at,
            dependencies: self.dependencies,
            resource_type: self.resource_type,
            tool: self.tool,
            manifest_alias: self.manifest_alias,
            applied_patches: self.applied_patches,
            install: self.install,
            template_vars,
        }
    }
}

impl LockedResource {
    /// Unique identifier combining name, source, tool, and template_vars.
    ///
    /// Canonical method for resource identification in checksum updates and lookups.
    #[must_use]
    pub fn id(&self) -> ResourceId {
        ResourceId {
            name: self.name.clone(),
            source: self.source.clone(),
            tool: self.tool.clone(),
            resource_type: self.resource_type,
            template_vars: self.template_vars.clone(),
        }
    }

    /// Check if resource matches ResourceId by comparing name, source, tool, and template_vars.
    ///
    /// Template_vars are part of identity - same resource with different template_vars
    /// produces different artifacts and must be tracked separately.
    #[must_use]
    pub fn matches_id(&self, id: &ResourceId) -> bool {
        self.name == id.name
            && self.source == id.source
            && self.tool == id.tool
            && self.template_vars == id.template_vars
    }

    /// Create a new LockedResource with template_vars serialization handled.
    ///
    /// This constructor handles the serialization of template_vars from serde_json::Value
    /// to the stored String format, ensuring consistency across lockfile entries.
    ///
    /// # Deprecated
    ///
    /// This method has too many parameters and triggers clippy warnings.
    /// Use `LockedResourceBuilder::new()` instead for a cleaner API.
    #[allow(deprecated)]
    #[deprecated(since = "0.5.0", note = "Use LockedResourceBuilder::new() instead")]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        source: Option<String>,
        url: Option<String>,
        path: String,
        version: Option<String>,
        resolved_commit: Option<String>,
        checksum: String,
        installed_at: String,
        dependencies: Vec<String>,
        resource_type: crate::core::ResourceType,
        tool: Option<String>,
        manifest_alias: Option<String>,
        applied_patches: HashMap<String, toml::Value>,
        install: Option<bool>,
        template_vars: serde_json::Value,
    ) -> Self {
        LockedResourceBuilder::new(name, path, checksum, installed_at, resource_type)
            .source(source)
            .url(url)
            .version(version)
            .resolved_commit(resolved_commit)
            .dependencies(dependencies)
            .tool(tool)
            .manifest_alias(manifest_alias)
            .applied_patches(applied_patches)
            .install(install)
            .template_vars(template_vars)
            .build()
    }

    /// Create a new LockedResource from pre-serialized template_vars string.
    ///
    /// This constructor accepts template_vars as an already-serialized string, useful
    /// when building from lockfile data or other sources where serialization has already occurred.
    ///
    /// # Deprecated
    ///
    /// This method has too many parameters and triggers clippy warnings.
    /// Use `LockedResourceBuilder::new()` with `build_from_serialized()` instead.
    #[allow(deprecated)]
    #[deprecated(
        since = "0.5.0",
        note = "Use LockedResourceBuilder::new() with build_from_serialized() instead"
    )]
    #[allow(clippy::too_many_arguments)]
    pub fn from_serialized(
        name: String,
        source: Option<String>,
        url: Option<String>,
        path: String,
        version: Option<String>,
        resolved_commit: Option<String>,
        checksum: String,
        installed_at: String,
        dependencies: Vec<String>,
        resource_type: crate::core::ResourceType,
        tool: Option<String>,
        manifest_alias: Option<String>,
        applied_patches: HashMap<String, toml::Value>,
        install: Option<bool>,
        template_vars: String,
    ) -> Self {
        LockedResourceBuilder::new(name, path, checksum, installed_at, resource_type)
            .source(source)
            .url(url)
            .version(version)
            .resolved_commit(resolved_commit)
            .dependencies(dependencies)
            .tool(tool)
            .manifest_alias(manifest_alias)
            .applied_patches(applied_patches)
            .install(install)
            .build_from_serialized(template_vars)
    }
}

// Submodules for organized implementation
mod checksum;
mod helpers;
mod io;
mod resource_ops;
mod validation;

// Public submodules
pub mod private_lock;
pub use private_lock::PrivateLockFile;

// Patch display utilities (currently unused - TODO: integrate with Cache API)
#[allow(dead_code)]
pub mod patch_display;

impl LockFile {
    /// Current lockfile format version.
    ///
    /// This constant defines the lockfile format version that this version of AGPM
    /// generates. It's used for compatibility checking when loading lockfiles that
    /// may have been created by different versions of AGPM.
    const CURRENT_VERSION: u32 = 1;

    /// Create a new empty lockfile with the current format version.
    ///
    /// Returns a fresh lockfile with no sources or resources. This is typically
    /// used when initializing a new project or regenerating a lockfile from scratch.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::lockfile::LockFile;
    ///
    /// let lockfile = LockFile::new();
    /// assert_eq!(lockfile.version, 1);
    /// assert!(lockfile.sources.is_empty());
    /// assert!(lockfile.agents.is_empty());
    /// assert!(lockfile.snippets.is_empty());
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            sources: Vec::new(),
            agents: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            mcp_servers: Vec::new(),
            scripts: Vec::new(),
            hooks: Vec::new(),
        }
    }
}

impl Default for LockFile {
    /// Equivalent to [`LockFile::new()`] - creates empty lockfile with current format version.
    fn default() -> Self {
        Self::new()
    }
}

/// Default value for `template_vars` field.
///
/// Returns an empty JSON object which will serialize as `"{}"` in TOML.
fn default_template_vars_string() -> String {
    "{}".to_string()
}

/// Find the lockfile in the current or parent directories.
///
/// Searches upward from the current working directory to find a `agpm.lock` file,
/// similar to how Git searches for `.git` directories. This enables running AGPM
/// commands from subdirectories within a project.
///
/// # Search Algorithm
///
/// 1. Start from current working directory
/// 2. Check for `agpm.lock` in current directory
/// 3. If found, return the path
/// 4. If not found, move to parent directory
/// 5. Repeat until root directory is reached
/// 6. Return `None` if no lockfile found
///
/// # Returns
///
/// * `Some(PathBuf)` - Path to the found lockfile
/// * `None` - No lockfile found in current or parent directories
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::lockfile::find_lockfile;
///
/// if let Some(lockfile_path) = find_lockfile() {
///     println!("Found lockfile: {}", lockfile_path.display());
/// } else {
///     println!("No lockfile found (run 'agpm install' to create one)");
/// }
/// ```
///
/// # Use Cases
///
/// - **CLI commands**: Find project root when run from subdirectories
/// - **Editor integration**: Locate project configuration
/// - **Build scripts**: Find lockfile for dependency information
/// - **Validation tools**: Check if project has lockfile
///
/// # Directory Structure Example
///
/// ```text
/// project/
/// ├── agpm.lock          # ← This will be found
/// ├── agpm.toml
/// └── src/
///     └── subdir/         # ← Commands run from here will find ../agpm.lock
/// ```
///
/// # Errors
///
/// This function does not return errors but rather `None` if:
/// - Cannot get current working directory (permission issues)
/// - No lockfile exists in the directory tree
/// - IO errors while checking file existence
///
/// For more robust error handling, consider using [`LockFile::load`] directly
/// with a known path.
#[must_use]
pub fn find_lockfile() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;

    loop {
        let lockfile_path = current.join("agpm.lock");
        if lockfile_path.exists() {
            return Some(lockfile_path);
        }

        if !current.pop() {
            return None;
        }
    }
}
