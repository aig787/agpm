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
//! - **Latest keywords**: `"latest"` (stable), `"latest-prerelease"` (any)
//! - **Git references**: `"main"`, `"feature/branch"`, `"abc123"` - Git branches, tags, or commits
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
//! use agpm::version::constraints::VersionConstraint;
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
//! use agpm::version::constraints::{ConstraintSet, VersionConstraint};
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
//! use agpm::version::constraints::ConstraintResolver;
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
//! | `latest` | Latest stable version | No prereleases |
//! | `latest-prerelease` | Latest version including prereleases | Includes alpha, beta, RC |
//! | `main` | Git branch reference | Branch name |
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
//! - **Explicit inclusion**: Use `latest-prerelease` or Git references to include prereleases
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
use std::collections::HashMap;
use std::fmt;

use crate::core::AgpmError;

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
/// - [`Latest`](Self::Latest): Matches the newest stable version available
/// - [`LatestPrerelease`](Self::LatestPrerelease): Matches newest version including prereleases
/// - [`GitRef`](Self::GitRef): Matches specific Git branches, tags, or commit hashes
///
/// # Examples
///
/// ```rust,no_run
/// use agpm::version::constraints::VersionConstraint;
/// use semver::Version;
///
/// // Parse various constraint formats
/// let exact = VersionConstraint::parse("1.0.0")?;
/// let caret = VersionConstraint::parse("^1.0.0")?; // Compatible versions
/// let tilde = VersionConstraint::parse("~1.2.0")?; // Patch-level compatible
/// let range = VersionConstraint::parse(">=1.0.0, <2.0.0")?; // Version range
/// let latest = VersionConstraint::parse("latest")?;
/// let branch = VersionConstraint::parse("main")?;
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
/// - `Latest` only matches stable versions (no `-alpha`, `-beta`, `-rc` suffixes)
/// - `LatestPrerelease` includes all versions including prereleases
/// - `GitRef` constraints allow prereleases since they reference specific commits
///
/// # Git Reference Matching
///
/// Git references are matched by name rather than semantic version:
/// - Branch names: `"main"`, `"develop"`, `"feature/auth"`
/// - Tag names: `"v1.0.0"`, `"release-2023-01"`
/// - Commit hashes: `"abc123def456"` (full or abbreviated)
#[derive(Debug, Clone)]
pub enum VersionConstraint {
    /// Exact version match (e.g., "1.0.0")
    Exact(Version),

    /// Semantic version requirement (e.g., "^1.0.0", "~1.2.0", ">=1.0.0")
    Requirement(VersionReq),

    /// Latest stable version
    Latest,

    /// Latest version including prereleases
    LatestPrerelease,

    /// Git tag or branch name
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
    /// 1. **Special keywords**: `"latest"`, `"*"`, `"latest-prerelease"`
    /// 2. **Exact versions**: `"1.0.0"`, `"v1.0.0"` (without range operators)
    /// 3. **Version requirements**: `"^1.0.0"`, `"~1.2.0"`, `">=1.0.0"`, `"<2.0.0"`
    /// 4. **Git references**: Any string that doesn't match the above patterns
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
    /// use agpm::version::constraints::VersionConstraint;
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
    /// let latest = VersionConstraint::parse("latest")?;     // Latest stable
    /// let any = VersionConstraint::parse("*")?;             // Any version
    /// let latest_pre = VersionConstraint::parse("latest-prerelease")?;
    ///
    /// // Git references
    /// let branch = VersionConstraint::parse("main")?;       // Branch name
    /// let tag = VersionConstraint::parse("release-v1")?;    // Tag name
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

        // Check for special keywords
        if trimmed == "latest" || trimmed == "*" {
            return Ok(Self::Latest);
        }

        if trimmed == "latest-prerelease" {
            return Ok(Self::LatestPrerelease);
        }

        // Try to parse as exact version (with or without 'v' prefix)
        let version_str = trimmed.strip_prefix('v').unwrap_or(trimmed);
        if let Ok(version) = Version::parse(version_str) {
            // Check if it's a range operator
            if !trimmed.starts_with('^')
                && !trimmed.starts_with('~')
                && !trimmed.starts_with('>')
                && !trimmed.starts_with('<')
                && !trimmed.starts_with('=')
            {
                return Ok(Self::Exact(version));
            }
        }

