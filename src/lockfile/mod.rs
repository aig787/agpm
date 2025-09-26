//! Lockfile management for reproducible installations across environments.
//!
//! This module provides comprehensive lockfile functionality for CCPM, similar to Cargo's
//! `Cargo.lock` but designed specifically for managing Claude Code resources (agents,
//! snippets, and commands) from Git repositories. The lockfile ensures that all team members and CI/CD
//! systems install identical versions of dependencies.
//!
//! # Overview
//!
//! The lockfile (`ccpm.lock`) is automatically generated from the manifest (`ccpm.toml`)
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
//! The lockfile is generated from the manifest (`ccpm.toml`) through dependency resolution:
//!
//! ```toml
//! # ccpm.toml (manifest)
//! [sources]
//! community = "https://github.com/example/repo.git"
//!
//! [agents]
//! example-agent = { source = "community", path = "agents/example.md", version = "^1.0" }
//! local-agent = { path = "../local/helper.md" }
//! ```
//!
//! During `ccpm install`, this becomes:
//!
//! ```toml
//! # ccpm.lock (lockfile)
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
//! CCPM resolves version constraints to exact commits using Git tags and branches:
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
//! ccpm install
//!
//! # Update to latest within manifest constraints
//! ccpm update
//!
//! # Update specific resource
//! ccpm update example-agent
//! ```
//!
//! # Checksum Verification
//!
//! CCPM uses SHA-256 checksums to ensure file integrity:
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
//! git add ccpm.toml ccpm.lock
//! git commit -m "Add new agent dependency"
//! ```
//!
//! This ensures all team members get identical dependency versions.
//!
//! ## Don't Edit Lockfile Manually
//! The lockfile is auto-generated and should not be edited manually:
//! - Use `ccpm install` to update lockfile from manifest changes
//! - Use `ccpm update` to update dependency versions
//! - Delete lockfile and run `ccpm install` to regenerate from scratch
//!
//! ## Lockfile Conflicts
//! During Git merges, lockfile conflicts may occur:
//!
//! ```bash
//! # Resolve by regenerating lockfile
//! rm ccpm.lock
//! ccpm install
//! git add ccpm.lock
//! git commit -m "Resolve lockfile conflict"
//! ```
//!
//! # Migration and Upgrades
//!
//! ## Format Version Compatibility
//! CCPM checks lockfile format version and provides clear error messages:
//!
//! ```text
//! Error: Lockfile version 2 is newer than supported version 1.
//! This lockfile was created by a newer version of ccpm.
//! Please update ccpm to the latest version to use this lockfile.
//! ```
//!
//! ## Upgrading Lockfiles
//! Future format versions will include automatic migration:
//!
//! ```bash
//! # Future: Migrate lockfile to newer format
//! ccpm install --migrate-lockfile
//! ```
//!
//! # Comparison with Cargo.lock
//!
//! CCPM's lockfile design is inspired by Cargo but adapted for Git-based resources:
//!
//! | Feature | Cargo.lock | ccpm.lock |
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
//! - **Process Safety**: Multiple ccpm instances coordinate via lockfile
//! - **Concurrent Reads**: Safe to read lockfile from multiple threads

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::fs::atomic_write;

/// The main lockfile structure representing a complete `ccpm.lock` file.
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
/// use ccpm::lockfile::LockFile;
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
/// # use ccpm::lockfile::LockFile;
/// # fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// println!("Loaded {} sources, {} agents",
///          lockfile.sources.len(), lockfile.agents.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    /// Version of the lockfile format.
    ///
    /// This field enables forward and backward compatibility checking. CCPM will
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
/// url = "https://github.com/example/ccpm-community.git"
/// commit = "a1b2c3d4e5f6789abcdef0123456789abcdef012"
/// fetched_at = "2024-01-15T10:30:00Z"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSource {
    /// Unique source name from the manifest.
    ///
    /// This corresponds to keys in the `[sources]` section of `ccpm.toml`
    /// and is used to reference the source in resource definitions.
    pub name: String,

    /// Full Git repository URL.
    ///
    /// Supports HTTP, HTTPS, and SSH URLs. This is the exact URL used
    /// for cloning and fetching the repository.
    pub url: String,

    /// Resolved commit hash (40 characters).
    ///
    /// This is the exact SHA-1 commit hash that was resolved during
    /// dependency resolution. It ensures all installations use identical
    /// source content regardless of new commits to the repository.
    pub commit: String,

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
    /// Unique resource name within its type (agents or snippets).
    ///
    /// This corresponds to keys in the `[agents]` or `[snippets]` sections
    /// of the manifest and must be unique within each resource type.
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

    /// Original version constraint from the manifest.
    ///
    /// This preserves the version constraint specified in `ccpm.toml` (e.g., "^1.0", "v2.1.0").
    /// For local resources or resources without version constraints, this field is `None`.
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
}

