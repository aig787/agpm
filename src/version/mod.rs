//! Version constraint parsing, comparison, and resolution for AGPM dependencies.
//!
//! This module provides comprehensive version management for AGPM, handling semantic
//! versioning, Git references, and dependency resolution. It supports multiple version
//! specification formats and provides sophisticated constraint resolution with conflict
//! detection and prerelease handling.
//!
//! # Module Organization
//!
//! - [`constraints`] - Version constraint parsing, sets, and resolution
//! - [`comparison`] - Version comparison utilities and analysis
//! - Core types and functions for Git tag resolution
//!
//! # Version Specifications
//!
//! AGPM supports several version specification formats:
//!
//! ## Semantic Versions
//! - **Exact versions**: `"1.0.0"` - Matches exactly the specified version
//! - **Caret ranges**: `"^1.0.0"` - Compatible within major version (1.x.x)
//! - **Tilde ranges**: `"~1.2.0"` - Compatible within minor version (1.2.x)
//! - **Comparison ranges**: `">=1.0.0"`, `"<2.0.0"`, `">=1.0.0, <2.0.0"`
//!
//! ## Special Keywords
//! - **Wildcard**: `"*"` - Matches any version
//!
//! ## Git References
//! - **Branches**: `"main"`, `"develop"`, `"feature/auth"`
//! - **Tags**: `"v1.0.0"`, `"release-2023-01"`
//! - **Commits**: `"abc123..."` (full or abbreviated SHA)
//!
//! # Version Resolution Strategy
//!
//! The version resolution system follows this process:
//!
//! 1. **Tag Discovery**: Fetch all tags from the Git repository
//! 2. **Semantic Parsing**: Parse tags as semantic versions where possible
//! 3. **Constraint Matching**: Apply version constraints to find candidates
//! 4. **Best Selection**: Choose the highest compatible version
//! 5. **Fallback Handling**: Use branches or commits if no tags match
//!
//! # Constraint Resolution Features
//!
//! - **Multi-constraint support**: Combine multiple constraints per dependency
//! - **Conflict detection**: Prevent impossible constraint combinations
//! - **Prerelease handling**: Sophisticated alpha/beta/RC version management
//! - **Cross-dependency resolution**: Resolve entire dependency graphs
//!
//! # Examples
//!
//! ## Basic Git Tag Resolution
//!
//! ```rust,no_run
//! use agpm_cli::version::{VersionResolver, VersionInfo};
//! use agpm_cli::git::GitRepo;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
//! let resolver = VersionResolver::from_git_tags(&repo).await?;
//!
//! // Resolve different constraint types
//! if let Ok(Some(version)) = resolver.resolve("^1.0.0") {
//!     println!("Resolved caret constraint to: {}", version.tag);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Advanced Constraint Resolution
//!
//! ```rust,no_run
//! use agpm_cli::version::constraints::{ConstraintResolver, VersionConstraint};
//! use semver::Version;
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! let mut resolver = ConstraintResolver::new();
//!
//! // Add constraints for multiple dependencies
//! resolver.add_constraint("web-framework", "^2.0.0")?;
//! resolver.add_constraint("database", "~1.5.0")?;
//! resolver.add_constraint("auth-lib", ">=3.0.0")?;
//!
//! // Provide available versions
//! let mut available = HashMap::new();
//! available.insert("web-framework".to_string(), vec![Version::parse("2.1.0")?]);
//! available.insert("database".to_string(), vec![Version::parse("1.5.3")?]);
//! available.insert("auth-lib".to_string(), vec![Version::parse("3.2.0")?]);
//!
//! // Resolve all dependencies simultaneously
//! let resolved = resolver.resolve(&available)?;
//! println!("Resolved {} dependencies", resolved.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Version Comparison and Analysis
//!
//! ```rust,no_run
//! use agpm_cli::version::comparison::VersionComparator;
//!
//! # fn example() -> anyhow::Result<()> {
//! let available_versions = vec![
//!     "v1.0.0".to_string(),
//!     "v1.5.0".to_string(),
//!     "v2.0.0".to_string(),
//! ];
//!
//! // Check for newer versions
//! let has_updates = VersionComparator::has_newer_version("1.2.0", &available_versions)?;
//! println!("Updates available: {}", has_updates);
//!
//! // Get all newer versions sorted by recency
//! let newer = VersionComparator::get_newer_versions("1.2.0", &available_versions)?;
//! for version in newer {
//!     println!("Newer version: {}", version);
//! }
//!
//! // Find the latest version
//! if let Some(latest) = VersionComparator::get_latest(&available_versions)? {
//!     println!("Latest version: {}", latest);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Prerelease Version Handling
//!
//! AGPM provides sophisticated prerelease version management:
//!
//! - **Default exclusion**: Most constraints exclude prereleases for stability
//! - **Explicit inclusion**: Use Git refs to include them
//! - **Constraint inheritance**: If any constraint allows prereleases, all do
//! - **Version precedence**: Stable versions are preferred when available
//!
//! # Error Handling
//!
//! The version system provides comprehensive error handling:
//!
//! - **Invalid version strings**: Malformed semantic versions are rejected
//! - **Conflicting constraints**: Impossible combinations are detected early
//! - **Missing dependencies**: Required dependencies without versions are flagged
//! - **Resolution failures**: Unsatisfiable constraints are clearly reported
//!
//! # Cross-References
//!
//! - For detailed constraint syntax and resolution: [`constraints`]
//! - For version comparison utilities: [`comparison`]
//! - For Git repository integration: [`crate::git`]
//! - For dependency management: [`crate::resolver`]

use crate::git::GitRepo;
use anyhow::{Context, Result};
use regex::Regex;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Parse a version requirement string, normalizing 'v' prefixes.
///
/// This helper function provides centralized semver parsing that handles both
/// prefixed (`v1.0.0`, `^v1.0.0`) and unprefixed (`1.0.0`, `^1.0.0`) version strings.
///
/// # Arguments
///
/// * `requirement` - Version requirement string (e.g., "^v1.0.0", "~2.1.0", ">=v1.0.0")
///
/// # Returns
///
/// A parsed `VersionReq` if the requirement is valid semver syntax.
///
/// # Examples
///
/// ```
/// use agpm_cli::version::parse_version_req;
///
/// // All of these parse successfully:
/// assert!(parse_version_req("1.0.0").is_ok());
/// assert!(parse_version_req("v1.0.0").is_ok());
/// assert!(parse_version_req("^1.0.0").is_ok());
/// assert!(parse_version_req("^v1.0.0").is_ok());
/// assert!(parse_version_req("~v2.1.0").is_ok());
/// assert!(parse_version_req(">=v1.0.0").is_ok());
/// ```
pub fn parse_version_req(requirement: &str) -> Result<VersionReq, semver::Error> {
    // Strip 'v' prefix from version requirements
    // Handles patterns like: "v1.0.0", "^v1.0.0", "~v2.1.0", "=v1.0.0", ">=v1.0.0", etc.
    // We match 'v' at the start OR after operators to avoid breaking prerelease tags
    // like "1.0.0-dev.1" or branch names like "develop"

    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(^|[~^=><])v").unwrap());

    let normalized = RE.replace_all(requirement, "$1");

    VersionReq::parse(&normalized)
}

