//! Error handling helper functions and utilities
//!
//! This module provides common error handling patterns used throughout CCPM,
//! reducing boilerplate and ensuring consistent error messages.

use anyhow::{Context, Result};
use std::path::Path;

use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;

/// Common file operations with consistent error handling
pub trait FileOperations {
    /// Read a file with appropriate error context
    fn read_file_with_context(path: impl AsRef<Path>) -> Result<String> {
        let path = path.as_ref();
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))
    }

    /// Write to a file with appropriate error context
    fn write_file_with_context(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, content.as_ref())
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }

    /// Create a directory with appropriate error context
    fn create_dir_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))
    }

    /// Read a file as bytes with appropriate error context
    fn read_bytes_with_context(path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        std::fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))
    }

    /// Write bytes to a file with appropriate error context
    fn write_bytes_with_context(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, content.as_ref())
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }

    /// Copy a file with appropriate error context
    fn copy_file_with_context(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<u64> {
        let from = from.as_ref();
        let to = to.as_ref();
        std::fs::copy(from, to).with_context(|| {
            format!(
                "Failed to copy file from {} to {}",
                from.display(),
                to.display()
            )
        })
    }

    /// Remove a file with appropriate error context
    fn remove_file_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove file: {}", path.display()))
    }

    /// Remove a directory recursively with appropriate error context
    fn remove_dir_all_with_context(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory: {}", path.display()))
    }

    /// Check if a path exists, returning error if checking fails
    fn check_exists_with_context(path: impl AsRef<Path>) -> Result<bool> {
        let path = path.as_ref();
        path.try_exists()
            .with_context(|| format!("Failed to check if path exists: {}", path.display()))
    }
}

/// Implement FileOperations for a unit struct to enable trait usage
pub struct FileOps;
impl FileOperations for FileOps {}

/// Common manifest operations with consistent error handling
pub trait ManifestOperations {
    /// Load a manifest with appropriate error context
    fn load_manifest_with_context(path: impl AsRef<Path>) -> Result<Manifest> {
        let path = path.as_ref();
        Manifest::load(path)
            .with_context(|| format!("Failed to parse manifest file: {}", path.display()))
    }

    /// Save a manifest with appropriate error context
    fn save_manifest_with_context(manifest: &Manifest, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content =
            toml::to_string_pretty(manifest).with_context(|| "Failed to serialize manifest")?;
        FileOps::write_file_with_context(path, content)
    }
}

/// Implement ManifestOperations for a unit struct to enable trait usage
pub struct ManifestOps;
impl ManifestOperations for ManifestOps {}

/// Common markdown operations with consistent error handling
pub trait MarkdownOperations {
    /// Parse markdown content with appropriate error context
    fn parse_markdown_with_context(
        content: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Result<MarkdownFile> {
        let path = path.as_ref();
        MarkdownFile::parse(content.as_ref())
            .with_context(|| format!("Invalid markdown file: {}", path.display()))
    }

    /// Read and parse a markdown file with appropriate error context
    fn read_markdown_with_context(path: impl AsRef<Path>) -> Result<MarkdownFile> {
        let path = path.as_ref();
        let content = FileOps::read_file_with_context(path)?;
        Self::parse_markdown_with_context(content, path)
    }
}

/// Implement MarkdownOperations for a unit struct to enable trait usage
pub struct MarkdownOps;
impl MarkdownOperations for MarkdownOps {}

/// Common lockfile operations with consistent error handling
pub trait LockfileOperations {
    /// Load a lockfile with appropriate error context
    fn load_lockfile_with_context(path: impl AsRef<Path>) -> Result<crate::lockfile::LockFile> {
        let path = path.as_ref();
        crate::lockfile::LockFile::load(path)
            .with_context(|| format!("Failed to load lockfile: {}", path.display()))
    }

    /// Save a lockfile with appropriate error context
    fn save_lockfile_with_context(
        lockfile: &crate::lockfile::LockFile,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        lockfile
            .save(path)
            .with_context(|| format!("Failed to save lockfile: {}", path.display()))
    }
}

/// Implement LockfileOperations for a unit struct to enable trait usage
pub struct LockfileOps;
impl LockfileOperations for LockfileOps {}

/// Common JSON operations with consistent error handling
pub trait JsonOperations {
    /// Read and parse a JSON file with appropriate error context
    fn read_json_with_context<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
        let path = path.as_ref();
        let content = FileOps::read_file_with_context(path)?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON file: {}", path.display()))
    }

    /// Serialize and write a JSON file with appropriate error context
    fn write_json_with_context<T: serde::Serialize>(
        value: &T,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        let content =
            serde_json::to_string_pretty(value).with_context(|| "Failed to serialize to JSON")?;
        FileOps::write_file_with_context(path, content)
    }
}

/// Implement JsonOperations for a unit struct to enable trait usage
pub struct JsonOps;
impl JsonOperations for JsonOps {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_operations() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        // Test write and read
        FileOps::write_file_with_context(&file_path, "test content").unwrap();
        let content = FileOps::read_file_with_context(&file_path).unwrap();
        assert_eq!(content, "test content");

        // Test exists check
        assert!(FileOps::check_exists_with_context(&file_path).unwrap());

        // Test remove
        FileOps::remove_file_with_context(&file_path).unwrap();
        assert!(!FileOps::check_exists_with_context(&file_path).unwrap());
    }

    #[test]
    fn test_directory_operations() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("test_dir").join("nested");

        // Test create nested directories
        FileOps::create_dir_with_context(&dir_path).unwrap();
        assert!(dir_path.exists());

        // Test remove directory tree
        let parent = temp.path().join("test_dir");
        FileOps::remove_dir_all_with_context(&parent).unwrap();
        assert!(!parent.exists());
    }

    #[test]
    fn test_json_operations() {
        let temp = TempDir::new().unwrap();
        let json_path = temp.path().join("test.json");

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct TestStruct {
            field: String,
            number: i32,
        }

        let test_data = TestStruct {
            field: "test".to_string(),
            number: 42,
        };

        // Test write and read
        JsonOps::write_json_with_context(&test_data, &json_path).unwrap();
        let loaded: TestStruct = JsonOps::read_json_with_context(&json_path).unwrap();
        assert_eq!(loaded, test_data);
    }
}