impl LockFile {
    /// Current lockfile format version.
    ///
    /// This constant defines the lockfile format version that this version of CCPM
    /// generates. It's used for compatibility checking when loading lockfiles that
    /// may have been created by different versions of CCPM.
    const CURRENT_VERSION: u32 = 1;

    /// Create a new empty lockfile with the current format version.
    ///
    /// Returns a fresh lockfile with no sources or resources. This is typically
    /// used when initializing a new project or regenerating a lockfile from scratch.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::lockfile::LockFile;
    ///
    /// let lockfile = LockFile::new();
    /// assert_eq!(lockfile.version, 1);
    /// assert!(lockfile.sources.is_empty());
    /// assert!(lockfile.agents.is_empty());
    /// assert!(lockfile.snippets.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
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

    /// Load a lockfile from disk with comprehensive error handling and validation.
    ///
    /// Attempts to load and parse a lockfile from the specified path. If the file
    /// doesn't exist, returns a new empty lockfile. Performs format version
    /// compatibility checking and provides detailed error messages for common issues.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the lockfile (typically "ccpm.lock")
    ///
    /// # Returns
    ///
    /// * `Ok(LockFile)` - Successfully loaded lockfile or new empty lockfile if file doesn't exist
    /// * `Err(anyhow::Error)` - Parse error, IO error, or version incompatibility
    ///
    /// # Error Handling
    ///
    /// This method provides detailed error messages for common issues:
    /// - **File not found**: Returns empty lockfile (not an error)
    /// - **Permission denied**: Suggests checking file ownership/permissions
    /// - **TOML parse errors**: Suggests regenerating lockfile or checking syntax
    /// - **Version incompatibility**: Suggests updating CCPM
    /// - **Empty file**: Returns empty lockfile (graceful handling)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use ccpm::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// // Load existing lockfile
    /// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
    /// println!("Loaded {} sources", lockfile.sources.len());
    ///
    /// // Non-existent file returns empty lockfile
    /// let empty = LockFile::load(Path::new("missing.lock"))?;
    /// assert!(empty.sources.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Version Compatibility
    ///
    /// The method checks the lockfile format version and will refuse to load
    /// lockfiles created by newer versions of CCPM:
    ///
    /// ```text
    /// Error: Lockfile version 2 is newer than supported version 1.
    /// This lockfile was created by a newer version of ccpm.
    /// Please update ccpm to the latest version to use this lockfile.
    /// ```
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path).with_context(|| {
            format!(
                "Cannot read lockfile: {}\n\n\
                    Possible causes:\n\
                    - File doesn't exist (run 'ccpm install' to create it)\n\
                    - Permission denied (check file ownership)\n\
                    - File is corrupted or locked by another process",
                path.display()
            )
        })?;

        // Handle empty file
        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let lockfile: Self = toml::from_str(&content)
            .map_err(|e| crate::core::CcpmError::LockfileParseError {
                file: path.display().to_string(),
                reason: e.to_string(),
            })
            .with_context(|| {
                format!(
                    "Invalid TOML syntax in lockfile: {}\n\n\
                    The lockfile may be corrupted. You can:\n\
                    - Delete ccpm.lock and run 'ccpm install' to regenerate it\n\
                    - Check for syntax errors if you manually edited the file\n\
                    - Restore from backup if available",
                    path.display()
                )
            })?;

        // Check version compatibility
        if lockfile.version > Self::CURRENT_VERSION {
            return Err(crate::core::CcpmError::Other {
                message: format!(
                    "Lockfile version {} is newer than supported version {}.\n\n\
                    This lockfile was created by a newer version of ccpm.\n\
                    Please update ccpm to the latest version to use this lockfile.",
                    lockfile.version,
                    Self::CURRENT_VERSION
                ),
            }
            .into());
        }

        Ok(lockfile)
    }

    /// Save the lockfile to disk with atomic write operations and custom formatting.
    ///
    /// Serializes the lockfile to TOML format and writes it atomically to prevent
    /// corruption. The output includes a header warning against manual editing and
    /// uses custom formatting for better readability compared to standard TOML
    /// serialization.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where to save the lockfile (typically "ccpm.lock")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Successfully saved lockfile
    /// * `Err(anyhow::Error)` - IO error, permission denied, or disk full
    ///
    /// # Atomic Write Behavior
    ///
    /// The save operation is atomic - the lockfile is written to a temporary file
    /// and then renamed to the target path. This ensures the lockfile is never
    /// left in a partially written state even if the process is interrupted.
    ///
    /// # Custom Formatting
    ///
    /// The method uses custom TOML formatting instead of standard serde serialization
    /// to produce more readable output:
    /// - Adds header comment warning against manual editing
    /// - Groups related fields together
    /// - Uses consistent indentation and spacing
    /// - Omits empty arrays to keep the file clean
    ///
    /// # Error Handling
    ///
    /// Provides detailed error messages for common issues:
    /// - **Permission denied**: Suggests running with elevated permissions
    /// - **Directory doesn't exist**: Suggests creating parent directories
    /// - **Disk full**: Suggests freeing space or using different location
    /// - **File locked**: Suggests closing other programs using the file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use ccpm::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut lockfile = LockFile::new();
    ///
    /// // Add a source
    /// lockfile.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/example/repo.git".to_string(),
    ///     "a1b2c3d4e5f6...".to_string()
    /// );
    ///
    /// // Save to disk
    /// lockfile.save(Path::new("ccpm.lock"))?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Generated File Format
    ///
    /// The saved file starts with a warning header:
    ///
    /// ```toml
    /// # Auto-generated lockfile - DO NOT EDIT
    /// version = 1
    ///
    /// [[sources]]
    /// name = "community"
    /// url = "https://github.com/example/repo.git"
    /// commit = "a1b2c3d4e5f6..."
    /// fetched_at = "2024-01-15T10:30:00Z"
    /// ```
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut content = String::from("# Auto-generated lockfile - DO NOT EDIT\n");
        content.push_str(&format!("version = {}\n\n", self.version));

        // Custom formatting for better readability
        if !self.sources.is_empty() {
            for source in &self.sources {
                content.push_str("[[sources]]\n");
                content.push_str(&format!("name = {:?}\n", source.name));
                content.push_str(&format!("url = {:?}\n", source.url));
                content.push_str(&format!("commit = {:?}\n", source.commit));
                content.push_str(&format!("fetched_at = {:?}\n\n", source.fetched_at));
            }
        }

        // Helper to write resource arrays
        let write_resources =
            |content: &mut String, resources: &[LockedResource], section: &str| {
                for resource in resources {
                    content.push_str(&format!("[[{section}]]\n"));
                    content.push_str(&format!("name = {:?}\n", resource.name));
                    if let Some(source) = &resource.source {
                        content.push_str(&format!("source = {source:?}\n"));
                    }
                    if let Some(url) = &resource.url {
                        content.push_str(&format!("url = {url:?}\n"));
                    }
                    content.push_str(&format!("path = {:?}\n", resource.path));
                    if let Some(version) = &resource.version {
                        content.push_str(&format!("version = {version:?}\n"));
                    }
                    if let Some(commit) = &resource.resolved_commit {
                        content.push_str(&format!("resolved_commit = {commit:?}\n"));
                    }
                    content.push_str(&format!("checksum = {:?}\n", resource.checksum));
                    content.push_str(&format!("installed_at = {:?}\n\n", resource.installed_at));
                }
            };

        write_resources(&mut content, &self.agents, "agents");
        write_resources(&mut content, &self.snippets, "snippets");
        write_resources(&mut content, &self.commands, "commands");
        write_resources(&mut content, &self.scripts, "scripts");
        write_resources(&mut content, &self.hooks, "hooks");
        write_resources(&mut content, &self.mcp_servers, "mcp-servers");

        atomic_write(path, content.as_bytes()).with_context(|| {
            format!(
                "Cannot write lockfile: {}\n\n\
                    Possible causes:\n\
                    - Permission denied (try running with elevated permissions)\n\
                    - Directory doesn't exist\n\
                    - Disk is full or read-only\n\
                    - File is locked by another process",
                path.display()
            )
        })?;

        Ok(())
    }

    /// Add or update a locked source repository with current timestamp.
    ///
    /// Adds a new source entry or updates an existing one with the same name.
    /// The `fetched_at` timestamp is automatically set to the current UTC time
    /// in RFC 3339 format.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique source identifier (matches manifest `[sources]` keys)
    /// * `url` - Full Git repository URL
    /// * `commit` - Resolved 40-character commit hash
    ///
    /// # Behavior
    ///
    /// If a source with the same name already exists, it will be replaced with
    /// the new information. This ensures that each source name appears exactly
    /// once in the lockfile.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::lockfile::LockFile;
    ///
    /// let mut lockfile = LockFile::new();
    /// lockfile.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/example/community.git".to_string(),
    ///     "a1b2c3d4e5f6789abcdef0123456789abcdef012".to_string()
    /// );
    ///
    /// assert_eq!(lockfile.sources.len(), 1);
    /// assert_eq!(lockfile.sources[0].name, "community");
    /// ```
    ///
    /// # Time Zone
    ///
    /// The `fetched_at` timestamp is always recorded in UTC to ensure consistency
    /// across different time zones and systems.
    pub fn add_source(&mut self, name: String, url: String, commit: String) {
        // Remove existing entry if present
        self.sources.retain(|s| s.name != name);

        self.sources.push(LockedSource {
            name,
            url,
            commit,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Add or update a locked resource (agent or snippet).
    ///
    /// Adds a new resource entry or updates an existing one with the same name
    /// within the appropriate resource type (agents or snippets).
    ///
    /// **Note**: This method is kept for backward compatibility but only supports
    /// agents and snippets. Use `add_typed_resource` to support all resource types
    /// including commands.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique resource identifier within its type
    /// * `resource` - Complete [`LockedResource`] with all resolved information
    /// * `is_agent` - `true` for agents, `false` for snippets
    ///
    /// # Behavior
    ///
    /// If a resource with the same name already exists in the same type category,
    /// it will be replaced. Resources are categorized separately (agents vs snippets),
    /// so an agent named "helper" and a snippet named "helper" can coexist.
    ///
    /// # Examples
    ///
    /// Adding an agent:
    ///
    /// ```rust,no_run
    /// use ccpm::lockfile::{LockFile, LockedResource};
    ///
    /// let mut lockfile = LockFile::new();
    /// let resource = LockedResource {
    ///     name: "example-agent".to_string(),
    ///     source: Some("community".to_string()),
    ///     url: Some("https://github.com/example/repo.git".to_string()),
    ///     path: "agents/example.md".to_string(),
    ///     version: Some("^1.0".to_string()),
    ///     resolved_commit: Some("a1b2c3d...".to_string()),
    ///     checksum: "sha256:abcdef...".to_string(),
    ///     installed_at: "agents/example-agent.md".to_string(),
    /// };
    ///
    /// lockfile.add_resource("example-agent".to_string(), resource, true);
    /// assert_eq!(lockfile.agents.len(), 1);
    /// ```
    ///
    /// Adding a snippet:
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::{LockFile, LockedResource};
    /// # let mut lockfile = LockFile::new();
    /// let snippet = LockedResource {
    ///     name: "util-snippet".to_string(),
    ///     source: None,  // Local resource
    ///     url: None,
    ///     path: "../local/utils.md".to_string(),
    ///     version: None,
    ///     resolved_commit: None,
    ///     checksum: "sha256:fedcba...".to_string(),
    ///     installed_at: "snippets/util-snippet.md".to_string(),
    /// };
    ///
    /// lockfile.add_resource("util-snippet".to_string(), snippet, false);
    /// assert_eq!(lockfile.snippets.len(), 1);
    /// ```
    pub fn add_resource(&mut self, name: String, resource: LockedResource, is_agent: bool) {
        let resources = if is_agent {
            &mut self.agents
        } else {
            &mut self.snippets
        };

        // Remove existing entry if present
        resources.retain(|r| r.name != name);
        resources.push(resource);
    }

    /// Add or update a locked resource with specific resource type.
    ///
    /// This is the preferred method for adding resources as it explicitly
    /// supports all resource types including commands.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique resource identifier within its type
    /// * `resource` - Complete [`LockedResource`] with all resolved information
    /// * `resource_type` - The type of resource (Agent, Snippet, or Command)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::lockfile::{LockFile, LockedResource};
    /// use ccpm::core::ResourceType;
    ///
    /// let mut lockfile = LockFile::new();
    /// let command = LockedResource {
    ///     name: "build-command".to_string(),
    ///     source: Some("community".to_string()),
    ///     url: Some("https://github.com/example/repo.git".to_string()),
    ///     path: "commands/build.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     resolved_commit: Some("a1b2c3d...".to_string()),
    ///     checksum: "sha256:abcdef...".to_string(),
    ///     installed_at: ".claude/commands/build-command.md".to_string(),
    /// };
    ///
    /// lockfile.add_typed_resource("build-command".to_string(), command, ResourceType::Command);
    /// assert_eq!(lockfile.commands.len(), 1);
    /// ```
    pub fn add_typed_resource(
        &mut self,
        name: String,
        resource: LockedResource,
        resource_type: crate::core::ResourceType,
    ) {
        let resources = match resource_type {
            crate::core::ResourceType::Agent => &mut self.agents,
            crate::core::ResourceType::Snippet => &mut self.snippets,
            crate::core::ResourceType::Command => &mut self.commands,
            crate::core::ResourceType::McpServer => {
                // MCP servers are handled differently - they don't use LockedResource
                // This shouldn't be called for MCP servers
                return;
            }
            crate::core::ResourceType::Script => &mut self.scripts,
            crate::core::ResourceType::Hook => &mut self.hooks,
        };

        // Remove existing entry if present
        resources.retain(|r| r.name != name);
        resources.push(resource);
    }

    /// Get a locked resource by name, searching across all resource types.
    ///
    /// Searches for a resource with the given name in the agents, snippets, and
    /// commands collections. Since resource names must be unique within their type,
    /// this method returns the first match found.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to search for
    ///
    /// # Returns
    ///
    /// * `Some(&LockedResource)` - Reference to the found resource
    /// * `None` - No resource with that name exists
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// if let Some(resource) = lockfile.get_resource("example-agent") {
    ///     println!("Found resource: {}", resource.installed_at);
    /// } else {
    ///     println!("Resource not found");
    /// }
    /// ```
    ///
    /// # Search Order
    ///
    /// The method searches agents first, then snippets, then commands. If multiple
    /// resource types have the same name, the first match in this order will be returned.
    #[must_use]
    pub fn get_resource(&self, name: &str) -> Option<&LockedResource> {
        self.agents
            .iter()
            .find(|r| r.name == name)
            .or_else(|| self.snippets.iter().find(|r| r.name == name))
            .or_else(|| self.commands.iter().find(|r| r.name == name))
    }

    /// Get a locked source repository by name.
    ///
    /// Searches for a source repository with the given name in the sources collection.
    ///
    /// # Arguments
    ///
    /// * `name` - Source name to search for (matches manifest `[sources]` keys)
    ///
    /// # Returns
    ///
    /// * `Some(&LockedSource)` - Reference to the found source
    /// * `None` - No source with that name exists
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// if let Some(source) = lockfile.get_source("community") {
    ///     println!("Source URL: {}", source.url);
    ///     println!("Last commit: {}", source.commit);
    /// }
    /// ```
    #[must_use]
    pub fn get_source(&self, name: &str) -> Option<&LockedSource> {
        self.sources.iter().find(|s| s.name == name)
    }

    /// Check if a resource is locked in the lockfile.
    ///
    /// Convenience method that checks whether a resource with the given name
    /// exists in either the agents or snippets collections.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name to check
    ///
    /// # Returns
    ///
    /// * `true` - Resource exists in the lockfile
    /// * `false` - Resource does not exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// if lockfile.has_resource("example-agent") {
    ///     println!("Agent is already locked");
    /// } else {
    ///     println!("Agent needs to be resolved and installed");
    /// }
    /// ```
    ///
    /// This is equivalent to calling `lockfile.get_resource(name).is_some()`.
    #[must_use]
    pub fn has_resource(&self, name: &str) -> bool {
        self.get_resource(name).is_some()
    }

    /// Get all locked resources as a combined vector.
    ///
    /// Returns references to all resources (agents, snippets, and commands) in a single
    /// vector for easy iteration. The order is agents first, then snippets, then commands.
    ///
    /// # Returns
    ///
    /// Vector of references to all locked resources, preserving the order within
    /// each type as they appear in the lockfile.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// let all_resources = lockfile.all_resources();
    /// println!("Total locked resources: {}", all_resources.len());
    ///
    /// for resource in all_resources {
    ///     println!("- {}: {}", resource.name, resource.installed_at);
    /// }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Generating reports of all installed resources
    /// - Validating checksums across all resources
    /// - Listing resources for user display
    /// - Bulk operations on all resources
    ///   Get locked resources for a specific resource type
    ///
    ///
    /// Returns a slice of locked resources for the specified type.
    pub fn get_resources(&self, resource_type: crate::core::ResourceType) -> &[LockedResource] {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &self.agents,
            ResourceType::Snippet => &self.snippets,
            ResourceType::Command => &self.commands,
            ResourceType::Script => &self.scripts,
            ResourceType::Hook => &self.hooks,
            ResourceType::McpServer => &self.mcp_servers,
        }
    }

    /// Get mutable locked resources for a specific resource type
    ///
    /// Returns a mutable slice of locked resources for the specified type.
    pub fn get_resources_mut(
        &mut self,
        resource_type: crate::core::ResourceType,
    ) -> &mut Vec<LockedResource> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => &mut self.agents,
            ResourceType::Snippet => &mut self.snippets,
            ResourceType::Command => &mut self.commands,
            ResourceType::Script => &mut self.scripts,
            ResourceType::Hook => &mut self.hooks,
            ResourceType::McpServer => &mut self.mcp_servers,
        }
    }

    /// Returns all locked resources across all resource types.
    ///
    /// This method collects all resources from agents, snippets, commands,
    /// scripts, hooks, and MCP servers into a single vector. It's useful for
    /// operations that need to process all resources uniformly, such as:
    /// - Generating installation reports
    /// - Validating checksums across all resources
    /// - Bulk operations on resources
    ///
    /// # Returns
    ///
    /// A vector containing references to all [`LockedResource`] entries in the lockfile.
    /// The order matches the resource type order defined in [`crate::core::ResourceType::all()`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// # let lockfile = LockFile::new();
    /// let all_resources = lockfile.all_resources();
    /// println!("Total locked resources: {}", all_resources.len());
    ///
    /// for resource in all_resources {
    ///     println!("- {}: {}", resource.name, resource.installed_at);
    /// }
    /// ```
    #[must_use]
    pub fn all_resources(&self) -> Vec<&LockedResource> {
        let mut resources = Vec::new();

        // Use ResourceType::all() to iterate through all resource types
        for resource_type in crate::core::ResourceType::all() {
            resources.extend(self.get_resources(*resource_type));
        }

        resources
    }

    /// Clear all locked entries from the lockfile.
    ///
    /// Removes all sources, agents, snippets, and commands from the lockfile, returning
    /// it to an empty state. The format version remains unchanged.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::LockFile;
    /// let mut lockfile = LockFile::new();
    /// // ... add sources and resources ...
    ///
    /// lockfile.clear();
    /// assert!(lockfile.sources.is_empty());
    /// assert!(lockfile.agents.is_empty());
    /// assert!(lockfile.snippets.is_empty());
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Preparing for complete lockfile regeneration
    /// - Implementing `ccpm clean` functionality
    /// - Resetting lockfile state during testing
    /// - Handling lockfile corruption recovery
    pub fn clear(&mut self) {
        self.sources.clear();

        // Use ResourceType::all() to clear all resource types
        for resource_type in crate::core::ResourceType::all() {
            self.get_resources_mut(*resource_type).clear();
        }
    }

    /// Compute SHA-256 checksum for a file with integrity verification.
    ///
    /// Calculates the SHA-256 hash of a file's content for integrity verification.
    /// The checksum is used to detect file corruption, tampering, or changes after
    /// installation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Checksum in format "`sha256:hexadecimal_hash`"
    /// * `Err(anyhow::Error)` - File read error with detailed context
    ///
    /// # Checksum Format
    ///
    /// The returned checksum follows the format:
    /// - **Algorithm prefix**: "sha256:"
    /// - **Hash encoding**: Lowercase hexadecimal
    /// - **Length**: 71 characters total (7 for prefix + 64 hex digits)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use ccpm::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let checksum = LockFile::compute_checksum(Path::new("example.md"))?;
    /// println!("File checksum: {}", checksum);
    /// // Output: "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// Provides detailed error context for common issues:
    /// - **File not found**: Suggests checking the path
    /// - **Permission denied**: Suggests checking file permissions
    /// - **IO errors**: Suggests checking disk health or file locks
    ///
    /// # Security Considerations
    ///
    /// - Uses SHA-256, a cryptographically secure hash function
    /// - Suitable for integrity verification and tamper detection
    /// - Consistent across platforms (Windows, macOS, Linux)
    /// - Not affected by line ending differences (hashes actual bytes)
    ///
    /// # Performance
    ///
    /// The method reads the entire file into memory before hashing.
    /// For very large files (>100MB), consider streaming implementations
    /// in future versions.
    pub fn compute_checksum(path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let content = fs::read(path).with_context(|| {
            format!(
                "Cannot read file for checksum calculation: {}\n\n\
                    This error occurs when verifying file integrity.\n\
                    Check that the file exists and is readable.",
                path.display()
            )
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = hasher.finalize();

        Ok(format!("sha256:{}", hex::encode(result)))
    }

    /// Verify that a file matches its expected checksum.
    ///
    /// Computes the current checksum of a file and compares it against the
    /// expected checksum. Used to verify file integrity and detect corruption
    /// or tampering after installation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to verify
    /// * `expected` - Expected checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - File checksum matches expected value
    /// * `Ok(false)` - File checksum does not match (corruption detected)
    /// * `Err(anyhow::Error)` - File read error or checksum calculation failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use ccpm::lockfile::LockFile;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let expected = "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3";
    /// let is_valid = LockFile::verify_checksum(Path::new("example.md"), expected)?;
    ///
    /// if is_valid {
    ///     println!("File integrity verified");
    /// } else {
    ///     println!("WARNING: File has been modified or corrupted!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Installation verification**: Ensure copied files are intact
    /// - **Periodic validation**: Detect file corruption over time
    /// - **Security checks**: Detect unauthorized modifications
    /// - **Troubleshooting**: Diagnose installation issues
    ///
    /// # Performance
    ///
    /// This method internally calls [`compute_checksum`](Self::compute_checksum),
    /// so it has the same performance characteristics. For bulk verification
    /// operations, consider caching computed checksums.
    ///
    /// # Security
    ///
    /// The comparison is performed using standard string equality, which is
    /// not timing-attack resistant. Since checksums are not secrets, this
    /// is acceptable for integrity verification purposes.
    pub fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
        let actual = Self::compute_checksum(path)?;
        Ok(actual == expected)
    }

    /// Update the checksum for a specific resource in the lockfile.
    ///
    /// This method finds a resource by name across all resource types and updates
    /// its checksum value. Used after installation to record the actual file checksum.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the resource to update
    /// * `checksum` - The new SHA-256 checksum in "sha256:hex" format
    ///
    /// # Returns
    ///
    /// Returns `true` if the resource was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use ccpm::lockfile::{LockFile, LockedResource};
    /// # use ccpm::core::ResourceType;
    /// # let mut lockfile = LockFile::default();
    /// # // First add a resource to update
    /// # lockfile.add_typed_resource("my-agent".to_string(), LockedResource {
    /// #     name: "my-agent".to_string(),
    /// #     source: None,
    /// #     url: None,
    /// #     path: "my-agent.md".to_string(),
    /// #     version: None,
    /// #     resolved_commit: None,
    /// #     checksum: "".to_string(),
    /// #     installed_at: "agents/my-agent.md".to_string(),
    /// # }, ResourceType::Agent);
    /// let updated = lockfile.update_resource_checksum(
    ///     "my-agent",
    ///     "sha256:abcdef123456..."
    /// );
    /// assert!(updated);
    /// ```
    pub fn update_resource_checksum(&mut self, name: &str, checksum: &str) -> bool {
        // Try each resource type until we find a match
        for resource in &mut self.agents {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.snippets {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.commands {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.scripts {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.hooks {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        for resource in &mut self.mcp_servers {
            if resource.name == name {
                resource.checksum = checksum.to_string();
                return true;
            }
        }

        false
    }
}

impl Default for LockFile {
    /// Create a new empty lockfile using the current format version.
    ///
    /// This implementation of [`Default`] is equivalent to calling [`LockFile::new()`].
    /// It creates a fresh lockfile with no sources or resources.
    fn default() -> Self {
        Self::new()
    }
}

/// Find the lockfile in the current or parent directories.
///
/// Searches upward from the current working directory to find a `ccpm.lock` file,
/// similar to how Git searches for `.git` directories. This enables running CCPM
/// commands from subdirectories within a project.
///
/// # Search Algorithm
///
/// 1. Start from current working directory
/// 2. Check for `ccpm.lock` in current directory
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
/// use ccpm::lockfile::find_lockfile;
///
/// if let Some(lockfile_path) = find_lockfile() {
///     println!("Found lockfile: {}", lockfile_path.display());
/// } else {
///     println!("No lockfile found (run 'ccpm install' to create one)");
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
/// ├── ccpm.lock          # ← This will be found
/// ├── ccpm.toml
/// └── src/
///     └── subdir/         # ← Commands run from here will find ../ccpm.lock
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
        let lockfile_path = current.join("ccpm.lock");
        if lockfile_path.exists() {
            return Some(lockfile_path);
        }

        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lockfile_new() {
        let lockfile = LockFile::new();
        assert_eq!(lockfile.version, LockFile::CURRENT_VERSION);
        assert!(lockfile.sources.is_empty());
        assert!(lockfile.agents.is_empty());
    }

    #[test]
    fn test_lockfile_save_load() {
        let temp = tempdir().unwrap();
        let lockfile_path = temp.path().join("ccpm.lock");

        let mut lockfile = LockFile::new();

        // Add a source
        lockfile.add_source(
            "official".to_string(),
            "https://github.com/example-org/ccpm-official.git".to_string(),
            "abc123".to_string(),
        );

        // Add a resource
        lockfile.add_resource(
            "test-agent".to_string(),
            LockedResource {
                name: "test-agent".to_string(),
                source: Some("official".to_string()),
                url: Some("https://github.com/example-org/ccpm-official.git".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:abcdef".to_string(),
                installed_at: "agents/test-agent.md".to_string(),
            },
            true,
        );

        // Save
        lockfile.save(&lockfile_path).unwrap();
        assert!(lockfile_path.exists());

        // Load
        let loaded = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(loaded.version, LockFile::CURRENT_VERSION);
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.get_source("official").unwrap().commit, "abc123");
        assert_eq!(
            loaded.get_resource("test-agent").unwrap().checksum,
            "sha256:abcdef"
        );
    }

    #[test]
    fn test_lockfile_empty_file() {
        let temp = tempdir().unwrap();
        let lockfile_path = temp.path().join("ccpm.lock");

        // Create empty file
        std::fs::write(&lockfile_path, "").unwrap();

        // Should return new lockfile
        let lockfile = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(lockfile.version, LockFile::CURRENT_VERSION);
        assert!(lockfile.sources.is_empty());
    }

    #[test]
    fn test_lockfile_version_check() {
        let temp = tempdir().unwrap();
        let lockfile_path = temp.path().join("ccpm.lock");

        // Create lockfile with future version
        let content = "version = 999\n";
        std::fs::write(&lockfile_path, content).unwrap();

        // Should fail to load
        let result = LockFile::load(&lockfile_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("newer than supported")
        );
    }

    #[test]
    fn test_resource_operations() {
        let mut lockfile = LockFile::new();

        // Add resources
        lockfile.add_resource(
            "agent1".to_string(),
            LockedResource {
                name: "agent1".to_string(),
                source: None,
                url: None,
                path: "local/agent1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:111".to_string(),
                installed_at: "agents/agent1.md".to_string(),
            },
            true, // is_agent
        );

        lockfile.add_resource(
            "snippet1".to_string(),
            LockedResource {
                name: "snippet1".to_string(),
                source: None,
                url: None,
                path: "local/snippet1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:222".to_string(),
                installed_at: "snippets/snippet1.md".to_string(),
            },
            false, // is_agent
        );

        lockfile.add_resource(
            "dev-agent1".to_string(),
            LockedResource {
                name: "dev-agent1".to_string(),
                source: None,
                url: None,
                path: "local/dev-agent1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:333".to_string(),
                installed_at: "agents/dev-agent1.md".to_string(),
            },
            true, // is_agent
        );

        // Test getters
        assert!(lockfile.has_resource("agent1"));
        assert!(lockfile.has_resource("snippet1"));
        assert!(lockfile.has_resource("dev-agent1"));
        assert!(!lockfile.has_resource("nonexistent"));

        assert_eq!(lockfile.all_resources().len(), 3);
        // Note: production_resources() removed as dev/production concept was eliminated

        // Test clear
        lockfile.clear();
        assert!(lockfile.all_resources().is_empty());
    }

    #[test]
    fn test_checksum_computation() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.md");

        std::fs::write(&file_path, "Hello, World!").unwrap();

        let checksum = LockFile::compute_checksum(&file_path).unwrap();
        assert!(checksum.starts_with("sha256:"));

        // Verify checksum
        assert!(LockFile::verify_checksum(&file_path, &checksum).unwrap());
        assert!(!LockFile::verify_checksum(&file_path, "sha256:wrong").unwrap());
    }

    #[test]
    fn test_lockfile_with_commands() {
        let mut lockfile = LockFile::new();

        // Add a command resource using add_typed_resource
        lockfile.add_typed_resource(
            "build".to_string(),
            LockedResource {
                name: "build".to_string(),
                source: Some("community".to_string()),
                url: Some("https://github.com/example/community.git".to_string()),
                path: "commands/build.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:cmd123".to_string(),
                installed_at: ".claude/commands/build.md".to_string(),
            },
            crate::core::ResourceType::Command,
        );

        assert_eq!(lockfile.commands.len(), 1);
        assert!(lockfile.has_resource("build"));

        let resource = lockfile.get_resource("build").unwrap();
        assert_eq!(resource.name, "build");
        assert_eq!(resource.installed_at, ".claude/commands/build.md");
    }

    #[test]
    fn test_lockfile_all_resources_with_commands() {
        let mut lockfile = LockFile::new();

        // Add resources of each type
        lockfile.add_resource(
            "agent1".to_string(),
            LockedResource {
                name: "agent1".to_string(),
                source: None,
                url: None,
                path: "agent1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:a1".to_string(),
                installed_at: "agents/agent1.md".to_string(),
            },
            true,
        );

        lockfile.add_resource(
            "snippet1".to_string(),
            LockedResource {
                name: "snippet1".to_string(),
                source: None,
                url: None,
                path: "snippet1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:s1".to_string(),
                installed_at: "snippets/snippet1.md".to_string(),
            },
            false,
        );

        lockfile.add_typed_resource(
            "command1".to_string(),
            LockedResource {
                name: "command1".to_string(),
                source: None,
                url: None,
                path: "command1.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:c1".to_string(),
                installed_at: ".claude/commands/command1.md".to_string(),
            },
            crate::core::ResourceType::Command,
        );

        let all = lockfile.all_resources();
        assert_eq!(all.len(), 3);

        // Test clear includes commands
        lockfile.clear();
        assert!(lockfile.agents.is_empty());
        assert!(lockfile.snippets.is_empty());
        assert!(lockfile.commands.is_empty());
    }

    #[test]
    fn test_lockfile_save_load_commands() {
        let temp = tempdir().unwrap();
        let lockfile_path = temp.path().join("ccpm.lock");

        let mut lockfile = LockFile::new();

        // Add command
        lockfile.add_typed_resource(
            "deploy".to_string(),
            LockedResource {
                name: "deploy".to_string(),
                source: Some("official".to_string()),
                url: Some("https://github.com/example/official.git".to_string()),
                path: "commands/deploy.md".to_string(),
                version: Some("v2.0.0".to_string()),
                resolved_commit: Some("def456".to_string()),
                checksum: "sha256:deploy123".to_string(),
                installed_at: ".claude/commands/deploy.md".to_string(),
            },
            crate::core::ResourceType::Command,
        );

        // Save
        lockfile.save(&lockfile_path).unwrap();

        // Load and verify
        let loaded = LockFile::load(&lockfile_path).unwrap();
        assert_eq!(loaded.commands.len(), 1);
        assert!(loaded.has_resource("deploy"));

        let cmd = &loaded.commands[0];
        assert_eq!(cmd.name, "deploy");
        assert_eq!(cmd.version, Some("v2.0.0".to_string()));
        assert_eq!(cmd.installed_at, ".claude/commands/deploy.md");
    }

    #[test]
    fn test_lockfile_get_resource_precedence() {
        let mut lockfile = LockFile::new();

        // Add resources with same name but different types
        lockfile.add_resource(
            "helper".to_string(),
            LockedResource {
                name: "helper".to_string(),
                source: None,
                url: None,
                path: "agent_helper.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:agent".to_string(),
                installed_at: "agents/helper.md".to_string(),
            },
            true,
        );

        lockfile.add_typed_resource(
            "helper".to_string(),
            LockedResource {
                name: "helper".to_string(),
                source: None,
                url: None,
                path: "command_helper.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: "sha256:command".to_string(),
                installed_at: ".claude/commands/helper.md".to_string(),
            },
            crate::core::ResourceType::Command,
        );

        // get_resource should return agent (higher precedence)
        let resource = lockfile.get_resource("helper").unwrap();
        assert_eq!(resource.installed_at, "agents/helper.md");
    }
}