/// Splits a version string into an optional prefix and the version/constraint part.
///
/// This function extracts versioned prefixes from tag names and version constraints,
/// enabling support for prefixed versioning schemes like `agents-v1.0.0` or `snippets-^v2.0.0`.
/// The prefix can contain hyphens and is separated from the version by detecting where
/// the version pattern begins.
///
/// # Algorithm
///
/// Scans left-to-right to find the first occurrence of:
/// - Constraint operators: `^`, `~`, `=`, `<`, `>`, `!`, `*`
/// - Version prefix: `v` followed immediately by a digit
/// - Bare digit (start of version number)
///
/// Everything before this point (minus trailing `-`) becomes the prefix.
///
/// # Arguments
///
/// * `s` - The string to parse (tag name or version constraint)
///
/// # Returns
///
/// A tuple of `(Option<String>, &str)` where:
/// - First element is the prefix (if any)
/// - Second element is the version/constraint string
///
/// # Examples
///
/// ```
/// use agpm_cli::version::split_prefix_and_version;
///
/// // Prefixed versions
/// assert_eq!(
///     split_prefix_and_version("agents-v1.0.0"),
///     (Some("agents".to_string()), "v1.0.0")
/// );
/// assert_eq!(
///     split_prefix_and_version("my-tool-^v2.0.0"),
///     (Some("my-tool".to_string()), "^v2.0.0")
/// );
///
/// // Unprefixed versions
/// assert_eq!(split_prefix_and_version("v1.0.0"), (None, "v1.0.0"));
/// assert_eq!(split_prefix_and_version("^1.0.0"), (None, "^1.0.0"));
///
/// // Edge cases
/// assert_eq!(
///     split_prefix_and_version("tool-v-v1.0.0"),
///     (Some("tool-v".to_string()), "v1.0.0")
/// );
/// ```
#[inline]
pub fn split_prefix_and_version(s: &str) -> (Option<String>, &str) {
    // Iterate through characters with their byte indices (O(n) single pass)
    for (byte_idx, ch) in s.char_indices() {
        // Check for constraint operators or wildcard
        if "^~=<>!*".contains(ch) {
            return split_at_index(s, byte_idx);
        }

        // Check for 'v' followed by digit (version prefix)
        if ch == 'v' {
            // Look ahead to check next character (O(1) operation)
            if s[byte_idx..].chars().nth(1).is_some_and(|next| next.is_ascii_digit()) {
                return split_at_index(s, byte_idx);
            }
        }

        // Check for bare digit (start of version number)
        // Only treat as version start if:
        // 1. At position 0 (start of string), OR
        // 2. Immediately after a hyphen delimiter
        if ch.is_ascii_digit() {
            // Check if at start or after hyphen (O(1) operation)
            let is_version_start = byte_idx == 0 || s[..byte_idx].ends_with('-');

            if is_version_start {
                return split_at_index(s, byte_idx);
            }
        }
    }

    // No version pattern found, entire string is the version/constraint
    (None, s)
}

/// Helper function to split string at an index, extracting prefix if present.
#[inline]
fn split_at_index(s: &str, i: usize) -> (Option<String>, &str) {
    if i == 0 {
        // Version starts at beginning, no prefix
        (None, s)
    } else {
        // Extract prefix (remove trailing hyphen)
        let prefix = s[..i].trim_end_matches('-');
        // Treat empty prefix as None (handles cases like "-v1.0.0")
        if prefix.is_empty() {
            (None, &s[i..])
        } else {
            (Some(prefix.to_string()), &s[i..])
        }
    }
}

/// Version information extracted from a Git tag.
///
/// `VersionInfo` represents a successfully parsed semantic version from a Git tag,
/// along with metadata about the original tag and prerelease status. This structure
/// is used throughout the version resolution system to maintain the connection
/// between semantic versions and their source Git references.
///
/// # Fields
///
/// - `prefix`: Optional prefix for monorepo-style versioning (e.g., `"agents"` in `agents-v1.0.0`)
/// - `version`: The parsed semantic version
/// - `tag`: The original Git tag string (may include prefixes like `v` or `agents-v`)
/// - `prerelease`: Whether this version contains prerelease identifiers
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::version::VersionInfo;
/// use semver::Version;
///
/// // Standard version without prefix
/// let info = VersionInfo {
///     prefix: None,
///     version: Version::parse("1.0.0-beta.1").unwrap(),
///     tag: "v1.0.0-beta.1".to_string(),
///     prerelease: true,
/// };
///
/// assert_eq!(info.prefix, None);
/// assert_eq!(info.version.major, 1);
/// assert_eq!(info.tag, "v1.0.0-beta.1");
/// assert!(info.prerelease);
///
/// // Prefixed version for monorepo
/// let prefixed = VersionInfo {
///     prefix: Some("agents".to_string()),
///     version: Version::parse("2.0.0").unwrap(),
///     tag: "agents-v2.0.0".to_string(),
///     prerelease: false,
/// };
///
/// assert_eq!(prefixed.prefix, Some("agents".to_string()));
/// assert_eq!(prefixed.version.major, 2);
/// ```
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// Optional prefix for versioned namespaces (e.g., "agents", "snippets")
    pub prefix: Option<String>,
    /// The parsed semantic version
    pub version: Version,
    /// The original Git tag string
    pub tag: String,
    /// Whether this is a prerelease version (alpha, beta, rc, etc.)
    pub prerelease: bool,
}

/// Resolves semantic versions from Git repository tags.
///
/// `VersionResolver` provides the core functionality for discovering, parsing, and
/// resolving semantic versions from Git tags. It handles tag discovery, version
/// parsing, constraint matching, and best-version selection.
///
/// # Tag Processing
///
/// The resolver automatically:
/// - Fetches all tags from a Git repository
/// - Normalizes tag names (removes `v` prefixes, handles common formats)
/// - Parses valid semantic versions (skips invalid tags)
/// - Sorts versions in descending order (newest first)
/// - Categorizes versions as stable or prerelease
///
/// # Resolution Strategy
///
/// When resolving version constraints:
/// 1. **Exact versions** are matched with or without `v` prefixes
/// 2. **Semantic ranges** are applied using semver matching rules (e.g., `^1.0.0`, `~2.1.0`)
/// 3. **Tag/branch names** are matched exactly as fallback (including "latest" - just a name)
/// 4. **Prerelease filtering** is applied based on constraint type
///
/// # Examples
///
/// ## Creating from Git Repository
///
/// ```rust,no_run
/// use agpm_cli::version::VersionResolver;
/// use agpm_cli::git::GitRepo;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
/// let resolver = VersionResolver::from_git_tags(&repo).await?;
///
/// println!("Found {} versions", resolver.list_all().len());
/// # Ok(())
/// # }
/// ```
///
/// ## Version Resolution
///
/// ```rust,no_run
/// # use agpm_cli::version::VersionResolver;
/// # use agpm_cli::git::GitRepo;
/// # use std::path::PathBuf;
/// #
/// # async fn example() -> anyhow::Result<()> {
/// # let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
/// # let resolver = VersionResolver::from_git_tags(&repo).await?;
///
/// // Resolve various constraint types
/// if let Some(version) = resolver.resolve("^1.0.0")? {
///     println!("Caret range resolved to: {} ({})", version.tag, version.version);
/// }
///
/// if let Some(version) = resolver.resolve("v1.2.3")? {
///     println!("Exact match: {}", version.tag);
/// }
/// # Ok(())
/// # }
/// ```
pub struct VersionResolver {
    versions: Vec<Arc<VersionInfo>>,
}