        // Try to parse as version requirement (with v-prefix normalization)
        match crate::version::parse_version_req(trimmed) {
            Ok(req) => return Ok(Self::Requirement(req)),
            Err(e) => {
                // If it looks like a semver constraint but failed to parse, return error
                if trimmed.starts_with('^')
                    || trimmed.starts_with('~')
                    || trimmed.starts_with('=')
                    || trimmed.starts_with('>')
                    || trimmed.starts_with('<')
                {
                    return Err(anyhow::anyhow!(
                        "Invalid semver constraint '{}': {}",
                        trimmed,
                        e
                    ));
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
    /// - **Latest**: Version must be stable (no prerelease components)
    /// - **`LatestPrerelease`**: Any version matches (selection happens during resolution)
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
    /// use agpm::version::constraints::VersionConstraint;
    /// use semver::Version;
    ///
    /// let constraint = VersionConstraint::parse("^1.0.0")?;
    /// let version = Version::parse("1.2.3")?;
    ///
    /// assert!(constraint.matches(&version)); // 1.2.3 is compatible with ^1.0.0
    ///
    /// let prerelease = Version::parse("1.0.0-alpha.1")?;
    /// let latest = VersionConstraint::parse("latest")?;
    /// assert!(!latest.matches(&prerelease)); // Latest excludes prereleases
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
            Self::Exact(v) => v == version,
            Self::Requirement(req) => req.matches(version),
            Self::Latest => version.pre.is_empty(), // Latest excludes prereleases
            Self::LatestPrerelease => true,         // Any version matches
            Self::GitRef(_) => false,               // Git refs don't match semver versions
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
    /// use agpm::version::constraints::VersionConstraint;
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
    /// - **Latest/LatestPrerelease**: Converted to `*` (any version) requirement
    /// - **`GitRef`**: Returns `None` (cannot be converted)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::VersionConstraint;
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
        if matches!(
            self,
            Self::Exact(_) | Self::Requirement(_) | Self::Latest | Self::LatestPrerelease
        ) {
            match self {
                Self::Exact(v) => {
                    // Create an exact version requirement
                    VersionReq::parse(&format!("={v}")).ok()
                }
                Self::Requirement(req) => Some(req.clone()),
                Self::Latest | Self::LatestPrerelease => {
                    // Match any version
                    VersionReq::parse("*").ok()
                }
                _ => unreachable!(),
            }
        } else {
            None
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
    /// - **Latest**: Excludes prereleases (stable versions only)
    /// - **`LatestPrerelease`**: Explicitly allows prereleases
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
    /// use agpm::version::constraints::VersionConstraint;
    ///
    /// let latest = VersionConstraint::parse("latest")?;
    /// assert!(!latest.allows_prerelease()); // Stable only
    ///
    /// let latest_pre = VersionConstraint::parse("latest-prerelease")?;
    /// assert!(latest_pre.allows_prerelease()); // Includes prereleases
    ///
    /// let branch = VersionConstraint::parse("main")?;
    /// assert!(branch.allows_prerelease()); // Git refs may be any version
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
    pub fn allows_prerelease(&self) -> bool {
        matches!(self, Self::LatestPrerelease | Self::GitRef(_))
    }
}

impl fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(v) => write!(f, "{v}"),
            Self::Requirement(req) => write!(f, "{req}"),
            Self::Latest => write!(f, "latest"),
            Self::LatestPrerelease => write!(f, "latest-prerelease"),
            Self::GitRef(ref_name) => write!(f, "{ref_name}"),
        }
    }
}

/// A collection of version constraints that must all be satisfied simultaneously.
///
/// `ConstraintSet` manages multiple [`VersionConstraint`]s for a single dependency,
/// ensuring that all constraints are compatible and can be resolved together.
/// It provides conflict detection, version matching, and best-match selection.
///
/// # Constraint Combination
///
/// When multiple constraints are added to a set, they create an intersection
/// of requirements. For example:
/// - `>=1.0.0` AND `<2.0.0` = versions in range `[1.0.0, 2.0.0)`
/// - `^1.0.0` AND `~1.2.0` = versions compatible with both (e.g., `1.2.x`)
///
/// # Conflict Detection
///
/// The constraint set detects and prevents conflicting constraints:
/// - Multiple exact versions: `1.0.0` AND `2.0.0` (impossible to satisfy)
/// - Conflicting Git refs: `main` AND `develop` (can't be both branches)
///
/// # Resolution Strategy
///
/// When selecting from available versions, the set:
/// 1. Filters versions that satisfy ALL constraints
/// 2. Excludes prereleases unless explicitly allowed
/// 3. Selects the highest remaining version
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust,no_run
/// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse(">=1.0.0")?)?;
/// set.add(VersionConstraint::parse("<2.0.0")?)?;
///
/// let version = Version::parse("1.5.0")?;
/// assert!(set.satisfies(&version));
///
/// let version = Version::parse("2.0.0")?;
/// assert!(!set.satisfies(&version)); // Outside range
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Best Match Selection
///
/// ```rust,no_run
/// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse("^1.0.0")?)?;
///
/// let versions = vec![
///     Version::parse("0.9.0")?,  // Too old
///     Version::parse("1.0.0")?,  // Matches
///     Version::parse("1.5.0")?,  // Matches, higher
///     Version::parse("2.0.0")?,  // Too new
/// ];
///
/// let best = set.find_best_match(&versions).unwrap();
/// assert_eq!(best, &Version::parse("1.5.0")?); // Highest compatible
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Conflict Detection
///
/// ```rust,no_run
/// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse("1.0.0")?)?; // Exact version
///
/// // This will fail - can't have two different exact versions
/// let result = set.add(VersionConstraint::parse("2.0.0")?);
/// assert!(result.is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ConstraintSet {
    constraints: Vec<VersionConstraint>,
}

