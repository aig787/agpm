//! Version constraint parsing and resolution for AGPM dependencies.
//!
//! This module provides comprehensive version constraint handling for AGPM dependencies,
//! supporting semantic versioning, Git references, and various constraint types. It enables
//! dependency resolution with conflict detection and version matching.
//!
//! # Version Constraint Types
//!
//! AGPM supports several types of version constraints:
//!
//! - **Exact versions**: `"1.0.0"` - Matches exactly the specified version
//! - **Semantic version ranges**: `"^1.0.0"`, `"~1.2.0"`, `">=1.0.0"` - Uses semver ranges
//! - **Git references**: `"main"`, `"feature/branch"`, `"abc123"`, `"latest"` - Git branches, tags, or commits
//!
//! # Constraint Resolution
//!
//! The constraint system provides:
//! - **Conflict detection**: Prevents incompatible constraints for the same dependency
//! - **Version resolution**: Finds best matching versions from available options
//! - **Prerelease handling**: Manages alpha, beta, RC versions appropriately
//! - **Precedence rules**: Resolves multiple constraints consistently
//!
//! # Examples
//!
//! ## Basic Constraint Parsing
//!
//! ```rust,no_run
//! use agpm_cli::version::constraints::VersionConstraint;
//!
//! // Parse different constraint types
//! let exact = VersionConstraint::parse("1.0.0")?;
//! let caret = VersionConstraint::parse("^1.0.0")?;
//! let latest = VersionConstraint::parse("latest")?;
//! let branch = VersionConstraint::parse("main")?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Constraint Set Management
//!
//! ```rust,no_run
//! use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
//! use semver::Version;
//!
//! let mut set = ConstraintSet::new();
//! set.add(VersionConstraint::parse(">=1.0.0")?)?;
//! set.add(VersionConstraint::parse("<2.0.0")?)?;
//!
//! let versions = vec![
//!     Version::parse("0.9.0")?,
//!     Version::parse("1.5.0")?,
//!     Version::parse("2.0.0")?,
//! ];
//!
//! let best = set.find_best_match(&versions).unwrap();
//! assert_eq!(best, &Version::parse("1.5.0")?);
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Dependency Resolution
//!
//! ```rust,no_run
//! use agpm_cli::version::constraints::ConstraintResolver;
//! use semver::Version;
//! use std::collections::HashMap;
//!
//! let mut resolver = ConstraintResolver::new();
//! resolver.add_constraint("dep1", "^1.0.0")?;
//! resolver.add_constraint("dep2", "~2.1.0")?;
//!
//! let mut available = HashMap::new();
//! available.insert("dep1".to_string(), vec![Version::parse("1.5.0")?]);
//! available.insert("dep2".to_string(), vec![Version::parse("2.1.3")?]);
//!
//! let resolved = resolver.resolve(&available)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Constraint Syntax Reference
//!
//! | Syntax | Description | Example |
//! |--------|-------------|----------|
//! | `1.0.0` | Exact version | `"1.0.0"` |
//! | `^1.0.0` | Compatible within major version | `"^1.0.0"` matches `1.x.x` |
//! | `~1.2.0` | Compatible within minor version | `"~1.2.0"` matches `1.2.x` |
//! | `>=1.0.0` | Greater than or equal | `">=1.0.0"` |
//! | `<2.0.0` | Less than | `"<2.0.0"` |
//! | `>=1.0.0, <2.0.0` | Range constraint | Multiple constraints |
//! | `main` | Git branch reference | Branch name |
//! | `latest` | Git tag or branch name | Just a regular ref |
//! | `v1.0.0` | Git tag reference | Tag name |
//! | `abc123` | Git commit reference | Commit hash (full or abbreviated) |
//!
//! # Version Resolution Precedence
//!
//! When resolving versions, AGPM follows this precedence:
//!
//! 1. **Exact matches** take highest priority
//! 2. **Semantic version requirements** are resolved to highest compatible version
//! 3. **Stable versions** are preferred over prereleases (unless explicitly allowed)
//! 4. **Newer versions** are preferred when multiple versions satisfy constraints
//! 5. **Git references** bypass semantic versioning and use exact ref matching
//!
//! # Prerelease Version Handling
//!
//! - **Default behavior**: Prereleases (alpha, beta, RC) are excluded from resolution
//! - **Explicit inclusion**: Use Git references to include prereleases
//! - **Version ranges**: Prereleases only match if explicitly specified in range
//! - **Constraint sets**: If any constraint allows prereleases, the entire set does
//!
//! # Error Conditions
//!
//! The constraint system handles these error conditions:
//! - **Conflicting constraints**: Same dependency with incompatible requirements
//! - **Invalid version strings**: Malformed semantic version specifications
//! - **Resolution failures**: No available version satisfies all constraints
//! - **Missing dependencies**: Required dependency not found in available versions