impl VersionResolver {
    /// Create a new empty resolver with no versions.
    ///
    /// This constructor creates an empty resolver that contains no version information.
    /// It's primarily useful for testing or as a starting point before adding versions
    /// manually. For normal usage, prefer [`from_git_tags`](Self::from_git_tags) which
    /// populates the resolver from a Git repository.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    ///
    /// let resolver = VersionResolver::new();
    /// assert_eq!(resolver.list_all().len(), 0);
    /// assert!(resolver.get_latest().is_none());
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            versions: Vec::new(),
        }
    }

    /// Create a resolver by discovering and parsing tags from a Git repository.
    ///
    /// This method performs the complete tag discovery and parsing workflow:
    /// 1. **Fetch tags**: Retrieve all Git tags from the repository
    /// 2. **Parse versions**: Attempt to parse each tag as a semantic version
    /// 3. **Filter valid**: Keep only tags that parse successfully
    /// 4. **Sort versions**: Order by semantic version (newest first)
    /// 5. **Detect prereleases**: Identify versions with prerelease components
    ///
    /// # Arguments
    ///
    /// * `repo` - The [`GitRepo`] instance to discover tags from
    ///
    /// # Returns
    ///
    /// Returns `Ok(VersionResolver)` with parsed versions, or `Err` if Git
    /// operations fail. Individual tag parsing failures are silently ignored.
    ///
    /// # Tag Parsing Rules
    ///
    /// - Common prefixes (`v`, `V`) are automatically stripped
    /// - Invalid semantic versions are skipped (not included in resolver)
    /// - Valid versions are sorted in descending order
    /// - Prerelease status is detected from version components
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// println!("Discovered {} valid versions", resolver.list_all().len());
    ///
    /// if let Some(latest) = resolver.get_latest() {
    ///     println!("Latest version: {} ({})", latest.tag, latest.version);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// This method returns errors for Git operations (repository access, tag listing)
    /// but handles individual tag parsing failures gracefully by skipping invalid tags.
    pub async fn from_git_tags(repo: &GitRepo) -> Result<Self> {
        let tags = repo.list_tags().await?;
        let mut versions = Vec::new();

        for tag in tags {
            if let Ok((prefix, version)) = Self::parse_tag(&tag) {
                versions.push(Arc::new(VersionInfo {
                    prefix,
                    version: version.clone(),
                    tag: tag.clone(),
                    prerelease: !version.pre.is_empty(),
                }));
            }
        }

        // Sort versions in descending order (newest first)
        versions.sort_by(|a, b| b.version.cmp(&a.version));

        Ok(Self {
            versions,
        })
    }

    /// Parse a Git tag string into an optional prefix and semantic version.
    ///
    /// This internal method handles the extraction of versioned prefixes and parsing of
    /// Git tag strings into semantic versions. It supports both prefixed tags (e.g.,
    /// `agents-v1.0.0`) and unprefixed tags (e.g., `v1.0.0`).
    ///
    /// # Parsing Process
    ///
    /// 1. **Prefix extraction**: Use `split_prefix_and_version()` to separate prefix from version
    /// 2. **Version normalization**: Strip `v` or `V` prefixes from version string
    /// 3. **Semantic parsing**: Parse the cleaned string as a semantic version
    /// 4. **Error context**: Provide helpful error messages for parsing failures
    ///
    /// # Arguments
    ///
    /// * `tag` - The Git tag string to parse
    ///
    /// # Returns
    ///
    /// Returns `Ok((Option<String>, Version))` where:
    /// - First element is the optional prefix
    /// - Second element is the parsed semantic version
    ///
    /// Returns `Err` for invalid semantic versions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use semver::Version;
    ///
    /// // These would all parse successfully (if the method were public)
    /// // "v1.0.0" → (None, Version::new(1, 0, 0))
    /// // "agents-v2.1.3" → (Some("agents"), Version::new(2, 1, 3))
    /// // "my-tool-v1.5.0-beta.1" → (Some("my-tool"), Version with prerelease)
    /// ```
    ///
    /// # Implementation Note
    ///
    /// This method is private and used internally by [`from_git_tags`](Self::from_git_tags)
    /// during the tag discovery and parsing process.
    fn parse_tag(tag: &str) -> Result<(Option<String>, Version)> {
        // Extract prefix and version string
        let (prefix, version_str) = split_prefix_and_version(tag);

        // Remove common version prefixes from the version part
        let cleaned = version_str.trim_start_matches('v').trim_start_matches('V');

        // Parse semantic version
        let version = Version::parse(cleaned)
            .with_context(|| format!("Failed to parse version from tag: {tag}"))?;

        Ok((prefix, version))
    }

    /// Resolve a version requirement string to a specific version from available tags.
    ///
    /// This method applies version constraint logic to find the best matching version
    /// from the resolver's collection of parsed Git tags. It supports various constraint
    /// formats and applies appropriate matching rules for each type.
    ///
    /// # Constraint Resolution Order
    ///
    /// 1. **Exact versions**: Direct semantic version matches (with/without `v` prefix)
    /// 2. **Version requirements**: Semver ranges like `"^1.0.0"`, `"~1.2.0"`, `"*"`
    /// 3. **Tag names**: Exact tag string matching as fallback
    ///
    /// # Arguments
    ///
    /// * `requirement` - The version constraint string to resolve
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(VersionInfo))` if a matching version is found, `Ok(None)`
    /// if no version satisfies the requirement, or `Err` for invalid requirements.
    ///
    /// # Prerelease Handling
    ///
    /// - **Default behavior**: Prereleases are excluded from semver range matching
    /// - **Explicit matches**: Direct version/tag matches include prereleases
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// // Exact version matching
    /// if let Some(version) = resolver.resolve("1.2.3")? {
    ///     println!("Found exact version: {}", version.tag);
    /// }
    ///
    /// // Semver ranges
    /// if let Some(version) = resolver.resolve("^1.0.0")? {
    ///     println!("Compatible version: {} ({})", version.tag, version.version);
    /// }
    ///
    /// // Tag name matching
    /// if let Some(version) = resolver.resolve("v1.0.0-beta.1")? {
    ///     println!("Tag match: {}", version.tag);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Resolution Precedence
    ///
    /// When multiple versions could match:
    /// - **Highest version wins**: Newer semantic versions are preferred
    /// - **Stable over prerelease**: Stable versions preferred unless prereleases explicitly allowed
    /// - **First match for tags**: Tag name matching returns the first occurrence
    pub fn resolve(&self, requirement: &str) -> Result<Option<Arc<VersionInfo>>> {
        // Extract prefix and version part (e.g., "agents-^v1.0.0" → (Some("agents"), "^v1.0.0"))
        let (prefix, version_str) = split_prefix_and_version(requirement);

        // Filter versions by prefix first
        let matching_prefix: Vec<&Arc<VersionInfo>> =
            self.versions.iter().filter(|v| v.prefix.as_ref() == prefix.as_ref()).collect();

        // Try exact version match (with or without 'v' prefix)
        if let Ok(exact_version) = Version::parse(version_str.trim_start_matches('v')) {
            return Ok(matching_prefix
                .iter()
                .find(|v| v.version == exact_version)
                .map(|&v| Arc::clone(v)));
        }

        // Try as semantic version requirement using centralized parser
        if let Ok(req) = parse_version_req(version_str) {
            return Ok(matching_prefix
                .iter()
                .filter(|v| !v.prerelease) // Exclude prereleases by default
                .find(|v| req.matches(&v.version))
                .map(|&v| Arc::clone(v)));
        }

        // Try exact tag match (full tag including prefix)
        for version in &self.versions {
            if version.tag == requirement {
                return Ok(Some(Arc::clone(version)));
            }
        }

        Ok(None)
    }

    /// Get the latest version including prereleases.
    ///
    /// This method returns the absolute newest version from the resolver's collection,
    /// including prerelease versions. Since versions are sorted in descending order,
    /// this simply returns the first version in the list.
    ///
    /// # Returns
    ///
    /// Returns `Some(VersionInfo)` with the highest version, or `None` if no versions
    /// are available in the resolver.
    ///
    /// # Prerelease Inclusion
    ///
    /// Unlike [`get_latest_stable`](Self::get_latest_stable), this method includes
    /// prerelease versions in consideration. If the highest version happens to be
    /// a prerelease (e.g., `2.0.0-beta.1` when `1.9.0` is the latest stable),
    /// the prerelease version will be returned.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// if let Some(latest) = resolver.get_latest() {
    ///     println!("Absolute latest: {} (prerelease: {})",
    ///              latest.tag, latest.prerelease);
    /// } else {
    ///     println!("No versions found in repository");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful when:
    /// - You want the cutting-edge version regardless of stability
    /// - Implementing `latest-prerelease` constraint resolution
    /// - Analyzing the most recent development activity
    #[must_use]
    pub fn get_latest(&self) -> Option<Arc<VersionInfo>> {
        self.versions.first().map(Arc::clone)
    }

    /// Get the latest stable version excluding prereleases.
    ///
    /// This method finds the newest version that doesn't contain prerelease identifiers
    /// (such as `-alpha`, `-beta`, `-rc`). It's the preferred method for production
    /// environments where stability is prioritized over cutting-edge features.
    ///
    /// # Returns
    ///
    /// Returns `Some(VersionInfo)` with the highest stable version, or `None` if no
    /// stable versions are available (only prereleases exist).
    ///
    /// # Stability Definition
    ///
    /// A version is considered stable if its prerelease component is empty. This means:
    /// - `1.0.0` is stable
    /// - `1.0.0-beta.1` is not stable (has prerelease suffix)
    /// - `1.0.0+build.123` is stable (build metadata doesn't affect stability)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// match resolver.get_latest_stable() {
    ///     Some(stable) => {
    ///         println!("Latest stable version: {}", stable.tag);
    ///         assert!(!stable.prerelease); // Always false for stable versions
    ///     }
    ///     None => println!("No stable versions found (only prereleases available)"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Comparison with `get_latest()`
    ///
    /// ```rust,no_run
    /// # use agpm_cli::version::VersionResolver;
    /// # use agpm_cli::git::GitRepo;
    /// # use std::path::PathBuf;
    /// #
    /// # async fn example() -> anyhow::Result<()> {
    /// # let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// # let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let latest = resolver.get_latest();
    /// let stable = resolver.get_latest_stable();
    ///
    /// // Latest might be a prerelease version
    /// // Stable will always be a non-prerelease version (or None)
    ///
    /// if let (Some(l), Some(s)) = (latest, stable) {
    ///     if l.version > s.version {
    ///         println!("Newest version {} is a prerelease", l.tag);
    ///         println!("Latest stable version is {}", s.tag);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is ideal for:
    /// - Production dependency resolution
    /// - Implementing `"latest"` constraint resolution
    /// - Default version selection in package managers
    /// - Stable release identification
    #[must_use]
    pub fn get_latest_stable(&self) -> Option<Arc<VersionInfo>> {
        self.versions.iter().find(|v| !v.prerelease).map(Arc::clone)
    }

    /// List all versions discovered from Git tags.
    ///
    /// This method returns a complete list of all successfully parsed versions from
    /// the Git repository, including both stable and prerelease versions. The list
    /// is sorted in descending order by semantic version (newest first).
    ///
    /// # Returns
    ///
    /// Returns `Vec<VersionInfo>` containing all parsed versions. The vector may be
    /// empty if no valid semantic versions were found in the repository tags.
    ///
    /// # Sorting Order
    ///
    /// Versions are sorted by semantic version precedence in descending order:
    /// - Higher major versions first (e.g., `2.0.0` before `1.9.0`)
    /// - Higher minor versions within same major (e.g., `1.5.0` before `1.2.0`)
    /// - Higher patch versions within same minor (e.g., `1.2.3` before `1.2.1`)
    /// - Release versions before prereleases (e.g., `1.0.0` before `1.0.0-beta.1`)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let all_versions = resolver.list_all();
    /// println!("Found {} versions:", all_versions.len());
    ///
    /// for (i, version) in all_versions.iter().enumerate() {
    ///     let status = if version.prerelease { "prerelease" } else { "stable" };
    ///     println!("  {}. {} ({}) - {}", i + 1, version.tag, version.version, status);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Filtering and Analysis
    ///
    /// ```rust,no_run
    /// # use agpm_cli::version::VersionResolver;
    /// # use agpm_cli::git::GitRepo;
    /// # use std::path::PathBuf;
    /// #
    /// # async fn example() -> anyhow::Result<()> {
    /// # let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// # let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let all_versions = resolver.list_all();
    ///
    /// // Count prereleases vs stable
    /// let prerelease_count = all_versions.iter().filter(|v| v.prerelease).count();
    /// let stable_count = all_versions.len() - prerelease_count;
    ///
    /// println!("Stable versions: {}, Prereleases: {}", stable_count, prerelease_count);
    ///
    /// // Find versions in a specific range
    /// let v1_versions: Vec<_> = all_versions.iter()
    ///     .filter(|v| v.version.major == 1)
    ///     .collect();
    /// println!("Found {} versions in v1.x.x series", v1_versions.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful for:
    /// - Version analysis and reporting
    /// - Building version selection interfaces
    /// - Debugging version resolution issues
    /// - Implementing custom constraint logic
    #[must_use]
    pub fn list_all(&self) -> Vec<Arc<VersionInfo>> {
        self.versions.clone()
    }

    /// List only stable versions excluding prereleases.
    ///
    /// This method filters the complete version list to include only versions without
    /// prerelease components. It's useful for scenarios where you need to work with
    /// production-ready versions only.
    ///
    /// # Returns
    ///
    /// Returns `Vec<VersionInfo>` containing only stable versions, sorted in descending
    /// order. The vector may be empty if no stable versions exist (only prereleases).
    ///
    /// # Filtering Criteria
    ///
    /// A version is included if:
    /// - Its prerelease component is empty (no `-alpha`, `-beta`, `-rc` suffixes)
    /// - It parses as a valid semantic version
    /// - It was successfully extracted from a Git tag
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let stable_versions = resolver.list_stable();
    /// println!("Found {} stable versions:", stable_versions.len());
    ///
    /// for version in stable_versions {
    ///     println!("  {} ({})", version.tag, version.version);
    ///     assert!(!version.prerelease); // Guaranteed to be false
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Comparison with All Versions
    ///
    /// ```rust,no_run
    /// # use agpm_cli::version::VersionResolver;
    /// # use agpm_cli::git::GitRepo;
    /// # use std::path::PathBuf;
    /// #
    /// # async fn example() -> anyhow::Result<()> {
    /// # let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// # let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let all_versions = resolver.list_all();
    /// let stable_versions = resolver.list_stable();
    ///
    /// println!("Total versions: {}", all_versions.len());
    /// println!("Stable versions: {}", stable_versions.len());
    /// println!("Prerelease versions: {}", all_versions.len() - stable_versions.len());
    ///
    /// if stable_versions.len() < all_versions.len() {
    ///     println!("Repository contains prerelease versions");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is particularly useful for:
    /// - Production environment version selection
    /// - Conservative update strategies
    /// - Compliance requirements that exclude prereleases
    /// - User interfaces that hide development versions by default
    #[must_use]
    pub fn list_stable(&self) -> Vec<Arc<VersionInfo>> {
        self.versions.iter().filter(|v| !v.prerelease).map(Arc::clone).collect()
    }

    /// Check if a specific version constraint can be resolved.
    ///
    /// This method tests whether a given version constraint string can be successfully
    /// resolved against the available versions in this resolver. It's a convenience
    /// method that combines resolution and existence checking.
    ///
    /// # Arguments
    ///
    /// * `version` - The version constraint string to test
    ///
    /// # Returns
    ///
    /// Returns `true` if the version constraint resolves to an actual version,
    /// `false` if no matching version is found or if resolution fails.
    ///
    /// # Resolution Types Tested
    ///
    /// This method can verify existence of:
    /// - **Exact versions**: `"1.0.0"`, `"v1.2.3"`
    /// - **Version ranges**: `"^1.0.0"`, `"~1.2.0"`, `">=1.0.0"`
    /// - **Tag/branch names**: Exact Git tag or branch matches (including "latest")
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionResolver;
    /// use agpm_cli::git::GitRepo;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// // Check if specific versions exist
    /// if resolver.has_version("1.0.0") {
    ///     println!("Version 1.0.0 is available");
    /// }
    ///
    /// if resolver.has_version("^1.0.0") {
    ///     println!("Compatible versions with 1.0.0 exist");
    /// }
    ///
    /// // This will likely return false unless you have this exact tag
    /// if resolver.has_version("v99.99.99") {
    ///     println!("Unlikely version found!");
    /// } else {
    ///     println!("Version 99.99.99 not found (as expected)");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Validation Before Resolution
    ///
    /// ```rust,no_run
    /// # use agpm_cli::version::VersionResolver;
    /// # use agpm_cli::git::GitRepo;
    /// # use std::path::PathBuf;
    /// #
    /// # async fn example() -> anyhow::Result<()> {
    /// # let repo = GitRepo::new(PathBuf::from("/path/to/repo"));
    /// # let resolver = VersionResolver::from_git_tags(&repo).await?;
    ///
    /// let constraint = "^2.0.0";
    ///
    /// if resolver.has_version(constraint) {
    ///     // Safe to resolve - we know it will succeed
    ///     let version = resolver.resolve(constraint)?.unwrap();
    ///     println!("Resolved {} to {}", constraint, version.tag);
    /// } else {
    ///     println!("No versions satisfy constraint: {}", constraint);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// This method handles resolution errors gracefully by returning `false` rather
    /// than propagating errors. This makes it safe to use for validation without
    /// extensive error handling.
    ///
    /// # Use Cases
    ///
    /// This method is useful for:
    /// - Validating user input before processing
    /// - Pre-flight checks in dependency resolution
    /// - Conditional logic based on version availability
    /// - User interface validation and feedback
    #[must_use]
    pub fn has_version(&self, version: &str) -> bool {
        self.resolve(version).unwrap_or(None).is_some()
    }
}