impl Default for ConstraintSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSet {
    /// Creates a new empty constraint set
    ///
    /// # Returns
    ///
    /// Returns a new `ConstraintSet` with no constraints
    #[must_use]
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    /// Add a constraint to this set with conflict detection.
    ///
    /// This method adds a new constraint to the set after checking for conflicts
    /// with existing constraints. If the new constraint would create an impossible
    /// situation (like requiring two different exact versions), an error is returned.
    ///
    /// # Arguments
    ///
    /// * `constraint` - The [`VersionConstraint`] to add to this set
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the constraint was added successfully, or `Err` if it
    /// conflicts with existing constraints.
    ///
    /// # Conflict Detection
    ///
    /// Current conflict detection covers:
    /// - **Exact version conflicts**: Different exact versions for the same dependency
    /// - **Git ref conflicts**: Different Git references for the same dependency
    ///
    /// Future versions may add more sophisticated conflict detection for semantic
    /// version ranges.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut set = ConstraintSet::new();
    ///
    /// // These constraints are compatible
    /// set.add(VersionConstraint::parse(">=1.0.0")?)?;
    /// set.add(VersionConstraint::parse("<2.0.0")?)?;
    ///
    /// // This would conflict with exact versions
    /// set.add(VersionConstraint::parse("1.5.0")?)?;
    /// let result = set.add(VersionConstraint::parse("1.6.0")?);
    /// assert!(result.is_err()); // Conflict: can't be both 1.5.0 AND 1.6.0
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn add(&mut self, constraint: VersionConstraint) -> Result<()> {
        // Check for conflicting constraints
        if self.has_conflict(&constraint) {
            return Err(AgpmError::Other {
                message: format!("Constraint {constraint} conflicts with existing constraints"),
            }
            .into());
        }

        self.constraints.push(constraint);
        Ok(())
    }

    /// Check if a version satisfies all constraints in this set.
    ///
    /// This method tests whether a given version passes all the constraints
    /// in this set. For the version to be acceptable, it must satisfy every
    /// single constraint - this represents a logical AND operation.
    ///
    /// # Arguments
    ///
    /// * `version` - The semantic version to test against all constraints
    ///
    /// # Returns
    ///
    /// Returns `true` if the version satisfies ALL constraints, `false` if it
    /// fails to satisfy any constraint.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse(">=1.0.0")?)?; // Must be at least 1.0.0
    /// set.add(VersionConstraint::parse("<2.0.0")?)?;  // Must be less than 2.0.0
    /// set.add(VersionConstraint::parse("^1.0.0")?)?;  // Must be compatible with 1.0.0
    ///
    /// assert!(set.satisfies(&Version::parse("1.5.0")?)); // Satisfies all three
    /// assert!(!set.satisfies(&Version::parse("0.9.0")?)); // Fails >=1.0.0
    /// assert!(!set.satisfies(&Version::parse("2.0.0")?)); // Fails <2.0.0
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Performance Note
    ///
    /// This method short-circuits on the first constraint that fails, making it
    /// efficient even with many constraints.
    #[must_use]
    pub fn satisfies(&self, version: &Version) -> bool {
        self.constraints.iter().all(|c| c.matches(version))
    }