use anyhow::Result;
use semver::{Version, VersionReq};
use std::fmt;

pub mod constraint_set;
pub mod resolver;

pub use constraint_set::ConstraintSet;
pub use resolver::ConstraintResolver;

/// A version constraint that defines acceptable versions for a dependency.
///
/// Version constraints in AGPM support multiple formats to handle different
/// versioning strategies and Git-based dependencies. Each constraint type
/// provides specific matching behavior for version resolution.
///
/// # Constraint Types
///
/// - [`Exact`](Self::Exact): Matches exactly one specific semantic version
/// - [`Requirement`](Self::Requirement): Matches versions using semver ranges
/// - [`GitRef`](Self::GitRef): Matches specific Git branches, tags, or commit hashes (including "latest")
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::VersionConstraint;
/// use semver::Version;
///
/// // Parse various constraint formats
/// let exact = VersionConstraint::parse("1.0.0")?;
/// let caret = VersionConstraint::parse("^1.0.0")?; // Compatible versions
/// let tilde = VersionConstraint::parse("~1.2.0")?; // Patch-level compatible
/// let range = VersionConstraint::parse(">=1.0.0, <2.0.0")?; // Version range
/// let branch = VersionConstraint::parse("main")?;
/// let latest_tag = VersionConstraint::parse("latest")?; // Just a tag name
/// let commit = VersionConstraint::parse("abc123def")?;
///
/// // Test version matching
/// let version = Version::parse("1.2.3")?;
/// assert!(caret.matches(&version));
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Prerelease Handling
///
/// By default, most constraints exclude prerelease versions to ensure stability:
/// - `GitRef` constraints (including "latest" tag names) may reference any commit
///
/// # Git Reference Matching
///
/// Git references are matched by name rather than semantic version:
/// - Branch names: `"main"`, `"develop"`, `"feature/auth"`
/// - Tag names: `"v1.0.0"`, `"release-2023-01"`
/// - Commit hashes: `"abc123def456"` (full or abbreviated)
///
/// # Prefix Support (Monorepo Versioning)
///
/// Constraints can include optional prefixes for monorepo-style versioning:
/// - `"agents-v1.0.0"`: Exact version with prefix
/// - `"agents-^v1.0.0"`: Compatible version range with prefix
/// - Prefixed constraints only match tags with the same prefix
#[derive(Debug, Clone)]
pub enum VersionConstraint {
    /// Exact version match with optional prefix (e.g., "1.0.0", "agents-v1.0.0")
    Exact {
        prefix: Option<String>,
        version: Version,
    },

    /// Semantic version requirement with optional prefix (e.g., "^1.0.0", "agents-^v1.0.0")
    Requirement {
        prefix: Option<String>,
        req: VersionReq,
    },

    /// Git tag or branch name (including "latest" - it's just a tag name)
    GitRef(String),
}