impl Default for VersionResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a version string satisfies a version requirement.
///
/// This utility function provides standalone version matching without requiring
/// a [`VersionResolver`] instance. It supports semantic version requirements and
/// special keywords for direct version-to-requirement comparison.
///
/// # Arguments
///
/// * `version` - The version string to test (supports `v` prefixes)
/// * `requirement` - The requirement string to match against
///
/// # Returns
///
/// Returns `Ok(true)` if the version satisfies the requirement, `Ok(false)` if it
/// doesn't match, or `Err` for invalid version/requirement strings.
///
/// # Supported Requirements
///
/// - **Special keywords**: `"*"` (wildcard, always matches)
/// - **Exact versions**: `"1.0.0"` (must match exactly)
/// - **Caret ranges**: `"^1.0.0"` (compatible within major version)
/// - **Tilde ranges**: `"~1.2.0"` (compatible within minor version)
/// - **Comparison ranges**: `">=1.0.0"`, `"<2.0.0"`
/// - **Complex ranges**: `">=1.0.0, <2.0.0"` (multiple constraints)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::version::matches_requirement;
///
/// # fn example() -> anyhow::Result<()> {
/// // Exact version matching
/// assert!(matches_requirement("1.0.0", "1.0.0")?);
/// assert!(matches_requirement("v1.0.0", "1.0.0")?); // v prefix ignored
/// assert!(!matches_requirement("1.0.1", "1.0.0")?);
///
/// // Caret range matching (compatible within major version)
/// assert!(matches_requirement("1.2.3", "^1.0.0")?);
/// assert!(matches_requirement("1.9.9", "^1.0.0")?);
/// assert!(!matches_requirement("2.0.0", "^1.0.0")?); // Major version change
///
/// // Tilde range matching (compatible within minor version)
/// assert!(matches_requirement("1.2.5", "~1.2.0")?);
/// assert!(!matches_requirement("1.3.0", "~1.2.0")?); // Minor version change
///
/// // Comparison ranges
/// assert!(matches_requirement("1.5.0", ">=1.0.0")?);
/// assert!(!matches_requirement("0.9.0", ">=1.0.0")?);
///
/// // Wildcard
/// assert!(matches_requirement("any.version", "*")?);
/// # Ok(())
/// # }
/// ```
///
/// ## Complex Range Matching
///
/// ```rust,no_run
/// use agpm_cli::version::matches_requirement;
///
/// # fn example() -> anyhow::Result<()> {
/// // Multiple constraints
/// assert!(matches_requirement("1.5.0", ">=1.0.0, <2.0.0")?);
/// assert!(!matches_requirement("2.0.0", ">=1.0.0, <2.0.0")?);
///
/// // Pre-release handling
/// assert!(matches_requirement("1.0.0-beta.1", "^1.0.0-beta")?);
/// # Ok(())
/// # }
/// ```
///
/// # Version Prefix Handling
///
/// The function handles both namespace prefixes and `v` prefixes:
/// - `"v1.0.0"` is treated as `"1.0.0"`
/// - `"V2.1.3"` is treated as `"2.1.3"`
/// - `"agents-v1.2.0"` requires `"agents-^v1.0.0"` (prefixes must match)
/// - Unprefixed versions don't match prefixed requirements and vice versa
///
/// # Error Cases
///
/// This function returns errors for:
/// - Invalid semantic version strings
/// - Malformed requirement syntax
/// - Unparseable version ranges
///
/// # Use Cases
///
/// This function is useful for:
/// - Quick version compatibility checks
/// - Input validation in CLI tools
/// - Testing version constraints programmatically
/// - Implementing custom version resolution logic
pub fn matches_requirement(version: &str, requirement: &str) -> Result<bool> {
    // Extract prefixes from both version and requirement
    let (version_prefix, version_str) = split_prefix_and_version(version);
    let (req_prefix, req_str) = split_prefix_and_version(requirement);

    // Ensure prefixes match (both None, or both Some with same value)
    if version_prefix != req_prefix {
        return Ok(false);
    }

    // Handle wildcard in the version portion
    if req_str == "*" {
        return Ok(true);
    }

    // Parse version (strip v prefix from version portion)
    let version = Version::parse(version_str.trim_start_matches('v'))?;

    // Parse requirement (with v-prefix normalization)
    let req = parse_version_req(req_str)
        .map_err(|e| anyhow::anyhow!("Invalid version requirement '{requirement}': {e}"))?;

    Ok(req.matches(&version))
}