    /// Find the best matching version from a list of available versions.
    ///
    /// This method filters the provided versions to find those that satisfy all
    /// constraints, then selects the "best" match according to AGPM's resolution
    /// strategy. The selection prioritizes newer versions while respecting prerelease
    /// preferences.
    ///
    /// # Resolution Strategy
    ///
    /// 1. **Filter candidates**: Keep only versions that satisfy all constraints
    /// 2. **Sort by version**: Order candidates from highest to lowest version
    /// 3. **Apply prerelease policy**: Remove prereleases unless explicitly allowed
    /// 4. **Select best**: Return the highest remaining version
    ///
    /// # Arguments
    ///
    /// * `versions` - Slice of available versions to choose from
    ///
    /// # Returns
    ///
    /// Returns `Some(&Version)` with the best matching version, or `None` if no
    /// version satisfies all constraints.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("^1.0.0")?)?;
    ///
    /// let versions = vec![
    ///     Version::parse("0.9.0")?,    // Too old
    ///     Version::parse("1.0.0")?,    // Compatible
    ///     Version::parse("1.2.0")?,    // Compatible, newer
    ///     Version::parse("1.5.0")?,    // Compatible, newest
    ///     Version::parse("2.0.0")?,    // Too new
    /// ];
    ///
    /// let best = set.find_best_match(&versions).unwrap();
    /// assert_eq!(best, &Version::parse("1.5.0")?); // Highest compatible version
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// ## Prerelease Handling
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("^1.0.0")?)?; // Doesn't allow prereleases
    ///
    /// let versions = vec![
    ///     Version::parse("1.0.0")?,
    ///     Version::parse("1.1.0-alpha.1")?,  // Prerelease
    ///     Version::parse("1.1.0")?,           // Stable
    /// ];
    ///
    /// let best = set.find_best_match(&versions).unwrap();
    /// assert_eq!(best, &Version::parse("1.1.0")?); // Stable version preferred
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn find_best_match<'a>(&self, versions: &'a [Version]) -> Option<&'a Version> {
        let mut candidates: Vec<&Version> = versions.iter().filter(|v| self.satisfies(v)).collect();

        // Sort by version (highest first)
        candidates.sort_by(|a, b| b.cmp(a));

        // If we don't allow prereleases, filter them out
        if !self.allows_prerelease() {
            candidates.retain(|v| v.pre.is_empty());
        }

        candidates.first().copied()
    }

    /// Check if any constraint in this set allows prerelease versions.
    ///
    /// This method determines the prerelease policy for the entire constraint set.
    /// If ANY constraint in the set allows prereleases, the entire set is considered
    /// to allow prereleases. This ensures that explicit prerelease constraints
    /// (like `latest-prerelease` or Git refs) are respected.
    ///
    /// # Returns
    ///
    /// Returns `true` if any constraint allows prereleases, `false` if all constraints
    /// exclude prereleases.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut stable_set = ConstraintSet::new();
    /// stable_set.add(VersionConstraint::parse("^1.0.0")?)?;
    /// stable_set.add(VersionConstraint::parse("latest")?)?;
    /// assert!(!stable_set.allows_prerelease()); // All constraints exclude prereleases
    ///
    /// let mut prerelease_set = ConstraintSet::new();
    /// prerelease_set.add(VersionConstraint::parse("^1.0.0")?)?;
    /// prerelease_set.add(VersionConstraint::parse("latest-prerelease")?)?;
    /// assert!(prerelease_set.allows_prerelease()); // One constraint allows prereleases
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Impact on Resolution
    ///
    /// This setting affects [`find_best_match`](Self::find_best_match) behavior:
    /// - If `false`: Prerelease versions are filtered out before selection
    /// - If `true`: Prerelease versions are included in selection
    #[must_use]
    pub fn allows_prerelease(&self) -> bool {
        self.constraints
            .iter()
            .any(VersionConstraint::allows_prerelease)
    }

    /// Check if a new constraint would conflict with existing constraints.
    ///
    /// This method performs conflict detection to prevent adding incompatible
    /// constraints to the same set. It currently detects basic conflicts but
    /// could be enhanced with more sophisticated analysis in the future.
    ///
    /// # Current Conflict Detection
    ///
    /// - **Exact version conflicts**: Two different exact versions (`1.0.0` vs `2.0.0`)
    /// - **Git reference conflicts**: Two different Git refs (`main` vs `develop`)
    ///
    /// # Arguments
    ///
    /// * `new_constraint` - The constraint to test for conflicts
    ///
    /// # Returns
    ///
    /// Returns `true` if the constraint conflicts with existing ones, `false` if
    /// it's compatible.
    ///
    /// # Future Enhancements
    ///
    /// Future versions could detect more sophisticated conflicts:
    /// - Impossible version ranges (e.g., `>2.0.0` AND `<1.0.0`)
    /// - Contradictory semver requirements
    /// - Mixed version and Git reference constraints
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("1.0.0")?)?;
    ///
    /// // This would conflict (different exact versions)
    /// let conflicting = VersionConstraint::parse("2.0.0")?;
    /// assert!(set.has_conflict(&conflicting));
    ///
    /// // This would not conflict (same exact version)
    /// let compatible = VersionConstraint::parse("1.0.0")?;
    /// assert!(!set.has_conflict(&compatible));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    fn has_conflict(&self, new_constraint: &VersionConstraint) -> bool {
        // Simple conflict detection - can be enhanced
        for existing in &self.constraints {
            match (existing, new_constraint) {
                (VersionConstraint::Exact(v1), VersionConstraint::Exact(v2)) => {
                    if v1 != v2 {
                        return true;
                    }
                }
                (VersionConstraint::GitRef(r1), VersionConstraint::GitRef(r2)) => {
                    if r1 != r2 {
                        return true;
                    }
                }
                _ => {
                    // More sophisticated conflict detection could be added here
                }
            }
        }
        false
    }
}

