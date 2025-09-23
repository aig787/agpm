//! Version comparison utilities for semantic version handling.
//!
//! This module provides utilities for comparing semantic versions, checking for
//! newer versions, and handling version parsing operations used throughout the
//! dependency resolution process. It supports common version prefixes and
//! provides error-handling for malformed version strings.
//!
//! # Features
//!
//! - **Semantic Version Parsing**: Handles `v1.2.3`, `version-1.2.3`, `release-1.2.3` formats
//! - **Version Comparison**: Find newer versions and latest versions in collections
//! - **Prefix Handling**: Automatically strips common version prefixes
//! - **Error Resilience**: Gracefully handles malformed version strings
//!
//! # Examples
//!
//! ```rust,no_run
//! use ccpm::version::comparison::VersionComparator;
//!
//! # fn example() -> anyhow::Result<()> {
//! let versions = vec![
//!     "v1.0.0".to_string(),
//!     "v1.1.0".to_string(),
//!     "v2.0.0".to_string(),
//!     "version-1.0.1".to_string(),
//! ];
//!
//! // Check if there are newer versions
//! let has_newer = VersionComparator::has_newer_version("v1.0.0", &versions)?;
//! assert!(has_newer);
//!
//! // Get the latest version
//! let latest = VersionComparator::get_latest(&versions)?
//!     .expect("Should find latest version");
//! assert_eq!(latest, "v2.0.0");
//!
//! // Get all newer versions than current
//! let newer = VersionComparator::get_newer_versions("v1.0.0", &versions)?;
//! assert_eq!(newer.len(), 3); // v1.1.0, v2.0.0, version-1.0.1
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use semver::Version;

/// Version comparison utilities for semantic version operations.
///
/// This struct provides static methods for comparing semantic versions,
/// finding newer versions, and handling version parsing with common prefixes.
/// All methods are designed to handle malformed version strings gracefully.
pub struct VersionComparator;