/// Parse a version constraint string into a structured constraint type.
///
/// This function analyzes a constraint string and determines whether it represents
/// a Git commit hash, a version/tag specification, or a branch name. It provides
/// a simple classification system for different types of version references.
///
/// # Classification Logic
///
/// The function uses heuristics to determine constraint types:
/// 1. **Commit hashes**: 7+ hexadecimal characters (e.g., `"abc123def"`)
/// 2. **Version/tag specs**: Valid semantic versions or requirements (e.g., `"^1.0.0"`, `"*"`)
/// 3. **Branch names**: Everything else (e.g., `"main"`, `"latest"`, `"feature/auth"`)
///
/// # Arguments
///
/// * `constraint` - The constraint string to parse and classify
///
/// # Returns
///
/// Returns a [`VersionConstraint`] enum variant indicating the constraint type:
/// - [`VersionConstraint::Commit`] for Git commit hashes
/// - [`VersionConstraint::Tag`] for semantic versions and requirements
/// - [`VersionConstraint::Branch`] for Git branch names
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::version::{parse_version_constraint, VersionConstraint};
///
/// // Semantic versions are classified as tags
/// let constraint = parse_version_constraint("1.0.0");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// let constraint = parse_version_constraint("v1.2.3");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// // Prefixed versions and constraints are also classified as tags
/// let constraint = parse_version_constraint("agents-v1.2.0");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// let constraint = parse_version_constraint("agents-^v1.0.0");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// // Version requirements are classified as tags
/// let constraint = parse_version_constraint("^1.0.0");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// let constraint = parse_version_constraint("*");
/// assert!(matches!(constraint, VersionConstraint::Tag(_)));
///
/// // Commit hashes are detected by hex pattern
/// let constraint = parse_version_constraint("abc1234");
/// assert!(matches!(constraint, VersionConstraint::Commit(_)));
///
/// let constraint = parse_version_constraint("1234567890abcdef1234567890abcdef12345678");
/// assert!(matches!(constraint, VersionConstraint::Commit(_)));
///
/// // Branch names are the fallback
/// let constraint = parse_version_constraint("main");
/// assert!(matches!(constraint, VersionConstraint::Branch(_)));
///
/// let constraint = parse_version_constraint("feature/auth-system");
/// assert!(matches!(constraint, VersionConstraint::Branch(_)));
/// ```
///
/// # Commit Hash Detection
///
/// The function identifies commit hashes using these criteria:
/// - Minimum 7 characters (Git's default abbreviation length)
/// - All characters must be hexadecimal (0-9, a-f, A-F)
/// - No maximum length (supports full 40-character SHA-1 hashes)
///
/// # Version/Tag Detection
///
/// Version and tag specifications are identified by:
/// - Valid semantic version parsing (with or without `v` prefix)
/// - Valid semantic version requirement parsing (ranges, comparisons)
/// - Wildcard `"*"` for any version
///
/// # Branch Name Fallback
///
/// Any string that doesn't match the above patterns is treated as a branch name:
/// - Simple names: `"main"`, `"develop"`, `"staging"`, `"latest"`
/// - Namespaced branches: `"feature/new-ui"`, `"bugfix/auth-issue"`
/// - Special characters: `"release/v1.0"`, `"user/name/branch"`
///
/// # Use Cases
///
/// This function is useful for:
/// - Parsing user input in dependency specifications
/// - Routing version resolution to appropriate handlers
/// - Validating constraint syntax in configuration files
/// - Building version constraint objects from strings
#[must_use]
pub fn parse_version_constraint(constraint: &str) -> VersionConstraint {
    // Check if it looks like a commit hash (40 hex chars or abbreviated)
    if constraint.len() >= 7 && constraint.chars().all(|c| c.is_ascii_hexdigit()) {
        return VersionConstraint::Commit(constraint.to_string());
    }

    // Extract prefix to check the version portion
    let (_prefix, version_str) = split_prefix_and_version(constraint);

    // Check if the version portion is a semantic version or version requirement
    if Version::parse(version_str.trim_start_matches('v')).is_ok()
        || parse_version_req(version_str).is_ok()
        || version_str == "*"
    {
        return VersionConstraint::Tag(constraint.to_string());
    }

    // Otherwise treat as branch
    VersionConstraint::Branch(constraint.to_string())
}