/// Manages version constraints for multiple dependencies and resolves them simultaneously.
///
/// `ConstraintResolver` coordinates version resolution across an entire dependency graph,
/// ensuring that all constraints are satisfied and conflicts are detected. It maintains
/// separate [`ConstraintSet`]s for each dependency and resolves them against available
/// version catalogs.
///
/// # Multi-Dependency Resolution
///
/// Unlike [`ConstraintSet`] which manages constraints for a single dependency, the
/// `ConstraintResolver` handles multiple dependencies simultaneously:
///
/// - Each dependency gets its own constraint set
/// - Constraints can be added incrementally
/// - Resolution happens across the entire dependency graph
/// - Missing dependencies are detected and reported
///
/// # Resolution Process
///
/// 1. **Collect constraints**: Gather all constraints for each dependency
/// 2. **Validate availability**: Ensure versions exist for all dependencies
/// 3. **Apply constraint sets**: Use each dependency's constraints to filter versions
/// 4. **Select best matches**: Choose optimal versions for each dependency
/// 5. **Return resolution map**: Provide final version selections
///
/// # Examples
///
/// ## Basic Multi-Dependency Resolution
///
/// ```rust,no_run
/// use agpm::version::constraints::ConstraintResolver;
/// use semver::Version;
/// use std::collections::HashMap;
///
/// let mut resolver = ConstraintResolver::new();
///
/// // Add constraints for multiple dependencies
/// resolver.add_constraint("dep1", "^1.0.0")?;
/// resolver.add_constraint("dep2", "~2.1.0")?;
/// resolver.add_constraint("dep3", "latest")?;
///
/// // Provide available versions for each dependency
/// let mut available = HashMap::new();
/// available.insert("dep1".to_string(), vec![Version::parse("1.5.0")?]);
/// available.insert("dep2".to_string(), vec![Version::parse("2.1.3")?]);
/// available.insert("dep3".to_string(), vec![Version::parse("3.0.0")?]);
///
/// // Resolve all dependencies
/// let resolved = resolver.resolve(&available)?;
/// assert_eq!(resolved.len(), 3);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Incremental Constraint Addition
///
/// ```rust,no_run
/// use agpm::version::constraints::ConstraintResolver;
///
/// let mut resolver = ConstraintResolver::new();
///
/// // Add multiple constraints for the same dependency
/// resolver.add_constraint("my-dep", ">=1.0.0")?;
/// resolver.add_constraint("my-dep", "<2.0.0")?;
/// resolver.add_constraint("my-dep", "^1.5.0")?;
///
/// // All constraints will be combined into a single constraint set
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Error Conditions
///
/// The resolver reports several types of errors:
///
/// - **Missing dependencies**: A constraint exists but no versions are available
/// - **Unsatisfiable constraints**: No available version meets all requirements
/// - **Conflicting constraints**: Impossible constraint combinations
///
/// # Use Cases
///
/// This resolver is particularly useful for:
/// - Package managers resolving dependency graphs
/// - Build systems selecting compatible versions
/// - Configuration management ensuring consistent environments
/// - Update analysis determining safe upgrade paths
pub struct ConstraintResolver {
    constraints: HashMap<String, ConstraintSet>,
}

impl Default for ConstraintResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintResolver {
    /// Creates a new constraint resolver
    ///
    /// # Returns
    ///
    /// Returns a new `ConstraintResolver` with empty constraint and resolution maps
    #[must_use]
    pub fn new() -> Self {
        Self {
            constraints: HashMap::new(),
        }
    }

    /// Add a version constraint for a specific dependency.
    ///
    /// This method parses the constraint string and adds it to the constraint set
    /// for the named dependency. If this is the first constraint for the dependency,
    /// a new constraint set is created. Multiple constraints for the same dependency
    /// are combined into a single set with conflict detection.
    ///
    /// # Arguments
    ///
    /// * `dependency` - The name of the dependency to constrain
    /// * `constraint` - The constraint string to parse and add (e.g., "^1.0.0", "latest")
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the constraint was added successfully, or `Err` if:
    /// - The constraint string is invalid
    /// - The constraint conflicts with existing constraints for this dependency
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::ConstraintResolver;
    ///
    /// let mut resolver = ConstraintResolver::new();
    ///
    /// // Add constraints for different dependencies
    /// resolver.add_constraint("web-framework", "^2.0.0")?;
    /// resolver.add_constraint("database", "~1.5.0")?;
    /// resolver.add_constraint("auth-lib", "latest")?;
    ///
    /// // Add multiple constraints for the same dependency
    /// resolver.add_constraint("api-client", ">=1.0.0")?;
    /// resolver.add_constraint("api-client", "<2.0.0")?; // Compatible range
    ///
    /// // This would fail - conflicting exact versions
    /// resolver.add_constraint("my-dep", "1.0.0")?;
    /// let result = resolver.add_constraint("my-dep", "2.0.0");
    /// assert!(result.is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Constraint Combination
    ///
    /// When multiple constraints are added for the same dependency, they are
    /// combined using AND logic. The final constraint set requires that all
    /// individual constraints be satisfied simultaneously.
    pub fn add_constraint(&mut self, dependency: &str, constraint: &str) -> Result<()> {
        let parsed = VersionConstraint::parse(constraint)?;

        self.constraints
            .entry(dependency.to_string())
            .or_default()
            .add(parsed)?;

        Ok(())
    }