impl VersionConstraint {
    /// Parse a constraint string into a [`VersionConstraint`].
    ///
    /// This method intelligently determines the constraint type based on the input format.
    /// It handles various syntaxes including semantic versions, version ranges, special
    /// keywords, and Git references.
    ///
    /// # Parsing Logic
    ///
    /// 1. **Special keywords**: `"*"` (wildcard for any version)
    /// 2. **Exact versions**: `"1.0.0"`, `"v1.0.0"` (without range operators)
    /// 3. **Version requirements**: `"^1.0.0"`, `"~1.2.0"`, `">=1.0.0"`, `"<2.0.0"`
    /// 4. **Git references**: Any string that doesn't match the above patterns (including "latest")
    ///
    /// # Arguments
    ///
    /// * `constraint` - The constraint string to parse (whitespace is trimmed)
    ///
    /// # Returns
    ///
    /// Returns `Ok(VersionConstraint)` on successful parsing, or `Err` if the
    /// semantic version parsing fails (Git references always succeed).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    ///
    /// // Exact version matching
    /// let exact = VersionConstraint::parse("1.0.0")?;
    /// let exact_with_v = VersionConstraint::parse("v1.0.0")?;
    ///
    /// // Semantic version ranges
    /// let caret = VersionConstraint::parse("^1.0.0")?;      // 1.x.x compatible
    /// let tilde = VersionConstraint::parse("~1.2.0")?;      // 1.2.x compatible
    /// let gte = VersionConstraint::parse(">=1.0.0")?;       // Greater or equal
    /// let range = VersionConstraint::parse(">1.0.0, <2.0.0")?; // Range
    ///
    /// // Special keywords
    /// let any = VersionConstraint::parse("*")?;             // Any version
    ///
    /// // Git references
    /// let branch = VersionConstraint::parse("main")?;       // Branch name
    /// let tag = VersionConstraint::parse("release-v1")?;    // Tag name
    /// let latest = VersionConstraint::parse("latest")?;     // Just a tag/branch name
    /// let commit = VersionConstraint::parse("abc123def")?;  // Commit hash
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Error Handling
    ///
    /// This method only returns errors for malformed semantic version strings.
    /// Git references and special keywords always parse successfully.
    pub fn parse(constraint: &str) -> Result<Self> {
        let trimmed = constraint.trim();

        // Extract prefix from constraint first (e.g., "agents-^v1.0.0" → (Some("agents"), "^v1.0.0"))
        let (prefix, version_str) = crate::version::split_prefix_and_version(trimmed);

        // Check for wildcard in the version portion (supports both "*" and "agents-*")
        if version_str == "*" {
            // Wildcard means any version - treat as a GitRef that matches everything
            return Ok(Self::GitRef(trimmed.to_string()));
        }

        // Try to parse as exact version (with or without 'v' prefix)
        let cleaned_version_str = version_str.strip_prefix('v').unwrap_or(version_str);
        if let Ok(version) = Version::parse(cleaned_version_str) {
            // Check if it's a range operator
            if !version_str.starts_with('^')
                && !version_str.starts_with('~')
                && !version_str.starts_with('>')
                && !version_str.starts_with('<')
                && !version_str.starts_with('=')
            {
                return Ok(Self::Exact {
                    prefix,
                    version,
                });
            }
        }

        // Try to parse as version requirement (with v-prefix normalization)
        match crate::version::parse_version_req(version_str) {
            Ok(req) => {
                return Ok(Self::Requirement {
                    prefix,
                    req,
                });
            }
            Err(e) => {
                // If it looks like a semver constraint but failed to parse, return error
                if version_str.starts_with('^')
                    || version_str.starts_with('~')
                    || version_str.starts_with('=')
                    || version_str.starts_with('>')
                    || version_str.starts_with('<')
                {
                    return Err(anyhow::anyhow!("Invalid semver constraint '{trimmed}': {e}"));
                }
                // Otherwise it might be a git ref, continue
            }
        }

        // Otherwise treat as git ref
        Ok(Self::GitRef(trimmed.to_string()))
    }