/// Version comparison utilities and analysis functions.
///
/// The [`comparison`] module provides tools for analyzing and comparing semantic
/// versions, finding newer versions, and determining latest releases from version
/// collections. See the module documentation for detailed usage examples.
pub mod comparison;

/// Version conflict detection and circular dependency detection.
///
/// The [`conflict`] module provides sophisticated conflict analysis for version
/// requirements, detecting incompatible version constraints and circular dependencies
/// in the dependency graph.
pub mod conflict;

/// Version constraint parsing, sets, and resolution system.
///
/// The [`constraints`] module contains the core constraint management system for
/// AGPM, including constraint parsing, conflict detection, and multi-dependency
/// resolution. See the module documentation for comprehensive examples.
pub mod constraints;

/// Represents different types of version constraints in AGPM.
///
/// `VersionConstraint` is a simple enum that categorizes version references into
/// three main types: Git tags (including semantic versions), Git branches, and
/// Git commit hashes. This classification helps AGPM route version resolution
/// to the appropriate handling logic.
///
/// # Variants
///
/// - [`Tag`](Self::Tag): Semantic versions, version requirements, and Git tags
/// - [`Branch`](Self::Branch): Git branch names and references
/// - [`Commit`](Self::Commit): Git commit hashes (full or abbreviated)
///
/// # Serialization
///
/// This enum implements [`Serialize`] and [`Deserialize`] for use in configuration
/// files and lockfiles, allowing version constraints to be persisted and restored.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::version::VersionConstraint;
///
/// // Create different constraint types
/// let version = VersionConstraint::Tag("1.0.0".to_string());
/// let branch = VersionConstraint::Branch("main".to_string());
/// let commit = VersionConstraint::Commit("abc123def".to_string());
///
/// // Access the string value
/// assert_eq!(version.as_str(), "1.0.0");
/// assert_eq!(branch.as_str(), "main");
/// assert_eq!(commit.as_str(), "abc123def");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionConstraint {
    /// A semantic version tag (e.g., "v1.2.0", "1.0.0")
    Tag(String),
    /// A git branch reference (e.g., "main", "develop", "feature/new")
    Branch(String),
    /// A specific git commit hash (full or abbreviated)
    Commit(String),
}