    /// Resolve all dependency constraints and return the best version for each.
    ///
    /// This method performs the core resolution algorithm, taking all accumulated
    /// constraints and finding the best matching version for each dependency from
    /// the provided catalog of available versions.
    ///
    /// # Resolution Algorithm
    ///
    /// For each dependency with constraints:
    /// 1. **Verify availability**: Check that versions exist for the dependency
    /// 2. **Apply constraints**: Filter versions using the dependency's constraint set
    /// 3. **Select best match**: Choose the highest compatible version
    /// 4. **Handle prereleases**: Apply prerelease policies appropriately
    ///
    /// # Arguments
    ///
    /// * `available_versions` - Map from dependency names to lists of available versions
    ///
    /// # Returns
    ///
    /// Returns `Ok(HashMap<String, Version>)` with the resolved version for each
    /// dependency, or `Err` if resolution fails.
    ///
    /// # Error Conditions
    ///
    /// - **Missing dependency**: Constraint exists but no versions available
    /// - **No satisfying version**: Available versions don't meet constraints
    /// - **Internal errors**: Constraint set conflicts or parsing failures
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::ConstraintResolver;
    /// use semver::Version;
    /// use std::collections::HashMap;
    ///
    /// let mut resolver = ConstraintResolver::new();
    /// resolver.add_constraint("web-server", "^1.0.0")?;
    /// resolver.add_constraint("database", "~2.1.0")?;
    ///
    /// // Provide version catalog
    /// let mut available = HashMap::new();
    /// available.insert(
    ///     "web-server".to_string(),
    ///     vec![
    ///         Version::parse("1.0.0")?,
    ///         Version::parse("1.2.0")?,
    ///         Version::parse("1.5.0")?, // Best match for ^1.0.0
    ///         Version::parse("2.0.0")?, // Too new
    ///     ],
    /// );
    /// available.insert(
    ///     "database".to_string(),
    ///     vec![
    ///         Version::parse("2.1.0")?,
    ///         Version::parse("2.1.3")?, // Best match for ~2.1.0
    ///         Version::parse("2.2.0")?, // Too new
    ///     ],
    /// );
    ///
    /// // Resolve dependencies
    /// let resolved = resolver.resolve(&available)?;
    /// assert_eq!(resolved["web-server"], Version::parse("1.5.0")?);
    /// assert_eq!(resolved["database"], Version::parse("2.1.3")?);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// ## Error Handling
    ///
    /// ```rust,no_run
    /// use agpm::version::constraints::ConstraintResolver;
    /// use std::collections::HashMap;
    ///
    /// let mut resolver = ConstraintResolver::new();
    /// resolver.add_constraint("missing-dep", "^1.0.0")?;
    ///
    /// let available = HashMap::new(); // No versions provided
    ///
    /// let result = resolver.resolve(&available);
    /// assert!(result.is_err()); // Missing dependency error
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - Resolution is performed independently for each dependency
    /// - Version filtering and sorting may be expensive for large version lists
    /// - Consider pre-filtering available versions if catalogs are very large
    pub fn resolve(
        &self,
        available_versions: &HashMap<String, Vec<Version>>,
    ) -> Result<HashMap<String, Version>> {
        let mut resolved = HashMap::new();

        for (dep, constraint_set) in &self.constraints {
            let versions = available_versions
                .get(dep)
                .ok_or_else(|| AgpmError::Other {
                    message: format!("No versions available for dependency: {dep}"),
                })?;

            let best_match =
                constraint_set
                    .find_best_match(versions)
                    .ok_or_else(|| AgpmError::Other {
                        message: format!("No version satisfies constraints for dependency: {dep}"),
                    })?;

            resolved.insert(dep.clone(), best_match.clone());
        }

        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constraint_parse() {
        // Exact version
        let constraint = VersionConstraint::parse("1.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Exact(_)));

        // Version with v prefix
        let constraint = VersionConstraint::parse("v1.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Exact(_)));

        // Caret requirement
        let constraint = VersionConstraint::parse("^1.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        // Tilde requirement
        let constraint = VersionConstraint::parse("~1.2.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        // Range requirement
        let constraint = VersionConstraint::parse(">=1.0.0, <2.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        // Latest
        let constraint = VersionConstraint::parse("latest").unwrap();
        assert!(matches!(constraint, VersionConstraint::Latest));

        // Git ref
        let constraint = VersionConstraint::parse("main").unwrap();
        assert!(matches!(constraint, VersionConstraint::GitRef(_)));
    }

    #[test]
    fn test_constraint_matching() {
        let v100 = Version::parse("1.0.0").unwrap();
        let v110 = Version::parse("1.1.0").unwrap();
        let v200 = Version::parse("2.0.0").unwrap();

        // Exact match
        let exact = VersionConstraint::Exact(v100.clone());
        assert!(exact.matches(&v100));
        assert!(!exact.matches(&v110));

        // Caret requirement
        let caret = VersionConstraint::parse("^1.0.0").unwrap();
        assert!(caret.matches(&v100));
        assert!(caret.matches(&v110));
        assert!(!caret.matches(&v200));

        // Latest matches all stable versions
        let latest = VersionConstraint::Latest;
        assert!(latest.matches(&v100));
        assert!(latest.matches(&v200));
    }

    #[test]
    fn test_constraint_set() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse(">=1.0.0").unwrap())
            .unwrap();
        set.add(VersionConstraint::parse("<2.0.0").unwrap())
            .unwrap();

        let v090 = Version::parse("0.9.0").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();
        let v150 = Version::parse("1.5.0").unwrap();
        let v200 = Version::parse("2.0.0").unwrap();

        assert!(!set.satisfies(&v090));
        assert!(set.satisfies(&v100));
        assert!(set.satisfies(&v150));
        assert!(!set.satisfies(&v200));
    }

    #[test]
    fn test_find_best_match() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse("^1.0.0").unwrap())
            .unwrap();

        let versions = vec![
            Version::parse("0.9.0").unwrap(),
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.2.0").unwrap(),
            Version::parse("1.5.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
        ];

        let best = set.find_best_match(&versions).unwrap();
        assert_eq!(best, &Version::parse("1.5.0").unwrap());
    }

    #[test]
    fn test_constraint_conflicts() {
        let mut set = ConstraintSet::new();

        // Add first exact version
        set.add(VersionConstraint::Exact(Version::parse("1.0.0").unwrap()))
            .unwrap();

        // Try to add conflicting exact version
        let result = set.add(VersionConstraint::Exact(Version::parse("2.0.0").unwrap()));
        assert!(result.is_err());

        // Adding the same version should be ok
        let result = set.add(VersionConstraint::Exact(Version::parse("1.0.0").unwrap()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_constraint_resolver() {
        let mut resolver = ConstraintResolver::new();

        resolver.add_constraint("dep1", "^1.0.0").unwrap();
        resolver.add_constraint("dep2", "~2.1.0").unwrap();

        let mut available = HashMap::new();
        available.insert(
            "dep1".to_string(),
            vec![
                Version::parse("0.9.0").unwrap(),
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.5.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ],
        );
        available.insert(
            "dep2".to_string(),
            vec![
                Version::parse("2.0.0").unwrap(),
                Version::parse("2.1.0").unwrap(),
                Version::parse("2.1.5").unwrap(),
                Version::parse("2.2.0").unwrap(),
            ],
        );

        let resolved = resolver.resolve(&available).unwrap();
        assert_eq!(
            resolved.get("dep1"),
            Some(&Version::parse("1.5.0").unwrap())
        );
        assert_eq!(
            resolved.get("dep2"),
            Some(&Version::parse("2.1.5").unwrap())
        );
    }

    #[test]
    fn test_allows_prerelease() {
        assert!(!VersionConstraint::Latest.allows_prerelease());
        assert!(VersionConstraint::LatestPrerelease.allows_prerelease());
        assert!(VersionConstraint::GitRef("main".to_string()).allows_prerelease());
        assert!(!VersionConstraint::Exact(Version::parse("1.0.0").unwrap()).allows_prerelease());
    }

    #[test]
    fn test_version_constraint_parse_edge_cases() {
        // Test latest-prerelease
        let constraint = VersionConstraint::parse("latest-prerelease").unwrap();
        assert!(matches!(constraint, VersionConstraint::LatestPrerelease));

        // Test asterisk for latest
        let constraint = VersionConstraint::parse("*").unwrap();
        assert!(matches!(constraint, VersionConstraint::Latest));

        // Test range operators
        let constraint = VersionConstraint::parse(">=1.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        let constraint = VersionConstraint::parse("<2.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        let constraint = VersionConstraint::parse("=1.0.0").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));

        // Test git branch names
        let constraint = VersionConstraint::parse("feature/new-feature").unwrap();
        assert!(matches!(constraint, VersionConstraint::GitRef(_)));

        // Test commit hash
        let constraint = VersionConstraint::parse("abc123def456").unwrap();
        assert!(matches!(constraint, VersionConstraint::GitRef(_)));
    }

    #[test]
    fn test_version_constraint_display() {
        let exact = VersionConstraint::Exact(Version::parse("1.0.0").unwrap());
        assert_eq!(format!("{exact}"), "1.0.0");

        let req = VersionConstraint::parse("^1.0.0").unwrap();
        assert_eq!(format!("{req}"), "^1.0.0");

        let latest = VersionConstraint::Latest;
        assert_eq!(format!("{latest}"), "latest");

        let latest_pre = VersionConstraint::LatestPrerelease;
        assert_eq!(format!("{latest_pre}"), "latest-prerelease");

        let git_ref = VersionConstraint::GitRef("main".to_string());
        assert_eq!(format!("{git_ref}"), "main");
    }

    #[test]
    fn test_version_constraint_matches_ref() {
        let git_ref = VersionConstraint::GitRef("main".to_string());
        assert!(git_ref.matches_ref("main"));
        assert!(!git_ref.matches_ref("develop"));

        // Other constraint types should return false for ref matching
        let exact = VersionConstraint::Exact(Version::parse("1.0.0").unwrap());
        assert!(!exact.matches_ref("v1.0.0"));

        let latest = VersionConstraint::Latest;
        assert!(!latest.matches_ref("latest"));
    }

    #[test]
    fn test_version_constraint_to_version_req() {
        let exact = VersionConstraint::Exact(Version::parse("1.0.0").unwrap());
        let req = exact.to_version_req().unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));

        let caret = VersionConstraint::parse("^1.0.0").unwrap();
        let req = caret.to_version_req().unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));

        let latest = VersionConstraint::Latest;
        let req = latest.to_version_req().unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));

