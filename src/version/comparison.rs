//! Version comparison utilities for semantic version handling.
//!
//! This module provides utilities for comparing semantic versions, checking for
//! newer versions, and handling version parsing operations used throughout the
//! dependency resolution process.

use anyhow::Result;
use semver::Version;

/// Version comparison utilities
pub struct VersionComparator;

impl VersionComparator {
    /// Check if there are newer versions available
    pub fn has_newer_version(current: &str, versions: &[String]) -> Result<bool> {
        let current_version = Self::parse_version(current)?;

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str) {
                if version > current_version {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Get all versions newer than the current one
    pub fn get_newer_versions<'a>(
        current: &str,
        versions: &'a [String],
    ) -> Result<Vec<&'a String>> {
        let current_version = Self::parse_version(current)?;
        let mut newer = Vec::new();

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str) {
                if version > current_version {
                    newer.push(version_str);
                }
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

    /// Get the latest version from a list
    pub fn get_latest(versions: &[String]) -> Result<Option<&String>> {
        if versions.is_empty() {
            return Ok(None);
        }

        let mut latest: Option<(&String, Version)> = None;

        for version_str in versions {
            if let Ok(version) = Self::parse_version(version_str) {
                if latest.is_none() || version > latest.as_ref().unwrap().1 {
                    latest = Some((version_str, version));
                }
            }
        }

        Ok(latest.map(|(s, _)| s))
    }

    /// Parse a version string, handling common prefixes
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