impl VersionConstraint {
    /// Get the string representation of this constraint.
    ///
    /// This method extracts the underlying string value from any constraint variant,
    /// providing a uniform way to access the constraint specification regardless
    /// of its type classification.
    ///
    /// # Returns
    ///
    /// Returns `&str` containing the original constraint string.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::VersionConstraint;
    ///
    /// let tag = VersionConstraint::Tag("^1.0.0".to_string());
    /// assert_eq!(tag.as_str(), "^1.0.0");
    ///
    /// let branch = VersionConstraint::Branch("feature/auth".to_string());
    /// assert_eq!(branch.as_str(), "feature/auth");
    ///
    /// let commit = VersionConstraint::Commit("abc123def456".to_string());
    /// assert_eq!(commit.as_str(), "abc123def456");
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful for:
    /// - Displaying constraints in user interfaces
    /// - Logging and debugging version resolution
    /// - Passing constraint strings to external tools
    /// - Serializing constraints to text formats
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Tag(s) => s,
            Self::Branch(s) => s,
            Self::Commit(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestGit;
    use tempfile::TempDir;

    fn create_test_repo_with_tags() -> (TempDir, GitRepo) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Use TestGit helper instead of raw Command
        let git = TestGit::new(repo_path);
        git.init().unwrap();
        git.config_user().unwrap();

        std::fs::write(repo_path.join("README.md"), "Test").unwrap();

        git.add_all().unwrap();
        git.commit("Initial commit").unwrap();

        let tags = vec!["v1.0.0", "v1.1.0", "v1.2.0", "v2.0.0-beta.1", "v2.0.0"];
        for tag in tags {
            git.tag(tag).unwrap();
        }

        let repo = GitRepo::new(repo_path);
        (temp_dir, repo)
    }

    #[tokio::test]
    async fn test_version_parsing() {
        let (_temp, repo) = create_test_repo_with_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        assert_eq!(resolver.versions.len(), 5);
        assert_eq!(resolver.get_latest().unwrap().tag, "v2.0.0");
        assert_eq!(resolver.get_latest_stable().unwrap().tag, "v2.0.0");
    }

    #[tokio::test]
    async fn test_version_resolution() {
        let (_temp, repo) = create_test_repo_with_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        // Exact versions
        assert_eq!(resolver.resolve("1.1.0").unwrap().unwrap().tag, "v1.1.0");
        assert_eq!(resolver.resolve("v1.1.0").unwrap().unwrap().tag, "v1.1.0");

        // Version constraints
        assert_eq!(resolver.resolve("^1.0.0").unwrap().unwrap().tag, "v1.2.0");
        assert_eq!(resolver.resolve("~1.1.0").unwrap().unwrap().tag, "v1.1.0");
    }

    #[tokio::test]
    async fn test_has_version() {
        let (_temp, repo) = create_test_repo_with_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        // Exact versions
        assert!(resolver.has_version("v1.0.0"));
        assert!(resolver.has_version("1.0.0"));

        // Non-existent versions (including "latest" - just a tag name)
        assert!(!resolver.has_version("v3.0.0"));
        assert!(!resolver.has_version("latest"));
        assert!(!resolver.has_version("latest-prerelease"));
    }

    #[tokio::test]
    async fn test_matches_requirement() {
        // Unprefixed versions
        assert!(matches_requirement("1.2.0", "^1.0.0").unwrap());
        assert!(matches_requirement("v1.2.0", "^1.0.0").unwrap());
        assert!(!matches_requirement("2.0.0", "^1.0.0").unwrap());
        assert!(matches_requirement("any.version", "*").unwrap());
    }

    #[test]
    fn test_matches_requirement_with_prefixes() {
        // Prefixed versions with matching prefixes
        assert!(matches_requirement("agents-v1.2.0", "agents-^v1.0.0").unwrap());
        assert!(matches_requirement("agents-v1.2.5", "agents-~v1.2.0").unwrap());
        assert!(matches_requirement("tool123-v2.0.0", "tool123->=v1.0.0").unwrap());

        // Prefixed versions with wildcard
        assert!(matches_requirement("agents-v1.2.0", "agents-*").unwrap());

        // Prefixed version doesn't match unprefixed requirement
        assert!(!matches_requirement("agents-v1.2.0", "^v1.0.0").unwrap());

        // Unprefixed version doesn't match prefixed requirement
        assert!(!matches_requirement("v1.2.0", "agents-^v1.0.0").unwrap());

        // Different prefixes don't match
        assert!(!matches_requirement("agents-v1.2.0", "snippets-^v1.0.0").unwrap());
        assert!(!matches_requirement("tool-v1.0.0", "agent-v1.0.0").unwrap());

        // Prefixed version doesn't satisfy constraint
        assert!(!matches_requirement("agents-v2.0.0", "agents-^v1.0.0").unwrap());
    }

    #[test]
    fn test_parse_version_constraint() {
        // Unprefixed constraints
        assert_eq!(
            parse_version_constraint("v1.0.0"),
            VersionConstraint::Tag("v1.0.0".to_string())
        );
        assert_eq!(
            parse_version_constraint("^1.0.0"),
            VersionConstraint::Tag("^1.0.0".to_string())
        );
        assert_eq!(parse_version_constraint("*"), VersionConstraint::Tag("*".to_string()));
        assert_eq!(parse_version_constraint("main"), VersionConstraint::Branch("main".to_string()));
        assert_eq!(
            parse_version_constraint("latest"),
            VersionConstraint::Branch("latest".to_string())
        );
        assert_eq!(
            parse_version_constraint("latest-prerelease"),
            VersionConstraint::Branch("latest-prerelease".to_string())
        );
        assert_eq!(
            parse_version_constraint("feature/test"),
            VersionConstraint::Branch("feature/test".to_string())
        );
        assert_eq!(
            parse_version_constraint("abc1234"),
            VersionConstraint::Commit("abc1234".to_string())
        );
        assert_eq!(
            parse_version_constraint("1234567890abcdef"),
            VersionConstraint::Commit("1234567890abcdef".to_string())
        );

        // Prefixed constraints - all should be Tag
        assert_eq!(
            parse_version_constraint("agents-v1.2.0"),
            VersionConstraint::Tag("agents-v1.2.0".to_string())
        );
        assert_eq!(
            parse_version_constraint("agents-^v1.0.0"),
            VersionConstraint::Tag("agents-^v1.0.0".to_string())
        );
        assert_eq!(
            parse_version_constraint("snippets-~v2.0.0"),
            VersionConstraint::Tag("snippets-~v2.0.0".to_string())
        );
        assert_eq!(
            parse_version_constraint("tool123-*"),
            VersionConstraint::Tag("tool123-*".to_string())
        );
        assert_eq!(
            parse_version_constraint("my-cool-tool->=v1.0.0"),
            VersionConstraint::Tag("my-cool-tool->=v1.0.0".to_string())
        );

        // Prefixed branches should still be Branch
        assert_eq!(
            parse_version_constraint("agents-main"),
            VersionConstraint::Branch("agents-main".to_string())
        );
    }

    #[tokio::test]
    async fn test_version_list_all() {
        let (_temp, repo) = create_test_repo_with_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        let all_versions = resolver.list_all();
        assert_eq!(all_versions.len(), 5);

        // Should be sorted in descending order
        assert_eq!(all_versions[0].tag, "v2.0.0");
        assert_eq!(all_versions[1].tag, "v2.0.0-beta.1");
    }

    #[tokio::test]
    async fn test_version_list_stable() {
        let (_temp, repo) = create_test_repo_with_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        let stable_versions = resolver.list_stable();
        assert_eq!(stable_versions.len(), 4); // No beta versions

        for version in stable_versions {
            assert!(!version.prerelease);
        }
    }

    // ========== Prefix Support Tests ==========