        let git_ref = VersionConstraint::GitRef("main".to_string());
        assert!(git_ref.to_version_req().is_none());
    }

    #[test]
    fn test_constraint_set_with_prereleases() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::LatestPrerelease).unwrap();

        let v100_pre = Version::parse("1.0.0-alpha.1").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();

        assert!(set.allows_prerelease());

        let versions = vec![v100_pre.clone(), v100.clone()];
        let best = set.find_best_match(&versions).unwrap();
        assert_eq!(best, &v100); // Stable version should still be preferred when both match
    }

    #[test]
    fn test_constraint_set_no_matches() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse(">=2.0.0").unwrap())
            .unwrap();

        let versions = vec![
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.5.0").unwrap(),
        ];

        let best = set.find_best_match(&versions);
        assert!(best.is_none());
    }

    #[test]
    fn test_constraint_resolver_missing_dependency() {
        let mut resolver = ConstraintResolver::new();
        resolver.add_constraint("dep1", "^1.0.0").unwrap();

        let available = HashMap::new(); // No versions available

        let result = resolver.resolve(&available);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_resolver_no_satisfying_version() {
        let mut resolver = ConstraintResolver::new();
        resolver.add_constraint("dep1", "^2.0.0").unwrap();

        let mut available = HashMap::new();
        available.insert(
            "dep1".to_string(),
            vec![Version::parse("1.0.0").unwrap()], // Only 1.x available, but we need 2.x
        );

        let result = resolver.resolve(&available);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_set_git_ref_conflicts() {
        let mut set = ConstraintSet::new();

        // Add first git ref
        set.add(VersionConstraint::GitRef("main".to_string()))
            .unwrap();

        // Try to add conflicting git ref
        let result = set.add(VersionConstraint::GitRef("develop".to_string()));
        assert!(result.is_err());

        // Adding the same ref should be ok
        let result = set.add(VersionConstraint::GitRef("main".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_latest_constraint_with_prereleases() {
        let latest = VersionConstraint::Latest;

        let v100_pre = Version::parse("1.0.0-alpha.1").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();

        // Latest should match stable versions
        assert!(latest.matches(&v100));
        // But not prereleases
        assert!(!latest.matches(&v100_pre));
    }

    #[test]
    fn test_latest_prerelease_constraint() {
        let latest_pre = VersionConstraint::LatestPrerelease;

        let v100_pre = Version::parse("1.0.0-alpha.1").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();

        // LatestPrerelease should match any version
        assert!(latest_pre.matches(&v100));
        assert!(latest_pre.matches(&v100_pre));
    }

    #[test]
    fn test_requirement_constraint_allows_prerelease() {
        let req = VersionConstraint::parse("^1.0.0").unwrap();
        assert!(!req.allows_prerelease());

        let exact = VersionConstraint::Exact(Version::parse("1.0.0").unwrap());
        assert!(!exact.allows_prerelease());
    }

    #[test]
    fn test_constraint_set_prerelease_filtering() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse("^1.0.0").unwrap())
            .unwrap();

        let versions = vec![
            Version::parse("1.0.0-alpha.1").unwrap(),
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.1.0-beta.1").unwrap(),
            Version::parse("1.1.0").unwrap(),
        ];

        let best = set.find_best_match(&versions).unwrap();
        assert_eq!(best, &Version::parse("1.1.0").unwrap()); // Should pick highest stable
    }

    #[test]
    fn test_parse_with_whitespace() {
        let constraint = VersionConstraint::parse("  1.0.0  ").unwrap();
        assert!(matches!(constraint, VersionConstraint::Exact(_)));

        let constraint = VersionConstraint::parse("  latest  ").unwrap();
        assert!(matches!(constraint, VersionConstraint::Latest));

        let constraint = VersionConstraint::parse("  ^1.0.0  ").unwrap();
        assert!(matches!(constraint, VersionConstraint::Requirement(_)));
    }

    #[test]
    fn test_constraint_resolver_add_constraint_error() {
        let mut resolver = ConstraintResolver::new();

        // Add a valid constraint first
        resolver.add_constraint("dep1", "1.0.0").unwrap();

        // Add conflicting constraint
        let result = resolver.add_constraint("dep1", "2.0.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_set_no_conflict_different_types() {
        let mut set = ConstraintSet::new();

        // These shouldn't conflict as they are different types
        set.add(VersionConstraint::parse("^1.0.0").unwrap())
            .unwrap();
        set.add(VersionConstraint::Latest).unwrap();

        // Should have 2 constraints
        assert_eq!(set.constraints.len(), 2);
    }

    #[test]
    fn test_version_constraint_to_version_req_none() {
        let latest_pre = VersionConstraint::LatestPrerelease;
        let req = latest_pre.to_version_req().unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));
        // Latest should match all versions
        assert!(req.matches(&Version::parse("0.1.0").unwrap()));
        assert!(req.matches(&Version::parse("10.0.0").unwrap()));
    }
}