    /// Check if a semantic version satisfies this constraint.
    ///
    /// This method tests whether a given semantic version matches the requirements
    /// of this constraint. Different constraint types use different matching logic:
    ///
    /// - **Exact**: Version must match exactly
    /// - **Requirement**: Version must satisfy the semver range
    /// - **`GitRef`**: Never matches semantic versions (Git refs are matched separately)
    ///
    /// # Arguments
    ///
    /// * `version` - The semantic version to test against this constraint
    ///
    /// # Returns
    ///
    /// Returns `true` if the version satisfies the constraint, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    /// use semver::Version;
    ///
    /// let constraint = VersionConstraint::parse("^1.0.0")?;
    /// let version = Version::parse("1.2.3")?;
    ///
    /// assert!(constraint.matches(&version)); // 1.2.3 is compatible with ^1.0.0
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Note
    ///
    /// Git reference constraints always return `false` for this method since they
    /// operate on Git refs rather than semantic versions. Use [`matches_ref`](Self::matches_ref)
    /// to test Git reference matching.
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Exact {
                version: v,
                ..
            } => v == version,
            Self::Requirement {
                req,
                ..
            } => req.matches(version),
            Self::GitRef(_) => false, // Git refs don't match semver versions
        }
    }

    /// Check if a Git reference satisfies this constraint.
    ///
    /// This method tests whether a Git reference (branch, tag, or commit hash)
    /// matches a Git reference constraint. Only [`GitRef`](Self::GitRef) constraints
    /// can match Git references - all other constraint types return `false`.
    ///
    /// # Arguments
    ///
    /// * `git_ref` - The Git reference string to test (branch, tag, or commit)
    ///
    /// # Returns
    ///
    /// Returns `true` if this is a `GitRef` constraint with matching reference name,
    /// `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    ///
    /// let branch_constraint = VersionConstraint::parse("main")?;
    /// assert!(branch_constraint.matches_ref("main"));
    /// assert!(!branch_constraint.matches_ref("develop"));
    ///
    /// let version_constraint = VersionConstraint::parse("^1.0.0")?;
    /// assert!(!version_constraint.matches_ref("main")); // Version constraints don't match refs
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is primarily used during dependency resolution to match
    /// dependencies that specify Git branches, tags, or commit hashes rather
    /// than semantic versions.
    #[must_use]
    pub fn matches_ref(&self, git_ref: &str) -> bool {
        match self {
            Self::GitRef(ref_name) => ref_name == git_ref,
            _ => false,
        }
    }

    /// Check if a VersionInfo satisfies this constraint, including prefix matching.
    ///
    /// This method performs comprehensive matching that considers both the prefix
    /// (for monorepo-style versioning) and the semantic version. It's the preferred
    /// method for version resolution when working with potentially prefixed versions.
    ///
    /// # Matching Rules
    ///
    /// - **Prefix matching**: Constraint and version must have the same prefix (both None, or same String)
    /// - **Version matching**: After prefix check, applies standard semver matching rules
    /// - **Prerelease handling**: Follows same rules as [`matches`](Self::matches)
    ///
    /// # Arguments
    ///
    /// * `version_info` - The version information to test, including prefix and semver
    ///
    /// # Returns
    ///
    /// Returns `true` if both the prefix matches AND the version satisfies the constraint.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    /// use agpm_cli::version::VersionInfo;
    /// use semver::Version;
    ///
    /// // Prefixed version matching
    /// let constraint = VersionConstraint::parse("agents-^v1.0.0")?;
    /// let version = VersionInfo {
    ///     prefix: Some("agents".to_string()),
    ///     version: Version::parse("1.2.0")?,
    ///     tag: "agents-v1.2.0".to_string(),
    ///     prerelease: false,
    /// };
    /// assert!(constraint.matches_version_info(&version));
    ///
    /// // Prefix mismatch
    /// let wrong_prefix = VersionInfo {
    ///     prefix: Some("snippets".to_string()),
    ///     version: Version::parse("1.2.0")?,
    ///     tag: "snippets-v1.2.0".to_string(),
    ///     prerelease: false,
    /// };
    /// assert!(!constraint.matches_version_info(&wrong_prefix));
    ///
    /// // Unprefixed constraint only matches unprefixed versions
    /// let no_prefix_constraint = VersionConstraint::parse("^1.0.0")?;
    /// let no_prefix_version = VersionInfo {
    ///     prefix: None,
    ///     version: Version::parse("1.2.0")?,
    ///     tag: "v1.2.0".to_string(),
    ///     prerelease: false,
    /// };
    /// assert!(no_prefix_constraint.matches_version_info(&no_prefix_version));
    /// assert!(!no_prefix_constraint.matches_version_info(&version)); // Has prefix
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn matches_version_info(&self, version_info: &crate::version::VersionInfo) -> bool {
        // Check prefix first
        let constraint_prefix = match self {
            Self::Exact {
                prefix,
                ..
            }
            | Self::Requirement {
                prefix,
                ..
            } => prefix.as_ref(),
            _ => None,
        };

        // Prefix must match (both None or both Some with same value)
        if constraint_prefix != version_info.prefix.as_ref() {
            return false;
        }

        // Then check version using existing logic
        self.matches(&version_info.version)
    }

    /// Convert this constraint to a semantic version requirement if applicable.
    ///
    /// This method converts version-based constraints into [`VersionReq`] objects
    /// that can be used with the semver crate for version matching. Git reference
    /// constraints cannot be converted since they don't represent version ranges.
    ///
    /// # Returns
    ///
    /// Returns `Some(VersionReq)` for constraints that can be expressed as semantic
    /// version requirements, or `None` for Git reference constraints.
    ///
    /// # Conversion Rules
    ///
    /// - **Exact**: Converted to `=1.0.0` requirement
    /// - **Requirement**: Returns the inner `VersionReq` directly
    /// - **`GitRef`**: Returns `None` (cannot be converted)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    /// use semver::Version;
    ///
    /// let exact = VersionConstraint::parse("1.0.0")?;
    /// let req = exact.to_version_req().unwrap();
    /// assert!(req.matches(&Version::parse("1.0.0")?));
    ///
    /// let caret = VersionConstraint::parse("^1.0.0")?;
    /// let req = caret.to_version_req().unwrap();
    /// assert!(req.matches(&Version::parse("1.2.0")?));
    ///
    /// let git_ref = VersionConstraint::parse("main")?;
    /// assert!(git_ref.to_version_req().is_none()); // Git refs can't be converted
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Use Cases
    ///
    /// This method is useful for integrating with existing semver-based tooling
    /// or for performing version calculations that require `VersionReq` objects.
    #[must_use]
    pub fn to_version_req(&self) -> Option<VersionReq> {
        match self {
            Self::Exact {
                version,
                ..
            } => {
                // Create an exact version requirement
                VersionReq::parse(&format!("={version}")).ok()
            }
            Self::Requirement {
                req,
                ..
            } => Some(req.clone()),
            Self::GitRef(_) => None, // Git refs cannot be converted to version requirements
        }
    }

    /// Check if this constraint allows prerelease versions.
    ///
    /// Prerelease versions contain identifiers like `-alpha`, `-beta`, `-rc` that
    /// indicate pre-release status. This method determines whether the constraint
    /// should consider such versions during resolution.
    ///
    /// # Prerelease Policy
    ///
    /// - **`GitRef`**: Allows prereleases (Git refs may point to any commit)
    /// - **Exact/Requirement**: Excludes prereleases unless explicitly specified
    ///
    /// # Returns
    ///
    /// Returns `true` if prerelease versions should be considered, `false` if only
    /// stable versions should be considered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::VersionConstraint;
    ///
    /// let branch = VersionConstraint::parse("main")?;
    /// assert!(branch.allows_prerelease()); // Git refs may be any version
    ///
    /// let latest = VersionConstraint::parse("latest")?;
    /// assert!(latest.allows_prerelease()); // Git ref - just a tag name
    ///
    /// let exact = VersionConstraint::parse("1.0.0")?;
    /// assert!(!exact.allows_prerelease()); // Exact stable version
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Impact on Resolution
    ///
    /// During version resolution, if any constraint in a set allows prereleases,
    /// the entire constraint set will consider prerelease versions as candidates.
    #[must_use]
    pub const fn allows_prerelease(&self) -> bool {
        matches!(self, Self::GitRef(_))
    }
}

impl fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact {
                prefix,
                version,
            } => {
                if let Some(p) = prefix {
                    write!(f, "{p}-{version}")
                } else {
                    write!(f, "{version}")
                }
            }
            Self::Requirement {
                prefix,
                req,
            } => {
                if let Some(p) = prefix {
                    write!(f, "{p}-{req}")
                } else {
                    write!(f, "{req}")
                }
            }
            Self::GitRef(ref_name) => write!(f, "{ref_name}"),
        }
    }
}

#[cfg(test)]
mod tests;