    #[test]
    fn test_split_prefix_and_version() {
        // Prefixed versions
        assert_eq!(
            split_prefix_and_version("agents-v1.0.0"),
            (Some("agents".to_string()), "v1.0.0")
        );
        assert_eq!(
            split_prefix_and_version("agents-^v1.0.0"),
            (Some("agents".to_string()), "^v1.0.0")
        );
        assert_eq!(
            split_prefix_and_version("my-cool-agent-v2.0.0"),
            (Some("my-cool-agent".to_string()), "v2.0.0")
        );

        // Unprefixed versions
        assert_eq!(split_prefix_and_version("v1.0.0"), (None, "v1.0.0"));
        assert_eq!(split_prefix_and_version("^v1.0.0"), (None, "^v1.0.0"));
        assert_eq!(split_prefix_and_version("1.0.0"), (None, "1.0.0"));

        // Edge cases
        assert_eq!(
            split_prefix_and_version("tool-v-v1.0.0"),
            (Some("tool-v".to_string()), "v1.0.0")
        );
        assert_eq!(split_prefix_and_version("a-b-c-v1.0.0"), (Some("a-b-c".to_string()), "v1.0.0"));
        assert_eq!(
            split_prefix_and_version("prefix-~1.0.0"),
            (Some("prefix".to_string()), "~1.0.0")
        );
    }

    #[test]
    fn test_split_prefix_edge_cases() {
        // Empty prefix - should be treated as None
        assert_eq!(split_prefix_and_version("-v1.0.0"), (None, "v1.0.0"));
        assert_eq!(split_prefix_and_version("--v1.0.0"), (None, "v1.0.0"));

        // Prefix with numbers - digits in middle of prefix are preserved
        assert_eq!(
            split_prefix_and_version("tool123-v1.0.0"),
            (Some("tool123".to_string()), "v1.0.0")
        );
        assert_eq!(
            split_prefix_and_version("agent2-v1.0.0"),
            (Some("agent2".to_string()), "v1.0.0")
        );
        // Digit after hyphen is treated as version start
        assert_eq!(split_prefix_and_version("tool-123"), (Some("tool".to_string()), "123"));
        // 'v' followed by digit takes precedence
        assert_eq!(
            split_prefix_and_version("abc-v2-agent-v1.0.0"),
            (Some("abc".to_string()), "v2-agent-v1.0.0")
        );

        // Very long prefix (stress test)
        let long_prefix = "a".repeat(100);
        let tag = format!("{}-v1.0.0", long_prefix);
        let (prefix, version) = split_prefix_and_version(&tag);
        assert_eq!(prefix, Some(long_prefix));
        assert_eq!(version, "v1.0.0");

        // Unicode in prefixes - note: 'v' followed by digit is detected as version
        assert_eq!(
            split_prefix_and_version("агенты-v1.0.0"),
            (Some("агенты".to_string()), "v1.0.0")
        );
        assert_eq!(split_prefix_and_version("工具-v1.0.0"), (Some("工具".to_string()), "v1.0.0"));
        // Unicode prefixes without version pattern
        assert_eq!(split_prefix_and_version("агенты-2.0.0"), (Some("агенты".to_string()), "2.0.0"));

        // String ending with 'v' (tests panic fix)
        assert_eq!(split_prefix_and_version("prefix-v"), (None, "prefix-v"));
        assert_eq!(split_prefix_and_version("v"), (None, "v"));

        // Multiple hyphens
        assert_eq!(
            split_prefix_and_version("my-cool-tool-v1.0.0"),
            (Some("my-cool-tool".to_string()), "v1.0.0")
        );
    }

    fn create_test_repo_with_prefixed_tags() -> (TempDir, GitRepo) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Use TestGit helper instead of raw Command
        let git = TestGit::new(repo_path);
        git.init().unwrap();
        git.config_user().unwrap();

        std::fs::write(repo_path.join("README.md"), "Test").unwrap();

        git.add_all().unwrap();
        git.commit("Initial commit").unwrap();

        // Create tags with different prefixes
        let tags = vec![
            "agents-v1.0.0",
            "agents-v1.2.0",
            "agents-v2.0.0",
            "snippets-v1.0.0",
            "snippets-v1.5.0",
            "v1.0.0", // Unprefixed
            "v2.0.0", // Unprefixed
        ];
        for tag in tags {
            git.tag(tag).unwrap();
        }

        let repo = GitRepo::new(repo_path);
        (temp_dir, repo)
    }

    #[tokio::test]
    async fn test_prefixed_version_parsing() {
        let (_temp, repo) = create_test_repo_with_prefixed_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        // Should parse all 7 tags
        assert_eq!(resolver.versions.len(), 7);

        // Check prefixes are correctly extracted
        let agents_versions: Vec<_> =
            resolver.versions.iter().filter(|v| v.prefix == Some("agents".to_string())).collect();
        assert_eq!(agents_versions.len(), 3);

        let snippets_versions: Vec<_> =
            resolver.versions.iter().filter(|v| v.prefix == Some("snippets".to_string())).collect();
        assert_eq!(snippets_versions.len(), 2);

        let unprefixed_versions: Vec<_> =
            resolver.versions.iter().filter(|v| v.prefix.is_none()).collect();
        assert_eq!(unprefixed_versions.len(), 2);
    }

    #[tokio::test]
    async fn test_prefixed_version_resolution() {
        let (_temp, repo) = create_test_repo_with_prefixed_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        // Prefixed exact version
        let result = resolver.resolve("agents-v1.2.0").unwrap().unwrap();
        assert_eq!(result.tag, "agents-v1.2.0");
        assert_eq!(result.prefix, Some("agents".to_string()));

        // Prefixed constraint - should match highest agents version
        let result = resolver.resolve("agents-^v1.0.0").unwrap().unwrap();
        assert_eq!(result.tag, "agents-v1.2.0");
        assert_eq!(result.prefix, Some("agents".to_string()));

        // Different prefix constraint
        let result = resolver.resolve("snippets-^v1.0.0").unwrap().unwrap();
        assert_eq!(result.tag, "snippets-v1.5.0");
        assert_eq!(result.prefix, Some("snippets".to_string()));

        // Unprefixed constraint should only match unprefixed tags
        let result = resolver.resolve("^v1.0.0").unwrap().unwrap();
        assert_eq!(result.tag, "v1.0.0");
        assert_eq!(result.prefix, None);
    }

    #[tokio::test]
    async fn test_prefix_isolation() {
        let (_temp, repo) = create_test_repo_with_prefixed_tags();
        let resolver = VersionResolver::from_git_tags(&repo).await.unwrap();

        // agents-^v1.0.0 should NOT match snippets-v1.5.0 even though 1.5.0 > 1.0.0
        let result = resolver.resolve("agents-^v1.0.0").unwrap().unwrap();
        assert_eq!(result.prefix, Some("agents".to_string()));
        assert_ne!(result.tag, "snippets-v1.5.0");

        // Unprefixed constraint should NOT match prefixed tags
        let result = resolver.resolve("^v1.0.0").unwrap().unwrap();
        assert_eq!(result.prefix, None);
        assert!(!result.tag.contains("agents-"));
        assert!(!result.tag.contains("snippets-"));
    }

    #[test]
    fn test_parse_version_req_with_prefix() -> anyhow::Result<()> {
        // The parse_version_req function should work on the version part only
        parse_version_req("^1.0.0")?;
        parse_version_req("^v1.0.0")?;
        parse_version_req("~2.1.0")?;
        parse_version_req(">=1.0.0")?;
        Ok(())
    }
}