impl VersionComparator {
    /// Checks if there are newer versions available than the current version.
    ///
    /// This method compares the current version against a list of available versions
    /// and returns `true` if any version is semantically newer.
    ///
    /// # Arguments
    ///
    /// * `current` - The current version string (e.g., "v1.0.0", "1.2.3")
    /// * `versions` - A slice of version strings to compare against
    ///
    /// # Returns
    ///
    /// `Ok(true)` if newer versions exist, `Ok(false)` if current is latest.
    ///
    /// # Errors
    ///
    /// Returns an error if the current version string cannot be parsed as a
    /// semantic version. Malformed versions in the comparison list are ignored.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::version::comparison::VersionComparator;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let versions = vec!["v1.0.0".to_string(), "v1.1.0".to_string(), "v2.0.0".to_string()];
    ///
    /// // Check if v1.0.0 has newer versions available
    /// let has_newer = VersionComparator::has_newer_version("v1.0.0", &versions)?;
    /// assert!(has_newer);
    ///
    /// // Check if v2.0.0 is the latest
    /// let has_newer = VersionComparator::has_newer_version("v2.0.0", &versions)?;
    /// assert!(!has_newer);
    /// # Ok(())
    /// # }
    /// ```
    pub fn has_newer_version(current: &str, versions: &[String]) -> Result<bool> {
        let current_version = Self::parse_version(current)?;

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str)
                && version > current_version
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Gets all versions newer than the current version, sorted by version descending.
    ///
    /// This method finds all versions in the provided list that are semantically
    /// newer than the current version and returns them sorted from newest to oldest.
    ///
    /// # Arguments
    ///
    /// * `current` - The current version string to compare against
    /// * `versions` - A slice of version strings to search
    ///
    /// # Returns
    ///
    /// A vector of references to version strings that are newer than current,
    /// sorted in descending order (newest first).
    ///
    /// # Errors
    ///
    /// Returns an error if the current version string cannot be parsed.
    /// Malformed versions in the search list are silently ignored.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::version::comparison::VersionComparator;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let versions = vec![
    ///     "v1.0.0".to_string(),
    ///     "v1.2.0".to_string(),
    ///     "v1.1.0".to_string(),
    ///     "v2.0.0".to_string(),
    /// ];
    ///
    /// let newer = VersionComparator::get_newer_versions("v1.0.0", &versions)?;
    /// assert_eq!(newer.len(), 3);
    /// // Results are sorted newest first
    /// assert_eq!(newer[0], "v2.0.0");
    /// assert_eq!(newer[1], "v1.2.0");
    /// assert_eq!(newer[2], "v1.1.0");
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_newer_versions<'a>(
        current: &str,
        versions: &'a [String],
    ) -> Result<Vec<&'a String>> {
        let current_version = Self::parse_version(current)?;
        let mut newer = Vec::new();

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str)
                && version > current_version
            {
                newer.push(version_str);
            }
        }

        // Sort by version descending
        newer.sort_by(|a, b| {
            let v1 = Self::parse_version(a).unwrap_or_else(|_| Version::new(0, 0, 0));
            let v2 = Self::parse_version(b).unwrap_or_else(|_| Version::new(0, 0, 0));
            v2.cmp(&v1)
        });

        Ok(newer)
    }

    /// Gets the latest (highest) semantic version from a list of versions.
    ///
    /// This method finds the semantically highest version from the provided list,
    /// ignoring any malformed version strings.
    ///
    /// # Arguments
    ///
    /// * `versions` - A slice of version strings to search
    ///
    /// # Returns
    ///
    /// `Ok(Some(&String))` with the latest version, or `Ok(None)` if the list is
    /// empty or contains no valid semantic versions.
    ///
    /// # Errors
    ///
    /// This method does not return errors - malformed version strings are silently
    /// ignored during comparison.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::version::comparison::VersionComparator;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let versions = vec![
    ///     "v1.0.0".to_string(),
    ///     "v2.0.0".to_string(),
    ///     "v1.5.0".to_string(),
    ///     "invalid-version".to_string(), // This will be ignored
    /// ];
    ///
    /// let latest = VersionComparator::get_latest(&versions)?
    ///     .expect("Should find a latest version");
    /// assert_eq!(latest, "v2.0.0");
    ///
    /// // Empty list returns None
    /// let empty: Vec<String> = vec![];
    /// assert!(VersionComparator::get_latest(&empty)?.is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_latest(versions: &[String]) -> Result<Option<&String>> {
        if versions.is_empty() {
            return Ok(None);
        }

        let mut latest: Option<(&String, Version)> = None;

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str)
                && (latest.is_none() || version > latest.as_ref().unwrap().1)
            {
                latest = Some((version_str, version));
            }
        }

        Ok(latest.map(|(s, _)| s))
    }

    /// Parses a version string, automatically handling common prefixes.
    ///
    /// This private method normalizes version strings by removing common prefixes
    /// like "v", "version-", and "release-" before parsing with the semver crate.
    ///
    /// # Supported Prefixes
    ///
    /// - `v1.2.3` → `1.2.3`
    /// - `version-1.2.3` → `1.2.3`
    /// - `release-1.2.3` → `1.2.3`
    /// - `1.2.3` → `1.2.3` (no change)
    ///
    /// # Arguments
    ///
    /// * `version_str` - The version string to parse
    ///
    /// # Returns
    ///
    /// A parsed `semver::Version` instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the version string (after prefix removal) is not
    /// a valid semantic version according to the semver specification.
    fn parse_version(version_str: &str) -> Result<Version> {
        // Remove common version prefixes
        let clean_version = if let Some(stripped) = version_str.strip_prefix("version-") {
            stripped
        } else if let Some(stripped) = version_str.strip_prefix("release-") {
            stripped
        } else if let Some(stripped) = version_str.strip_prefix('v') {
            stripped
        } else {
            version_str
        };

        Ok(Version::parse(clean_version)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_newer_version() {
        let versions = vec![
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
            "v2.0.0".to_string(),
        ];

        assert!(VersionComparator::has_newer_version("1.0.0", &versions).unwrap());
        assert!(VersionComparator::has_newer_version("v1.0.0", &versions).unwrap());
        assert!(!VersionComparator::has_newer_version("2.0.0", &versions).unwrap());
        assert!(!VersionComparator::has_newer_version("v3.0.0", &versions).unwrap());
    }

    #[test]
    fn test_get_newer_versions() {
        let versions = vec![
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
            "v2.0.0".to_string(),
            "v0.9.0".to_string(),
        ];

        let newer = VersionComparator::get_newer_versions("1.0.0", &versions).unwrap();
        assert_eq!(newer.len(), 2);
        assert_eq!(newer[0], "v2.0.0");
        assert_eq!(newer[1], "v1.1.0");

        let newer = VersionComparator::get_newer_versions("2.0.0", &versions).unwrap();
        assert_eq!(newer.len(), 0);
    }

    #[test]
    fn test_get_latest() {
        let versions = vec![
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
            "v2.0.0".to_string(),
            "v0.9.0".to_string(),
        ];

        let latest = VersionComparator::get_latest(&versions).unwrap();
        assert_eq!(latest, Some(&"v2.0.0".to_string()));

        let empty: Vec<String> = vec![];
        let latest = VersionComparator::get_latest(&empty).unwrap();
        assert_eq!(latest, None);
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(
            VersionComparator::parse_version("1.0.0").unwrap(),
            Version::new(1, 0, 0)
        );
        assert_eq!(
            VersionComparator::parse_version("v1.0.0").unwrap(),
            Version::new(1, 0, 0)
        );
        assert_eq!(
            VersionComparator::parse_version("version-1.0.0").unwrap(),
            Version::new(1, 0, 0)
        );
        assert_eq!(
            VersionComparator::parse_version("release-1.0.0").unwrap(),
            Version::new(1, 0, 0)
        );
    }
}
